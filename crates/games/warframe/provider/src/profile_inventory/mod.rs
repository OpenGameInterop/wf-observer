//! Bounded inventory vectors and profile commit markers shared by account topics.
mod facts;
mod validation;

pub(crate) use facts::{INVENTORY_OWNER, PROFILE_COMMIT};
pub(crate) use validation::{validate_commit_fields, validate_layout};

use memory_reader::ProcessMemory;
use provider_sdk::memory::{ObjectOffset, ReadError, TargetReader};

pub(crate) const VECTOR_HEADER_BYTES: usize = 16;
pub(crate) const MAX_VECTOR_BYTES: u32 = 8 * 1024 * 1024;

const MAX_USER_ADDRESS: u64 = 0x0000_7fff_ffff_ffff;

/// Native pointer/byte-length/capacity vector header.
#[derive(Debug, ..Copy, ..Eq)]
pub(crate) struct NativeVectorHeader {
    pub(crate) pointer: u64,
    pub(crate) bytes: u32,
    pub(crate) capacity: u32,
}

pub(crate) fn decode_header(
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

pub(crate) const fn is_user_address(address: u64) -> bool {
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

pub(crate) fn read_commit_state(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    profile_data: u64,
) -> Result<[u8; 24], ReadError> {
    if reader.read_object_u8(profile_data, PROFILE_COMMIT.force_update)? != 0 {
        return Err(ReadError::not_ready("profile inventory rebuild"));
    }
    reader.read_object_array(profile_data, PROFILE_COMMIT.sync_tokens)
}

/// Validates a vector's header around its payload read.
pub(crate) fn read_vector(
    reader: &mut TargetReader<'_, impl ProcessMemory + ?Sized>,
    inventory: u64,
    vector: ObjectOffset,
    record_bytes: u32,
) -> Result<(u64, Vec<u8>), ReadError> {
    let before = reader.read_object_array::<VECTOR_HEADER_BYTES>(inventory, vector)?;
    let header = decode_header(&before, record_bytes)?;
    let mut payload = vec![0_u8; header.bytes as usize];
    if !payload.is_empty() {
        reader.read_at(header.pointer, &mut payload)?;
    }
    if before != reader.read_object_array::<VECTOR_HEADER_BYTES>(inventory, vector)? {
        return Err(ReadError::changed("inventory vector header"));
    }
    Ok((header.pointer, payload))
}
