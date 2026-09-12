//! Resolves native item descriptors to canonical paths, using [`facts`].
//! The cache holds process-wide item identities, not inventory counts or ownership.

pub(crate) mod facts;
mod validation;

pub(crate) use validation::validate_item_types;

use derive_more::{Error, From};
use displaydoc::Display;
use std::{collections::BTreeMap, mem::size_of};

use memory_reader::ProcessMemory;
use provider_sdk::memory::{ReadError, RecordView, TargetReader};
use warframe_model::ItemKey;

use crate::string_pool::StringTokenCache;

use facts::ItemTypeFacts;

#[derive(Debug, Display, Error, From)]
pub(crate) enum ItemTypeError {
    /// item descriptor or string read failed: {0}
    #[from]
    Read(ReadError),
    /// item type data or layout failed validation at {0}
    Invalid(#[error(not(source))] &'static str),
}

const MAX_ITEM_TYPES: usize = 4_096;
const MAX_PATH_BYTES: usize = 512;
// The validated constructor/path builder places the last required field at +44.
const DESCRIPTOR_BYTES: usize = 48;

/// Process-scoped semantic identity cache for immutable item type descriptors.
#[derive(Default)]
pub(crate) struct ItemTypeCache {
    paths: BTreeMap<u64, ItemKey>,
}

impl ItemTypeCache {
    /// Resolves one immutable native item descriptor to its canonical game path.
    /// Start `pool` at `None` for a read batch; cache misses share its resolved address.
    pub(crate) fn resolve_item_path(
        &mut self,
        reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
        facts: ItemTypeFacts,
        item_type: u64,
        strings: &mut StringTokenCache,
        pool: &mut Option<u64>,
    ) -> Result<ItemKey, ItemTypeError> {
        if let Some(path) = self.paths.get(&item_type) {
            return Ok(path.clone());
        }
        let string_pool = if let Some(pool) = *pool {
            pool
        } else {
            let current = StringTokenCache::pool(reader, facts.strings)?;
            *pool = Some(current);
            current
        };
        let leaf_vtable = reader.module_address(facts.leaf_vtable, size_of::<u64>())?;
        let item_key = resolve_path(reader, facts, string_pool, leaf_vtable, item_type, strings)?;
        if self.paths.len() < MAX_ITEM_TYPES {
            self.paths.insert(item_type, item_key.clone());
        }
        Ok(item_key)
    }
}

fn resolve_path(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    facts: ItemTypeFacts,
    string_pool: u64,
    expected_vtable: u64,
    descriptor: u64,
    strings: &mut StringTokenCache,
) -> Result<ItemKey, ItemTypeError> {
    require_aligned(descriptor, 8, "type descriptor")?;
    let bytes = reader.read_array::<DESCRIPTOR_BYTES>(descriptor)?;
    let vtable = u64::from_le_bytes(
        bytes[..8]
            .try_into()
            .map_err(|_| item_error("type descriptor"))?,
    );
    let record = RecordView::new(&bytes, "type descriptor");
    if vtable != expected_vtable {
        return Err(item_error("type descriptor vtable"));
    }
    let prefix_object = record.u64(facts.prefix_object).map_err(ReadError::from)?;
    let prefix = if prefix_object == 0 {
        String::new()
    } else {
        require_aligned(prefix_object, 4, "type prefix")?;
        let token = reader.read_u32(prefix_object)?;
        strings.resolve(reader, string_pool, token)?
    };
    let suffix_token = record.u32(facts.suffix_token).map_err(ReadError::from)?;
    let suffix = strings.resolve(reader, string_pool, suffix_token)?;
    join_path(&prefix, &suffix)
}

fn join_path(prefix: &str, suffix: &str) -> Result<ItemKey, ItemTypeError> {
    if suffix.is_empty()
        || (!prefix.is_empty() && (!prefix.ends_with('/') || suffix.starts_with('/')))
    {
        return Err(item_error("item type path"));
    }
    let length = prefix
        .len()
        .checked_add(suffix.len())
        .filter(|length| *length <= MAX_PATH_BYTES)
        .ok_or_else(|| item_error("item type path"))?;
    let mut path = String::with_capacity(length);
    path.push_str(prefix);
    path.push_str(suffix);
    ItemKey::new(path).map_err(|_| item_error("type path"))
}

fn require_aligned(
    address: u64,
    alignment: u64,
    location: &'static str,
) -> Result<(), ItemTypeError> {
    if address != 0 && address.is_multiple_of(alignment) {
        Ok(())
    } else {
        Err(item_error(location))
    }
}

const fn item_error(location: &'static str) -> ItemTypeError {
    ItemTypeError::Invalid(location)
}
