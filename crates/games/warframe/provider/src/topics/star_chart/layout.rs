use super::facts::{COMPLETIONS, RECORD_BYTES, TAG, TIER};
use provider_sdk::memory::{ReadError, RecordView};

#[derive(Debug, ..Copy, ..Eq)]
pub(super) struct NativeNode {
    pub(super) token: u32,
    pub(super) completions: u32,
    pub(super) tier: u8,
}

pub(super) fn decode_records(bytes: &[u8]) -> Result<Vec<NativeNode>, ReadError> {
    if !bytes.len().is_multiple_of(RECORD_BYTES as usize) {
        return Err(ReadError::invalid("mission progress stride"));
    }
    bytes
        .as_chunks::<{ RECORD_BYTES as usize }>()
        .0
        .iter()
        .map(|bytes| {
            let record = RecordView::new(bytes, "mission progress");
            let token = record.u32(TAG)?;
            if token == 0 {
                return Err(ReadError::invalid("mission node tag"));
            }
            Ok(NativeNode {
                token,
                completions: record.u32(COMPLETIONS)?,
                tier: record.field(TIER, 1)?[0],
            })
        })
        .collect()
}
