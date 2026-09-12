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
        || vector[..3] != [0x4d, 0x8d, 0xae]
        || vector[3..] != CHAT.channels.get().to_le_bytes()
        || reader.read_module_array::<4>(CHAT.retention_check)? != [0x48, 0x83, 0xf8, retention]
    {
        return Err(ReadError::layout("chat history registration"));
    }
    let fields = reader.read_module_array::<56>(CHAT.entry_fields)?;
    for (index, offset) in [(0_u8, CHAT.sender), (14, CHAT.text), (28, CHAT.timestamp)] {
        let offset = displacement(offset.get())?;
        let call = CHAT
            .entry_fields
            .checked_add(u32::from(index) + 4)
            .ok_or_else(|| ReadError::layout("chat string copy"))?;
        let index = usize::from(index);
        if fields[index..index + 5] != [0x48, 0x8d, 0x4b, offset, 0xe8]
            || call.rip_target(5, &fields[index + 5..index + 9]) != Some(CHAT.string_copy)
        {
            return Err(ReadError::layout("chat entry strings"));
        }
    }
    if fields[53..] != [0x89, 0x43, displacement(CHAT.flags.get())?] {
        return Err(ReadError::layout("chat entry flags"));
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
            0x77,
            displacement(CHAT.channel_name.get())?,
        ]
        || reader.read_module_array::<4>(CHAT.channel_entries_reference)?
            != [0x48, 0x8d, 0x77, displacement(CHAT.channel_entries.get())?]
    {
        return Err(ReadError::layout("chat channel fields"));
    }
    Ok(())
}
