use std::collections::BTreeMap;

use derive_more::Error;
use displaydoc::Display;
use provider_sdk::memory::{ReadError, RecordError, RecordView};

use super::facts::{InventoryRecordCountFacts, InventoryRecordFacts};

pub(super) const VECTOR_HEADER_BYTES: usize = 16;
pub(super) const MAX_VECTOR_BYTES: u32 = 8 * 1024 * 1024;

const MAX_USER_ADDRESS: u64 = 0x0000_7fff_ffff_ffff;

/// Native pointer/byte-length/capacity vector header.
#[derive(Debug, ..Copy, ..Eq)]
pub(super) struct NativeVectorHeader {
    pub(super) pointer: u64,
    pub(super) bytes: u32,
    pub(super) capacity: u32,
}

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

pub(super) fn decode_header(
    bytes: &[u8; VECTOR_HEADER_BYTES],
    record_bytes: u32,
) -> Result<NativeVectorHeader, ReadError> {
    if record_bytes == 0 {
        return Err(ReadError::invalid("native vector stride"));
    }
    let header = NativeVectorHeader {
        pointer: u64::from_le_bytes(
            bytes[..8]
                .try_into()
                .map_err(|_| ReadError::invalid("vector pointer"))?,
        ),
        bytes: u32::from_le_bytes(
            bytes[8..12]
                .try_into()
                .map_err(|_| ReadError::invalid("vector length"))?,
        ),
        capacity: u32::from_le_bytes(
            bytes[12..]
                .try_into()
                .map_err(|_| ReadError::invalid("vector capacity"))?,
        ),
    };
    if header.bytes > header.capacity {
        return Err(ReadError::invalid("native vector length/capacity"));
    }
    if header.capacity > MAX_VECTOR_BYTES {
        return Err(ReadError::limit("native vector capacity"));
    }
    if !header.bytes.is_multiple_of(record_bytes) || !header.capacity.is_multiple_of(record_bytes) {
        return Err(ReadError::invalid("native vector records"));
    }
    if !is_user_range(header.pointer, header.capacity) {
        return Err(ReadError::invalid("native vector payload"));
    }
    Ok(header)
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

pub(in crate::topics) const fn is_user_address(address: u64) -> bool {
    address >= 0x1_0000 && address <= MAX_USER_ADDRESS
}

const fn is_user_range(address: u64, bytes: u32) -> bool {
    if bytes == 0 {
        return address == 0 || is_user_address(address);
    }
    is_user_address(address)
        && match address.checked_add(bytes as u64 - 1) {
            Some(end) => end <= MAX_USER_ADDRESS,
            None => false,
        }
}
