use memory_reader::ProcessMemory;
use provider_sdk::memory::{ReadError, TargetReader};

use crate::target::READ_LIMITS;

use super::facts::{
    CODEC, CREDITS, ENDO, EncodedNameFacts, NON_TRADABLE_PLATINUM, PLATINUM_SERIALIZER,
    TRADABLE_PLATINUM,
};

/// Checks the four balance fields and their encodings without following account data.
pub(crate) fn validate_currencies_layout(
    memory: &mut (impl ProcessMemory + ?Sized),
    module_base: u64,
    image_size: u32,
) -> Result<(), ReadError> {
    let mut reader = TargetReader::new(memory, module_base, image_size, READ_LIMITS)?;
    if CODEC.check.get() != 0
        || CODEC.stored.get() != 4
        || CODEC.rotate_left >= u32::BITS
        || CODEC.address_shift >= u64::BITS
    {
        return Err(ReadError::layout("currency codec"));
    }
    let rotate =
        u8::try_from(CODEC.rotate_left).map_err(|_| ReadError::layout("currency rotate"))?;
    let shift =
        u8::try_from(CODEC.address_shift).map_err(|_| ReadError::layout("currency shift"))?;
    for field in [CREDITS, TRADABLE_PLATINUM, NON_TRADABLE_PLATINUM] {
        validate_name(&mut reader, field.name)?;
    }
    validate_credits(&mut reader, rotate, shift)?;
    validate_endo(&mut reader, rotate, shift)?;
    validate_platinum(&mut reader, rotate, shift)
}

fn validate_name(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: EncodedNameFacts,
) -> Result<(), ReadError> {
    let reference = reader.read_module_array::<7>(facts.reference)?;
    let key = reader.read_module_array::<10>(facts.key_reference)?;
    if reference[..3] != [0x48, 0x8d, 0x15]
        || facts.reference.rip_target(7, &reference[3..]) != Some(facts.object)
        || key[..2] != [0x49, 0xb8]
        || key[2..] != facts.key.to_le_bytes()
    {
        return Err(ReadError::layout("currency field name reference"));
    }
    // The header gives a count of 16-byte blocks. Name bytes start after the header.
    let header = reader.read_module_array::<10>(facts.object)?;
    let blocks = usize::from(header[9] >> 4);
    if !(1..=4).contains(&blocks) {
        return Err(ReadError::layout("currency field name length"));
    }
    let data = facts
        .object
        .checked_add(16)
        .ok_or_else(|| ReadError::layout("currency field name"))?;
    let mut decoded = [0; 64];
    let decoded = &mut decoded[..blocks * 16];
    reader.read_module(data, decoded)?;
    let key = facts.key.to_le_bytes();
    for (index, byte) in decoded.iter_mut().enumerate() {
        *byte ^= key[index % key.len()];
    }
    if decoded.get(..facts.text.len()) != Some(facts.text.as_bytes())
        || decoded.get(facts.text.len()) != Some(&0)
    {
        return Err(ReadError::layout("currency field name"));
    }
    Ok(())
}

fn validate_credits(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    rotate: u8,
    shift: u8,
) -> Result<(), ReadError> {
    let facts = CREDITS.balance;
    let stored = facts
        .field
        .get()
        .checked_add(CODEC.stored.get())
        .ok_or_else(|| ReadError::layout("credits field"))?
        .to_le_bytes();
    let code = reader.read_module_array::<162>(facts.reader)?;
    if code[..3] != [0x41, 0x8b, 0x87]
        || code[3..7] != stored
        || code[7..10] != [0x49, 0x8d, 0x9f]
        || code[10..14] != stored
        || code[14..17] != [0xc1, 0xc0, rotate]
        || code[37..41] != [0x48, 0xc1, 0xfb, shift]
        || code[47..50] != [0x48, 0x81, 0xf1]
        || code[50..54] != CODEC.value_xor.to_le_bytes()
        || code[133..135] != [0x81, 0xf2]
        || code[135..139] != CODEC.value_xor.to_le_bytes()
        || code[139..142] != [0xc1, 0xca, rotate]
        || code[142..145] != [0x41, 0x89, 0x97]
        || code[145..149] != stored
        || code[149..151] != [0x81, 0xf2]
        || code[151..155] != CODEC.check_xor.to_le_bytes()
        || code[155..158] != [0x41, 0x89, 0x97]
        || code[158..162] != facts.field.get().to_le_bytes()
    {
        return Err(ReadError::layout("credits codec"));
    }
    Ok(())
}

fn validate_endo(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    rotate: u8,
    shift: u8,
) -> Result<(), ReadError> {
    let stored = ENDO
        .field
        .get()
        .checked_add(CODEC.stored.get())
        .ok_or_else(|| ReadError::layout("endo field"))?
        .to_le_bytes();
    let code = reader.read_module_array::<96>(ENDO.reader)?;
    if code[..3] != [0x49, 0x8d, 0x8e]
        || code[3..7] != stored
        || code[13..17] != [0x48, 0xc1, 0xf9, shift]
        || code[40..43] != [0x41, 0x8b, 0x96]
        || code[43..47] != stored
        || code[47..50] != [0xc1, 0xc2, rotate]
        || code[50..52] != [0x33, 0xd1]
        || code[52..54] != [0x81, 0xf2]
        || code[54..58] != CODEC.value_xor.to_le_bytes()
        || code[65..67] != [0x33, 0xd1]
        || code[67..69] != [0x81, 0xf2]
        || code[69..73] != CODEC.value_xor.to_le_bytes()
        || code[73..76] != [0xc1, 0xca, rotate]
        || code[76..79] != [0x41, 0x89, 0x96]
        || code[79..83] != stored
        || code[83..85] != [0x81, 0xf2]
        || code[85..89] != CODEC.check_xor.to_le_bytes()
        || code[89..92] != [0x41, 0x89, 0x96]
        || code[92..96] != ENDO.field.get().to_le_bytes()
    {
        return Err(ReadError::layout("endo codec"));
    }
    Ok(())
}

fn validate_platinum(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    rotate: u8,
    shift: u8,
) -> Result<(), ReadError> {
    for field in [TRADABLE_PLATINUM.balance, NON_TRADABLE_PLATINUM.balance] {
        let reference = reader.read_module_array::<17>(field.reader)?;
        let call = field
            .reader
            .checked_add(12)
            .ok_or_else(|| ReadError::layout("platinum serializer call"))?;
        if reference[..3] != [0x49, 0x8d, 0x8f]
            || reference[3..7] != field.field.get().to_le_bytes()
            || reference[7..12] != [0x48, 0x8d, 0x54, 0x24, 0x60]
            || reference[12] != 0xe8
            || call.rip_target(5, &reference[13..]) != Some(PLATINUM_SERIALIZER)
        {
            return Err(ReadError::layout("platinum field reference"));
        }
    }
    let code = reader.read_module_array::<101>(PLATINUM_SERIALIZER)?;
    if code[20..24] != [0x48, 0x8d, 0x79, 4]
        || code[24..27] != [0x4c, 0x8b, 0xf1]
        || code[74..77] != [0x48, 0x8b, 0xc7]
        || code[77..81] != [0x48, 0xc1, 0xf8, shift]
        || code[81..83] != [0x33, 0x07]
        || code[83] != 0x35
        || code[84..88] != CODEC.value_xor.to_le_bytes()
        || code[88..91] != [0xc1, 0xc8, rotate]
        || code[91..93] != [0x89, 0x07]
        || code[93] != 0x35
        || code[94..98] != CODEC.check_xor.to_le_bytes()
        || code[98..101] != [0x41, 0x89, 0x06]
    {
        return Err(ReadError::layout("platinum codec"));
    }
    Ok(())
}
