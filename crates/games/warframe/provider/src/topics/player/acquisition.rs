use memory_reader::ProcessMemory;
use provider_sdk::memory::{ReadError, TargetReader, read_stable};
use warframe_model::{PlayerName, PlayerSnapshot};

use super::facts::PLAYER;
use crate::{
    native_string::{player_name, read_string},
    roots::LoginIdentity,
    target::READ_LIMITS,
};

pub(crate) fn read_player(
    memory: &mut (impl ProcessMemory + ?Sized),
    base: u64,
    size: u32,
    login: &LoginIdentity,
) -> Result<PlayerSnapshot, ReadError> {
    let mut reader = TargetReader::new(memory, base, size, READ_LIMITS)?;
    let profile = login.profile.get();
    reader.require_vtable_slot_le64(profile, PLAYER.slot, PLAYER.getter, "player name getter")?;
    let username = read_stable(
        || {
            let preferred = read_string(&mut reader, profile, PLAYER.preferred, 128)?;
            let name = if preferred.is_empty() {
                read_string(&mut reader, profile, PLAYER.fallback, 128)?
            } else {
                preferred
            };
            PlayerName::new(player_name(&name)).map_err(|_| ReadError::invalid("player name"))
        },
        || ReadError::changed("player name"),
    )?;
    Ok(PlayerSnapshot {
        account_id: login.account_id.clone(),
        username,
    })
}
