use std::collections::BTreeMap;

use derive_more::Error;
use displaydoc::Display;
use provider_sdk::memory::{ReadError, RecordError, RecordView};

use crate::profile_inventory::is_user_address;

use super::facts::{InventoryRecordCountFacts, InventoryRecordFacts};

/// Invalid committed-inventory bytes for the selected build layout.
#[derive(Debug, Display, Error)]
pub(crate) enum LayoutError {
    /// invalid inventory record fields: {0}
    Record(ReadError),
    /// inventory record {0} has an invalid item-type identity {1:#x}
    InvalidItemType(usize, u64),
    /// inventory record {0} has an invalid count integrity word
    InvalidCount(#[error(not(source))] usize),
    /// aggregate inventory count overflowed
    CountOverflow,
}

impl From<RecordError> for LayoutError {
    fn from(error: RecordError) -> Self {
        Self::Record(error.into())
    }
}

pub(super) fn decode_records(
    payload_address: u64,
    payload: &[u8],
    facts: InventoryRecordFacts,
) -> Result<BTreeMap<u64, u64>, LayoutError> {
    let record_bytes = facts.bytes as usize;
    if record_bytes == 0 || !payload.len().is_multiple_of(record_bytes) {
        return Err(LayoutError::Record(ReadError::invalid(
            "inventory record stride",
        )));
    }

    let mut item_counts = BTreeMap::<u64, u64>::new();
    for (index, record) in payload.chunks_exact(record_bytes).enumerate() {
        let record = RecordView::new(record, "inventory record");
        let item_type = record.u64(facts.item_type)?;
        if !is_user_address(item_type) {
            return Err(LayoutError::InvalidItemType(index, item_type));
        }

        let count = match facts.count {
            InventoryRecordCountFacts::Single { .. } => 1,
            InventoryRecordCountFacts::Plain { count, .. } => record.u32(count)?,
            InventoryRecordCountFacts::AddressEncoded { codec, .. } => {
                let stored_offset = codec.stored.get() as usize;
                let check = record.u32(codec.check)?;
                let stored = record.u32(codec.stored)?;
                let stored_address = payload_address
                    .checked_add((index * record_bytes + stored_offset) as u64)
                    .ok_or(LayoutError::CountOverflow)?;
                codec
                    .decode(check, stored, stored_address)
                    .ok_or(LayoutError::InvalidCount(index))?
            }
        };
        if count == 0 {
            continue;
        }
        let aggregate = item_counts.entry(item_type).or_default();
        *aggregate = aggregate
            .checked_add(u64::from(count))
            .ok_or(LayoutError::CountOverflow)?;
    }

    Ok(item_counts)
}
