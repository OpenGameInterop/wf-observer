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
        &[0x49, 0x8d, 0x97],
        BLOCK,
        "Intrinsic block",
    )?;
    let call = SERIALIZER_FIELD
        .checked_add(10)
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
        Rva::new(0x0164_c0a1),
        &[0x48, 0x8d, 0x55, 4],
        "Intrinsic second pool",
    )?;
    require_bytes(
        &mut reader,
        Rva::new(0x0164_c0b2),
        &[0x48, 0x83, 0xc5, 8],
        "Intrinsic rank block",
    )?;
    require_bytes(
        &mut reader,
        Rva::new(0x0164_c128),
        &[
            0x48, 0xff, 0xc6, 0x48, 0x83, 0xc7, 0x10, 0x48, 0x83, 0xc5, 4, 0x48, 0x81, 0xff, 0xa0,
            0, 0, 0, 0x72, 0x84,
        ],
        "Intrinsic rank count",
    )?;
    // Native rank costs use thousandth-point units: Railjack costs 1000 << rank,
    // and Drifter's cost formula is multiplied by 5000 (five whole points).
    require_bytes(
        &mut reader,
        Rva::new(0x0177_85a8),
        &[0x8b, 0xca, 0xb8, 0xe8, 3, 0, 0, 0xd3, 0xe0],
        "Intrinsic point scale",
    )?;
    require_bytes(
        &mut reader,
        Rva::new(0x0177_861c),
        &[0x69, 0xc0, 0x88, 0x13, 0, 0],
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
            0x80, 0xfa, 9, 0x77, 0x25, 0x4c, 0x8d, 5, 0xe4, 0x5a, 0xf3, 0xfe, 0x0f, 0xb6, 0xc2,
            0x41, 0x8b, 0x84, 0x80, 0x44, 0xa5, 0x0c, 1, 0x49, 3, 0xc0, 0xff, 0xe0, 0xc6, 1, 0,
            0x48, 0x8b, 0xc1, 0xc3, 0xc6, 1, 1, 0x48, 0x8b, 0xc1, 0xc3, 0xcd, 0x2c, 0xc6, 1, 2,
            0x48, 0x8b, 0xc1, 0xc3,
        ],
        "Intrinsic pool classifier",
    )?;
    let mut table = Vec::new();
    for target in [
        0x010c_a53c_u32,
        0x010c_a52c,
        0x010c_a52c,
        0x010c_a52c,
        0x010c_a52c,
        0x010c_a52c,
        0x010c_a533,
        0x010c_a533,
        0x010c_a533,
        0x010c_a533,
    ] {
        table.extend(target.to_le_bytes());
    }
    require_bytes(reader, GROUP_TABLE, &table, "Intrinsic pool membership")?;
    // Ordered name references in the enum registration accompany values 0..9.
    for (index, &(name, text)) in SKILL_NAMES.iter().enumerate() {
        let reference = Rva::new(
            [
                0x0008_4f72,
                0x0008_4f84,
                0x0008_4f8f,
                0x0008_4f9a,
                0x0008_4fa5,
                0x0008_4fb0,
                0x0008_4fbb,
                0x0008_4fc6,
                0x0008_4fd1,
                0x0008_4fdc,
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
        Rva::new(0x0135_4b70),
        &[0x48, 0x8d, 0xbe],
        RANKS,
        "mastery Intrinsic ranks",
    )?;
    require_bytes(
        reader,
        Rva::new(0x0135_4b88),
        &[0xe8, 0x83, 0x59, 0xd7, 0xff, 0x0f, 0xb6, 8, 0x80, 0xf9, 2],
        "mastery Intrinsic grouping",
    )?;
    require_bytes(
        reader,
        Rva::new(0x0135_4b95),
        &[
            0x8b, 0xc1, 0x48, 0x8d, 0x4c, 0x24, 0x48, 0x48, 0x8d, 0x0c, 0x81, 0x8b, 0xc5, 0x0f,
            0xaf, 7, 1, 1,
        ],
        "mastery rank contributions",
    )?;
    require_bytes(
        reader,
        Rva::new(0x0135_4ba7),
        &[
            0x48, 0xff, 0xc3, 0x48, 0x83, 0xc7, 4, 0x48, 0x83, 0xfb, 10, 0x72, 0xcc,
        ],
        "mastery rank iteration",
    )?;
    require_offset(
        reader,
        Rva::new(0x0135_4bb7),
        &[0x4c, 0x8d, 0x9e],
        ObjectOffset::new(0xe30c),
        "Railjack mastery pool",
    )?;
    require_bytes(
        reader,
        Rva::new(0x0135_4be2),
        &[
            0x44, 0x0f, 0xaf, 0x86, 0xe4, 0x66, 1, 0, 0x44, 3, 0x44, 0x24, 0x48,
        ],
        "retained Railjack respec mastery",
    )?;
    require_offset(
        reader,
        Rva::new(0x0135_4c08),
        &[0x89, 0x96],
        ObjectOffset::new(0xe30c),
        "Railjack mastery write",
    )?;
    require_bytes(
        reader,
        Rva::new(0x0135_4c1f),
        &[0x4c, 0x8d, 0x8e, 0x14, 0xe3, 0, 0, 0x8b, 0x5c, 0x24, 0x4c],
        "Drifter mastery pool",
    )?;
    require_offset(
        reader,
        Rva::new(0x0135_4c4f),
        &[0x89, 0x8e],
        ObjectOffset::new(0xe314),
        "Drifter mastery write",
    )?;
    // The mission-progress calculator's result is passed to the first-pool writer.
    require_bytes(
        reader,
        Rva::new(0x00bc_bd70),
        &[0xe8, 0x8b, 0x73, 0x2f, 0],
        "mission mastery calculation",
    )?;
    require_bytes(
        reader,
        Rva::new(0x00bc_bd8b),
        &[0x48, 0x8d, 0x15, 0x8e, 0x92, 0x75, 1],
        "mission mastery log reference",
    )?;
    require_bytes(
        reader,
        Rva::new(0x0232_5020),
        b"Mission progress XP: \0",
        "mission mastery identity",
    )?;
    require_bytes(
        reader,
        Rva::new(0x00bc_bdba),
        &[0x8b, 0xd7, 0x48, 0x8b, 0xce, 0xe8, 0xec, 0xd1, 0x72, 0xff],
        "mission mastery pool write",
    )
}
