//! Validates the calculation semantics, inventory relation and both sides of the codecs.
use super::facts::{MASTERY, MasteryFacts};
use crate::{profile_inventory::INVENTORY_OWNER, target::READ_LIMITS};
use memory_reader::ProcessMemory;
use provider_sdk::memory::{ObjectOffset, ReadError, Rva, TargetReader};

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;

const CALCULATION_NAME: &[u8] = b"Calculating (item based) player XP\n\0";

pub(crate) fn validate_mastery_layout(
    memory: &mut (impl ProcessMemory + ?Sized),
    module_base: u64,
    image_size: u32,
) -> Result<(), ReadError> {
    let mut reader = TargetReader::new(memory, module_base, image_size, READ_LIMITS)?;
    validate_calculation_name(&mut reader, MASTERY)?;
    validate_calculation_reader(&mut reader, MASTERY)?;
    validate_non_item_xp_reader(&mut reader, MASTERY, INVENTORY_OWNER.offset)?;
    crate::topics::intrinsics::validate_mastery_sources(&mut reader)?;
    validate_rank_reader(&mut reader, MASTERY)?;
    validate_serializer(&mut reader, MASTERY)
}

fn validate_calculation_name(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: MasteryFacts,
) -> Result<(), ReadError> {
    let start = facts
        .calculation_name_reference
        .get()
        .checked_sub(6)
        .map(Rva::new)
        .ok_or(ReadError::layout("calculation name"))?;
    let mut code = [0_u8; 13];
    reader.read_module(start, &mut code)?;
    if code[..2] != [0x41, 0xb8]
        || code[2..6] != 35_u32.to_le_bytes()
        || code[6..9] != [0x48, 0x8d, 0x15]
        || facts.calculation_name_reference.rip_target(7, &code[9..])
            != Some(facts.calculation_name)
    {
        return Err(ReadError::layout("calculation name reference"));
    }
    let mut name = [0_u8; CALCULATION_NAME.len()];
    reader.read_module(facts.calculation_name, &mut name)?;
    if name != CALCULATION_NAME {
        return Err(ReadError::layout("calculation name"));
    }
    Ok(())
}

fn validate_calculation_reader(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: MasteryFacts,
) -> Result<(), ReadError> {
    let vector_size = facts
        .vector
        .get()
        .checked_add(8)
        .ok_or(ReadError::layout("mastery vector"))?;
    let record_bytes =
        u8::try_from(facts.record_bytes).map_err(|_| ReadError::layout("mastery record"))?;
    let stored =
        u8::try_from(facts.xp.stored.get()).map_err(|_| ReadError::layout("mastery record"))?;
    let rotate =
        u8::try_from(facts.xp.rotate_left).map_err(|_| ReadError::layout("mastery XP codec"))?;
    let shift =
        u8::try_from(facts.xp.address_shift).map_err(|_| ReadError::layout("mastery XP codec"))?;
    let mut code = [0_u8; 327];
    reader.read_module(facts.calculation_reader, &mut code)?;
    if facts.calculation.get().checked_add(44) != Some(facts.calculation_reader.get())
        || facts.item_type.get() != 0
        || code[..2] != [0x80, 0xbf]
        || code[2..6] != facts.item_xp_dirty.get().to_le_bytes()
        || code[6] != 0
        || code[72..74] != [0xc7, 0x87]
        || code[74..78] != facts.item_xp.get().to_le_bytes()
        || code[78..82] != [0, 0, 0, 0]
        || code[82..85] != [0x48, 0x8d, 0xb7]
        || code[85..89] != facts.item_xp.get().to_le_bytes()
        || code[89..92] != [0x48, 0x8b, 0x9f]
        || code[92..96] != facts.vector.get().to_le_bytes()
        || code[96..98] != [0x8b, 0x87]
        || code[98..102] != vector_size.to_le_bytes()
        || code[119..122] != [0x4c, 0x8d, 0x73]
        || code[122] != stored
        || code[123..127] != [0x48, 0x83, 0x3b, 0]
        || code[136..139] != [0x41, 0x8b, 0x16]
        || code[139..142] != [0xc1, 0xc2, rotate]
        || code[145..148] != [0x49, 0x8b, 0xc6]
        || code[148..152] != [0x48, 0xc1, 0xf8, shift]
        || code[152..156] != [0x33, 0xd0, 0x81, 0xf2]
        || code[156..160] != facts.xp.value_xor.to_le_bytes()
        || code[197..199] != [0x01, 0x87]
        || code[199..203] != facts.item_xp.get().to_le_bytes()
        || code[209..212] != [0x48, 0x83, 0xc3]
        || code[212] != record_bytes
        || code[220..223] != [0x49, 0x83, 0xc6]
        || code[223] != record_bytes
        || code[279..281] != [0x8b, 0x97]
        || code[281..285] != facts.item_xp.get().to_le_bytes()
        || code[320..322] != [0xc6, 0x87]
        || code[322..326] != facts.item_xp_dirty.get().to_le_bytes()
        || code[326] != 0
    {
        return Err(ReadError::layout("mastery calculation reader"));
    }
    Ok(())
}

fn validate_non_item_xp_reader(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: MasteryFacts,
    profile_inventory: ObjectOffset,
) -> Result<(), ReadError> {
    let codec = facts.non_item_xp_codec;
    let stored = codec.stored.get();
    let pools = facts
        .non_item_xp
        .map(|field| field.get().saturating_add(stored));
    let rotate =
        u8::try_from(codec.rotate_left).map_err(|_| ReadError::layout("non-item XP reader"))?;
    let shift =
        u8::try_from(codec.address_shift).map_err(|_| ReadError::layout("non-item XP reader"))?;
    let mut code = [0_u8; 125];
    reader.read_module(facts.non_item_xp_reader, &mut code)?;
    let jump = facts
        .non_item_xp_reader
        .checked_add(120)
        .ok_or(ReadError::layout("non-item XP reader"))?;
    if facts.non_item_xp[1].get() != facts.non_item_xp[0].get().saturating_add(8)
        || facts.non_item_xp[2].get() != facts.non_item_xp[1].get().saturating_add(8)
        || code[..3] != [0x4c, 0x8d, 0x89]
        || code[3..7] != pools[0].to_le_bytes()
        || code[10..14] != [0x49, 0xc1, 0xf9, shift]
        || code[17..19] != [0x33, 0xc2]
        || code[19] != 0x35
        || code[20..24] != codec.value_xor.to_le_bytes()
        || code[24..27] != [0xc1, 0xc8, rotate]
        || code[27..29] != [0x89, 0x81]
        || code[29..33] != pools[0].to_le_bytes()
        || code[33] != 0x35
        || code[34..38] != codec.check_xor.to_le_bytes()
        || code[38..40] != [0x89, 0x81]
        || code[40..44] != facts.non_item_xp[0].get().to_le_bytes()
        || code[44..47] != [0x48, 0x8d, 0x81]
        || code[47..51] != pools[1].to_le_bytes()
        || code[53..56] != [0x48, 0x81, 0xc1]
        || code[56..60] != pools[2].to_le_bytes()
        || code[60..64] != [0x48, 0xc1, 0xf8, shift]
        || code[64..67] != [0xc1, 0xc2, rotate]
        || code[67..69] != [0x33, 0xd0]
        || code[71..73] != [0x81, 0xf2]
        || code[73..77] != codec.value_xor.to_le_bytes()
        || code[77..80] != [0xc1, 0xc0, rotate]
        || code[80..84] != [0x48, 0xc1, 0xf9, shift]
        || code[84..86] != [0x33, 0xc1]
        || code[86..89] != [0x49, 0x8d, 0x8a]
        || code[89..93] != profile_inventory.get().to_le_bytes()
        || code[93] != 0x35
        || code[94..98] != codec.value_xor.to_le_bytes()
        || code[98..100] != [0x03, 0xd0]
        || code[100..103] != [0x41, 0x8b, 0x82]
        || code[103..107] != pools[0].to_le_bytes()
        || code[107..110] != [0xc1, 0xc0, rotate]
        || code[110..113] != [0x41, 0x33, 0xc1]
        || code[113] != 0x35
        || code[114..118] != codec.value_xor.to_le_bytes()
        || code[118..120] != [0x03, 0xd0]
        || code[120] != 0xe9
        || jump.rip_target(5, &code[121..125]) != Some(facts.calculation)
    {
        return Err(ReadError::layout("non-item XP reader"));
    }
    Ok(())
}

fn validate_rank_reader(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: MasteryFacts,
) -> Result<(), ReadError> {
    let stored = facts
        .rank
        .get()
        .checked_add(facts.rank_codec.stored.get())
        .ok_or(ReadError::layout("mastery rank reader"))?;
    let check = facts
        .rank
        .get()
        .checked_add(facts.rank_codec.check.get())
        .ok_or(ReadError::layout("mastery rank reader"))?;
    let rotate = u8::try_from(facts.rank_codec.rotate_left)
        .map_err(|_| ReadError::layout("mastery rank reader"))?;
    let shift = u8::try_from(facts.rank_codec.address_shift)
        .map_err(|_| ReadError::layout("mastery rank reader"))?;
    let mut code = [0_u8; 119];
    reader.read_module(facts.rank_reader, &mut code)?;
    if code[..3] != [0x48, 0x8d, 0x97]
        || code[3..7] != stored.to_le_bytes()
        || code[12] != 0xbd
        || code[13..17] != u32::from(facts.rank_codec.value_xor).to_le_bytes()
        || code[17..21] != [0x48, 0xc1, 0xfa, shift]
        || code[26..29] != [0x0f, 0xb7, 0x97]
        || code[29..33] != stored.to_le_bytes()
        || code[33..37] != [0x66, 0xc1, 0xc2, rotate]
        || code[41..44] != [0x44, 0x33, 0xc3]
        || code[44..47] != [0x44, 0x33, 0xc5]
        || code[87..90] != [0x66, 0x33, 0xde]
        || code[90] != 0xb8
        || code[91..95] != u32::from(facts.rank_codec.check_xor).to_le_bytes()
        || code[95..98] != [0x66, 0x33, 0xdd]
        || code[98..102] != [0x66, 0xc1, 0xcb, rotate]
        || code[102..105] != [0x66, 0x89, 0x9f]
        || code[105..109] != stored.to_le_bytes()
        || code[109..112] != [0x66, 0x33, 0xd8]
        || code[112..115] != [0x66, 0x89, 0x9f]
        || code[115..119] != check.to_le_bytes()
    {
        return Err(ReadError::layout("mastery rank reader"));
    }
    Ok(())
}

fn validate_serializer(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: MasteryFacts,
) -> Result<(), ReadError> {
    let stored =
        u8::try_from(facts.xp.stored.get()).map_err(|_| ReadError::layout("mastery serializer"))?;
    let check =
        u8::try_from(facts.xp.check.get()).map_err(|_| ReadError::layout("mastery serializer"))?;
    let rotate =
        u8::try_from(facts.xp.rotate_left).map_err(|_| ReadError::layout("mastery serializer"))?;
    let shift = u8::try_from(facts.xp.address_shift)
        .map_err(|_| ReadError::layout("mastery serializer"))?;
    let mut code = [0_u8; 86];
    reader.read_module(facts.serializer, &mut code)?;
    if code[..2] != [0x8b, 0x42]
        || code[2] != stored
        || code[3..6] != [0x48, 0x8d, 0x5a]
        || code[6] != stored
        || code[7..10] != [0xc1, 0xc0, rotate]
        || code[13..17] != [0x48, 0xc1, 0xfb, shift]
        || code[24] != 0x35
        || code[25..29] != facts.xp.value_xor.to_le_bytes()
        || code[42..44] != [0x8b, 0x47]
        || code[44] != stored
        || code[49..52] != [0xc1, 0xc0, rotate]
        || code[54] != 0x35
        || code[55..59] != facts.xp.value_xor.to_le_bytes()
        || code[65..67] != [0x81, 0xf3]
        || code[67..71] != facts.xp.value_xor.to_le_bytes()
        || code[71..74] != [0xc1, 0xcb, rotate]
        || code[74..76] != [0x89, 0x5f]
        || code[76] != stored
        || code[77..79] != [0x81, 0xf3]
        || code[79..83] != facts.xp.check_xor.to_le_bytes()
        || code[83..85] != [0x89, 0x5f]
        || code[85] != check
    {
        return Err(ReadError::layout("mastery serializer"));
    }
    Ok(())
}
