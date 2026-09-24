use super::facts::{
    BLOCK, GROUP_CLASSIFIER, GROUP_TABLE, POOL_UPDATE, RANK_UPDATE, RANKS, SERIALIZER,
    SERIALIZER_FIELD, SKILL_NAMES,
};
use crate::{
    code::{require_bytes, require_offset},
    target::READ_LIMITS,
};
use memory_reader::ProcessMemory;
use provider_sdk::memory::{ObjectOffset, ReadError, Rva, TargetReader};

pub(crate) fn validate_intrinsics_layout(
    memory: &mut (impl ProcessMemory + ?Sized),
    base: u64,
    image_size: u32,
) -> Result<(), ReadError> {
    let mut reader = TargetReader::new(memory, base, image_size, READ_LIMITS)?;
    require_offset(
        &mut reader,
        SERIALIZER_FIELD,
        &[0x49, 0x8d, 0x94, 0x24],
        BLOCK,
        "Intrinsic block",
    )?;
    let call = SERIALIZER_FIELD
        .checked_add(11)
        .ok_or(ReadError::layout("Intrinsic serializer"))?;
    let bytes = reader.read_module_array::<5>(call)?;
    if bytes[0] != 0xe8 || call.rip_target(5, &bytes[1..]) != Some(SERIALIZER) {
        return Err(ReadError::layout("Intrinsic serializer call"));
    }
    require_offset(
        &mut reader,
        RANK_UPDATE,
        &[0x89, 0x84, 0x8b],
        RANKS,
        "Intrinsic rank update",
    )?;
    require_offset(
        &mut reader,
        POOL_UPDATE,
        &[0x44, 0x01, 0x84, 0x83],
        BLOCK,
        "Intrinsic balance update",
    )?;
    // The serializer emits pool[0], pool[1], then rank[0..10], at four-byte strides.
    require_bytes(
        &mut reader,
        Rva::new(0x0160_d6a1),
        &[0x48, 0x8d, 0x55, 0x04],
        "Intrinsic second pool",
    )?;
    require_bytes(
        &mut reader,
        Rva::new(0x0160_d6b2),
        &[0x48, 0x83, 0xc5, 0x08],
        "Intrinsic rank block",
    )?;
    require_bytes(
        &mut reader,
        Rva::new(0x0160_d728),
        &[
            0x48, 0xff, 0xc6, 0x48, 0x83, 0xc7, 0x10, 0x48, 0x83, 0xc5, 0x04, 0x48, 0x81, 0xff,
            0xa0, 0x00, 0x00, 0x00, 0x72, 0x84,
        ],
        "Intrinsic rank count",
    )?;
    // Native rank costs use thousandth-point units: Railjack costs 1000 << rank,
    // and Drifter's cost formula is multiplied by 5000 (five whole points).
    require_bytes(
        &mut reader,
        Rva::new(0x00f1_ca58),
        &[0x8b, 0xca, 0xb8, 0xe8, 0x03, 0x00, 0x00, 0xd3, 0xe0],
        "Intrinsic point scale",
    )?;
    require_bytes(
        &mut reader,
        Rva::new(0x00f1_cacc),
        &[0x69, 0xc0, 0x88, 0x13, 0x00, 0x00],
        "Drifter point scale",
    )?;
    validate_groups(&mut reader)
}

fn validate_groups(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<(), ReadError> {
    require_bytes(
        reader,
        GROUP_CLASSIFIER,
        &[
            0x80, 0xfa, 0x09, 0x77, 0x25, 0x4c, 0x8d, 0x05, 0x34, 0xd5, 0x02, 0xff, 0x0f, 0xb6,
            0xc2, 0x41, 0x8b, 0x84, 0x80, 0xf4, 0x2a, 0xfd, 0x00, 0x49, 0x03, 0xc0, 0xff, 0xe0,
            0xc6, 0x01, 0x00, 0x48, 0x8b, 0xc1, 0xc3, 0xc6, 0x01, 0x01, 0x48, 0x8b, 0xc1, 0xc3,
            0xcd, 0x2c, 0xc6, 0x01, 0x02, 0x48, 0x8b, 0xc1, 0xc3,
        ],
        "Intrinsic pool classifier",
    )?;
    let mut table = Vec::new();
    for target in [
        0x00fd_2aec_u32,
        0x00fd_2adc,
        0x00fd_2adc,
        0x00fd_2adc,
        0x00fd_2adc,
        0x00fd_2adc,
        0x00fd_2ae3,
        0x00fd_2ae3,
        0x00fd_2ae3,
        0x00fd_2ae3,
    ] {
        table.extend(target.to_le_bytes());
    }
    require_bytes(reader, GROUP_TABLE, &table, "Intrinsic pool membership")?;
    // Ordered name references in the enum registration accompany values 0..9.
    for (index, &(name, text)) in SKILL_NAMES.iter().enumerate() {
        let reference = Rva::new(
            [
                0x0008_5912,
                0x0008_5924,
                0x0008_592f,
                0x0008_593a,
                0x0008_5945,
                0x0008_5950,
                0x0008_595b,
                0x0008_5966,
                0x0008_5971,
                0x0008_597c,
            ][index],
        );
        let code = reader.read_module_array::<7>(reference)?;
        if code[..3] != [0x48, 0x8d, 5]
            || reference.rip_target(7, &code[3..]) != Some(Rva::new(name))
        {
            return Err(ReadError::layout("Intrinsic rank identity"));
        }
        let mut expected = text.as_bytes().to_vec();
        expected.push(0);
        require_bytes(reader, Rva::new(name), &expected, "Intrinsic rank name")?;
    }
    Ok(())
}

/// Mastery depends on these source identities without demanding Intrinsics data.
pub(crate) fn validate_mastery_sources(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<(), ReadError> {
    validate_groups(reader)?;
    require_offset(
        reader,
        Rva::new(0x0182_1ed0),
        &[0x48, 0x8d, 0xbe],
        RANKS,
        "mastery Intrinsic ranks",
    )?;
    require_bytes(
        reader,
        Rva::new(0x0182_1ee8),
        &[
            0xe8, 0xd3, 0x0b, 0x7b, 0xff, 0x0f, 0xb6, 0x08, 0x80, 0xf9, 0x02,
        ],
        "mastery Intrinsic grouping",
    )?;
    require_bytes(
        reader,
        Rva::new(0x0182_1ef5),
        &[
            0x8b, 0xc1, 0x48, 0x8d, 0x4c, 0x24, 0x48, 0x48, 0x8d, 0x0c, 0x81, 0x8b, 0xc5, 0x0f,
            0xaf, 0x07, 0x01, 0x01,
        ],
        "mastery rank contributions",
    )?;
    require_bytes(
        reader,
        Rva::new(0x0182_1f07),
        &[
            0x48, 0xff, 0xc3, 0x48, 0x83, 0xc7, 0x04, 0x48, 0x83, 0xfb, 0x0a, 0x72, 0xcc,
        ],
        "mastery rank iteration",
    )?;
    require_offset(
        reader,
        Rva::new(0x0182_1f17),
        &[0x4c, 0x8d, 0x9e],
        ObjectOffset::new(0xe4cc),
        "Railjack mastery pool",
    )?;
    require_bytes(
        reader,
        Rva::new(0x0182_1f2f),
        &[0x44, 0x0f, 0xaf, 0x86, 0xac, 0x70, 0x01, 0x00],
        "retained Railjack respec mastery",
    )?;
    require_bytes(
        reader,
        Rva::new(0x0182_1f4a),
        &[0x44, 0x03, 0x44, 0x24, 0x48],
        "retained and current Railjack mastery sum",
    )?;
    require_offset(
        reader,
        Rva::new(0x0182_1f68),
        &[0x89, 0x8e],
        ObjectOffset::new(0xe4cc),
        "Railjack mastery write",
    )?;
    require_bytes(
        reader,
        Rva::new(0x0182_1f7f),
        &[
            0x4c, 0x8d, 0x8e, 0xd4, 0xe4, 0x00, 0x00, 0x8b, 0x5c, 0x24, 0x4c,
        ],
        "Drifter mastery pool",
    )?;
    require_offset(
        reader,
        Rva::new(0x0182_1faf),
        &[0x89, 0x96],
        ObjectOffset::new(0xe4d4),
        "Drifter mastery write",
    )?;
    // The mission-progress calculator's result is passed to the first-pool writer.
    require_bytes(
        reader,
        Rva::new(0x010a_6a58),
        &[0xe8, 0xb3, 0xea, 0x7e, 0xff],
        "mission mastery calculation",
    )?;
    require_bytes(
        reader,
        Rva::new(0x010a_6a73),
        &[0x48, 0x8d, 0x15, 0x06, 0xc6, 0x24, 0x01],
        "mission mastery log reference",
    )?;
    require_bytes(
        reader,
        Rva::new(0x022f_3080),
        b"Mission progress XP: \0",
        "mission mastery identity",
    )?;
    require_bytes(
        reader,
        Rva::new(0x010a_6aa5),
        &[0x8b, 0xd7, 0x48, 0x8b, 0xce, 0xe8, 0x21, 0xd7, 0x95, 0xff],
        "mission mastery pool write",
    )
}
