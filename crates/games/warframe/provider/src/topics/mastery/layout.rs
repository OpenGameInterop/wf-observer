use super::facts::MASTERY;
use crate::profile_inventory::is_user_address;
use provider_sdk::memory::{ReadError, RecordView};

pub(super) struct NativeItem {
    pub(super) item_type: u64,
    pub(super) affinity: u32,
}

/// Decodes at the original payload address, with bounded record access.
pub(super) fn decode_records(
    payload_address: u64,
    payload: &[u8],
) -> Result<Vec<NativeItem>, ReadError> {
    let stride = MASTERY.record_bytes as usize;
    if stride == 0 || !payload.len().is_multiple_of(stride) {
        return Err(ReadError::invalid("mastery record stride"));
    }
    let mut items = Vec::with_capacity(payload.len() / stride);
    for (index, bytes) in payload.chunks_exact(stride).enumerate() {
        let record = RecordView::new(bytes, "mastery record");
        let item_type = record.u64(MASTERY.item_type)?;
        // The game's calculation skips null item slots, including their XP words.
        if item_type == 0 {
            continue;
        }
        if !is_user_address(item_type) {
            return Err(ReadError::invalid("mastery item type"));
        }
        let stored_address = payload_address
            .checked_add((index * stride) as u64)
            .and_then(|address| address.checked_add(u64::from(MASTERY.xp.stored.get())))
            .ok_or_else(|| ReadError::overflow("mastery record address"))?;
        let affinity = MASTERY
            .xp
            .decode(
                record.u32(MASTERY.xp.check)?,
                record.u32(MASTERY.xp.stored)?,
                stored_address,
            )
            .ok_or_else(|| ReadError::invalid("mastery affinity integrity"))?;
        items.push(NativeItem {
            item_type,
            affinity,
        });
    }
    Ok(items)
}
