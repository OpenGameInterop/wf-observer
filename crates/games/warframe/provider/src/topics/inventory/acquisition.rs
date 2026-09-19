use std::collections::BTreeMap;

use derive_more::{Error, From};
use displaydoc::Display;
use memory_reader::ProcessMemory;
use provider_sdk::memory::{ReadError, TargetReader};
use warframe_model::{
    InventoryFamily, InventoryFamilySnapshot, InventoryItemCount, InventorySnapshot, ItemKey,
};

use crate::{
    item_type::{ItemTypeCache, ItemTypeError, facts::ITEM_TYPES},
    profile_inventory::{INVENTORY_OWNER, VECTOR_HEADER_BYTES, read_commit_state, read_vector},
    roots::LoginIdentity,
    string_pool::StringTokenCache,
    target::READ_LIMITS,
};

use super::{
    facts::INVENTORY,
    layout::{LayoutError, decode_records},
};

#[derive(Debug, Display, Error, From)]
pub(crate) enum InventoryError {
    /// bounded inventory read failed: {0}
    #[from]
    Read(ReadError),
    /// inventory layout validation failed: {0}
    #[from]
    Layout(LayoutError),
    /// bundled inventory layout for {0:?} failed validation at {1}
    UnsupportedLayout(InventoryFamily, &'static str),
    /// item type resolution or validation failed: {0}
    #[from]
    ItemType(ItemTypeError),
    /// inventory model failed validation: {0}
    #[from]
    InvalidModel(warframe_model::InvalidInventory),
}

/// Reads all families between inventory commit markers.
pub(crate) fn read_inventory(
    memory: &mut (impl ProcessMemory + ?Sized),
    module_base: u64,
    image_size: u32,
    login: &LoginIdentity,
    item_type_cache: &mut ItemTypeCache,
    string_cache: &mut StringTokenCache,
) -> Result<InventorySnapshot, InventoryError> {
    let mut reader = TargetReader::new(memory, module_base, image_size, READ_LIMITS)?;
    let profile_data = login.profile_data.get();
    let inventory =
        reader.object_address(profile_data, INVENTORY_OWNER.offset, VECTOR_HEADER_BYTES)?;
    let before = read_commit_state(&mut reader, profile_data)?;
    let mut families = Vec::with_capacity(INVENTORY.families.len());
    for facts in INVENTORY.families {
        let (address, payload) =
            read_vector(&mut reader, inventory, facts.vector, facts.record.bytes)?;
        let counts = decode_records(address, &payload, facts.record)?;
        families.push(InventoryFamilySnapshot {
            family: facts.family,
            items: project_items(&mut reader, &counts, item_type_cache, string_cache)?,
        });
    }
    if before != read_commit_state(&mut reader, profile_data)? {
        return Err(ReadError::changed("inventory commit markers").into());
    }
    InventorySnapshot::new(login.account_id.clone(), families).map_err(InventoryError::from)
}

/// Resolves native identities and combines counts that map to the same item path.
fn project_items(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    item_counts: &BTreeMap<u64, u64>,
    item_types: &mut ItemTypeCache,
    strings: &mut StringTokenCache,
) -> Result<Vec<InventoryItemCount>, InventoryError> {
    let mut counts = BTreeMap::<ItemKey, u64>::new();
    // Resolve the pool only on cache misses, once per family.
    let mut pool = None;
    for (&item_type, &quantity) in item_counts {
        let item_key =
            item_types.resolve_item_path(reader, ITEM_TYPES, item_type, strings, &mut pool)?;
        let aggregate = counts.entry(item_key).or_default();
        *aggregate = aggregate
            .checked_add(quantity)
            .ok_or(LayoutError::CountOverflow)?;
    }
    Ok(counts
        .into_iter()
        .map(|(item_key, quantity)| InventoryItemCount { item_key, quantity })
        .collect())
}
