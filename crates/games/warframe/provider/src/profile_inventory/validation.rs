//! Executable evidence for the embedded inventory and its commit markers.
use super::INVENTORY_OWNER;
use crate::roots::LOGIN;
use memory_reader::ProcessMemory;
use provider_sdk::memory::{ReadError, TargetReader};

pub(crate) fn validate_layout(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<(), ReadError> {
    let getter = reader.read_module_array::<8>(INVENTORY_OWNER.getter)?;
    if getter[..3] != [0x48, 0x8d, 0x81]
        || getter[3..7] != INVENTORY_OWNER.offset.get().to_le_bytes()
        || getter[7] != 0xc3
    {
        return Err(ReadError::layout("inventory getter"));
    }
    validate_commit_fields(reader)
}

fn validate_commit_fields(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
) -> Result<(), ReadError> {
    let facts = INVENTORY_OWNER.commit;
    let constructor = reader.read_module_array::<14>(facts.constructor_vtable)?;
    // lea rax, [profile-data vtable]; lea rcx, [rdi+28h]; mov [rdi], rax.
    if constructor[..3] != [0x48, 0x8d, 0x05]
        || facts.constructor_vtable.rip_target(7, &constructor[3..7])
            != Some(LOGIN.profile_data.vtable)
        || constructor[7..] != [0x48, 0x8d, 0x4f, 0x28, 0x48, 0x89, 0x07]
    {
        return Err(ReadError::layout("inventory coherence owner"));
    }
    let init = reader.read_module_array::<7>(facts.force_update_init)?;
    let read = reader.read_module_array::<7>(facts.force_update_read)?;
    // The same profile-data byte is initialized and read as an unsigned byte.
    if init[..3] != [0x44, 0x88, 0xa7]
        || init[3..] != INVENTORY_OWNER.force_update.get().to_le_bytes()
        || read[..3] != [0x0f, 0xb6, 0x90]
        || read[3..] != INVENTORY_OWNER.force_update.get().to_le_bytes()
    {
        return Err(ReadError::layout("inventory rebuild flag"));
    }
    let sync = INVENTORY_OWNER.sync_tokens.get();
    let init = reader.read_module_array::<26>(facts.sync_tokens_init)?;
    // Two qword+dword stores initialize the adjacent 12-byte fields in that constructor.
    if init[..3] != [0x48, 0x89, 0x87]
        || init[3..7] != sync.to_le_bytes()
        || init[7..9] != [0x89, 0x87]
        || init[9..13] != (sync + 8).to_le_bytes()
        || init[13..16] != [0x48, 0x89, 0x87]
        || init[16..20] != (sync + 12).to_le_bytes()
        || init[20..22] != [0x89, 0x87]
        || init[22..] != (sync + 20).to_le_bytes()
    {
        return Err(ReadError::layout("inventory synchronization fields"));
    }
    let compare = reader.read_module_array::<34>(facts.sync_tokens_compare)?;
    let call = facts
        .sync_tokens_compare
        .checked_add(29)
        .ok_or_else(|| ReadError::layout("inventory synchronization comparison"))?;
    // lea rdi, [rcx+second]; movzx esi, dl; lea rbx, [rcx+first];
    // compare(first, second, 12). Both addresses use the same profile-data owner.
    if compare[..3] != [0x48, 0x8d, 0xb9]
        || compare[3..7] != (sync + 12).to_le_bytes()
        || compare[7..13] != [0x0f, 0xb6, 0xf2, 0x48, 0x8d, 0x99]
        || compare[13..17] != sync.to_le_bytes()
        || compare[17..30]
            != [
                0x48, 0x8b, 0xd7, 0x48, 0x8b, 0xcb, 0x41, 0xb8, 0x0c, 0, 0, 0, 0xe8,
            ]
        || call.rip_target(5, &compare[30..]) != Some(facts.compare_bytes)
    {
        return Err(ReadError::layout("inventory synchronization comparison"));
    }
    Ok(())
}
