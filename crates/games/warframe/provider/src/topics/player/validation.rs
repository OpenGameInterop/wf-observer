use memory_reader::ProcessMemory;
use provider_sdk::memory::{ReadError, TargetReader};

use super::facts::PLAYER;
use crate::target::READ_LIMITS;

pub(crate) fn validate_player_layout(
    memory: &mut (impl ProcessMemory + ?Sized),
    base: u64,
    size: u32,
) -> Result<(), ReadError> {
    let mut reader = TargetReader::new(memory, base, size, READ_LIMITS)?;
    let fallback =
        u8::try_from(PLAYER.fallback.get()).map_err(|_| ReadError::layout("player name"))?;
    let preferred =
        u8::try_from(PLAYER.preferred.get()).map_err(|_| ReadError::layout("player name"))?;
    let marker = preferred
        .checked_add(15)
        .ok_or_else(|| ReadError::layout("player name"))?;
    // The getter checks both external and inline lengths before choosing the fallback.
    if reader.read_module_array::<64>(PLAYER.getter)?
        != [
            0x84, 0xd2, 0x75, 0x37, 0x0f, 0xb6, 0x41, marker, 0x48, 0x8d, 0x51, preferred, 0x3c,
            0xff, 0x75, 0x19, 0x8b, 0x42, 0x08, 0x48, 0xa9, 0xff, 0xff, 0xff, 0x0f, 0x48, 0x8d,
            0x41, fallback, 0x41, 0x0f, 0x94, 0xc0, 0x45, 0x84, 0xc0, 0x48, 0x0f, 0x44, 0xc2, 0xc3,
            0x3c, 0x0f, 0x48, 0x8d, 0x41, fallback, 0x41, 0x0f, 0x94, 0xc0, 0x45, 0x84, 0xc0, 0x48,
            0x0f, 0x44, 0xc2, 0xc3, 0x48, 0x8d, 0x41, fallback, 0xc3,
        ]
    {
        return Err(ReadError::layout("player name getter"));
    }
    Ok(())
}
