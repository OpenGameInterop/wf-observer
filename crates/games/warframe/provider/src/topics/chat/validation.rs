use memory_reader::ProcessMemory;
use provider_sdk::memory::{ReadError, TargetReader};

use super::facts::{CHAT, MAX_ENTRIES};
use crate::target::READ_LIMITS;

pub(crate) fn validate_chat_layout(
    memory: &mut (impl ProcessMemory + ?Sized),
    base: u64,
    size: u32,
) -> Result<(), ReadError> {
    let mut reader = TargetReader::new(memory, base, size, READ_LIMITS)?;
    let displacement = |value| {
        i8::try_from(value)
            .map(i8::cast_unsigned)
            .map_err(|_| ReadError::layout("chat instruction displacement"))
    };
    let retention = u8::try_from(MAX_ENTRIES).map_err(|_| ReadError::layout("chat retention"))?;
    let call = reader.read_module_array::<5>(CHAT.insert_call)?;
    let vector = reader.read_module_array::<7>(CHAT.channels_reference)?;
    if call[0] != 0xe8
        || CHAT.insert_call.rip_target(5, &call[1..]) != Some(CHAT.insert)
        || vector[..3] != [0x4c, 0x8d, 0xae]
        || vector[3..] != CHAT.channels.get().to_le_bytes()
        || reader.read_module_array::<4>(CHAT.retention_check)? != [0x48, 0x83, 0xf8, retention]
    {
        return Err(ReadError::layout("chat history registration"));
    }
    let fields = reader.read_module_array::<59>(CHAT.entry_fields)?;
    // RSI holds entry + sender; the remaining fields are relative to RSI.
    if fields[..4] != [0x48, 0x8d, 0x73, displacement(CHAT.sender.get())?]
        || fields[14..17] != [0x48, 0x8b, 0xce]
    {
        return Err(ReadError::layout("chat entry strings"));
    }
    for (index, offset) in [(27, CHAT.text), (41, CHAT.timestamp)] {
        let relative = offset
            .get()
            .checked_sub(CHAT.sender.get())
            .ok_or_else(|| ReadError::layout("chat entry strings"))?;
        if fields[index..index + 4] != [0x48, 0x8d, 0x4e, displacement(relative)?] {
            return Err(ReadError::layout("chat entry strings"));
        }
    }
    for index in [17_u8, 31, 45] {
        let call = CHAT
            .entry_fields
            .checked_add(u32::from(index))
            .ok_or_else(|| ReadError::layout("chat string copy"))?;
        let index = usize::from(index);
        if fields[index] != 0xe8
            || call.rip_target(5, &fields[index + 1..index + 5]) != Some(CHAT.string_copy)
        {
            return Err(ReadError::layout("chat entry strings"));
        }
    }
    let role = CHAT
        .role
        .get()
        .checked_sub(CHAT.sender.get())
        .ok_or_else(|| ReadError::layout("chat entry role"))?;
    if fields[56..] != [0x89, 0x46, displacement(role)?] {
        return Err(ReadError::layout("chat entry role"));
    }
    // Channel name selection and entry-list selection precede the entry writes.
    if reader.read_module_array::<9>(CHAT.channel_name_reference)?
        != [
            0x48,
            0x0f,
            0xbe,
            0x47,
            0x27,
            0x48,
            0x8d,
            0x5f,
            displacement(CHAT.channel_name.get())?,
        ]
        || reader.read_module_array::<4>(CHAT.channel_entries_reference)?
            != [0x48, 0x83, 0xc7, displacement(CHAT.channel_entries.get())?]
    {
        return Err(ReadError::layout("chat channel fields"));
    }
    validate_channel_keys(&mut reader)
}

fn validate_channel_keys(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<(), ReadError> {
    let public = reader.read_module_array::<7>(CHAT.public_prefix_reference)?;
    let prefix = CHAT
        .public_prefix_reference
        .rip_target(7, &public[3..])
        .ok_or_else(|| ReadError::layout("chat public prefix"))?;
    if public[..3] != [0x48, 0x8d, 0x15] || reader.read_module_array::<2>(prefix)? != *b"#\0" {
        return Err(ReadError::layout("chat public prefix"));
    }
    let private = reader.read_module_array::<36>(CHAT.private_key_reference)?;
    let separator = CHAT
        .private_key_reference
        .rip_target(12, &private[8..12])
        .ok_or_else(|| ReadError::layout("chat private separator"))?;
    if private[..8] != [0x48, 0x8b, 0x54, 0x24, 0x28, 0x4c, 0x8d, 0x05]
        || reader.read_module_array::<2>(separator)? != *b",\0"
        || private[12..16] != [0x48, 0x8d, 0x4d, 0xc0]
        || private[21..31] != [0x48, 0x8b, 0xd0, 0x48, 0x8d, 0x4d, 0xd0, 0x4d, 0x8b, 0xc7]
    {
        return Err(ReadError::layout("chat private participants"));
    }
    for (index, target) in [(16_u8, CHAT.append_separator), (31, CHAT.append_recipient)] {
        let call = CHAT
            .private_key_reference
            .checked_add(u32::from(index))
            .ok_or_else(|| ReadError::layout("chat private append"))?;
        let index = usize::from(index);
        if private[index] != 0xe8
            || call.rip_target(5, &private[index + 1..index + 5]) != Some(target)
        {
            return Err(ReadError::layout("chat private append"));
        }
    }
    Ok(())
}
