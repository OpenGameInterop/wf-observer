use derive_more::Error;
use displaydoc::Display;
use warframe_model::AccountId;

pub(crate) const ACCOUNT_ID_BYTES: usize = 24;

/// Invalid native account identifier.
#[derive(Debug, Display, Error, ..Copy, ..Eq)]
pub(crate) enum DecodeError {
    /// target string header has an unsupported representation
    UnsupportedStringRepresentation,
    /// target account identifier has an unexpected length
    InvalidAccountLength,
    /// target account identifier contains unexpected bytes
    InvalidAccountBytes,
}

pub(crate) fn decode_account_string_header(header: &[u8; 16]) -> Result<u64, DecodeError> {
    if header[15] != 0xff {
        return Err(DecodeError::UnsupportedStringRepresentation);
    }
    if u32::from_le_bytes(header.as_chunks::<4>().0[2]) & 0x0fff_ffff != 24 {
        return Err(DecodeError::InvalidAccountLength);
    }
    Ok(u64::from_le_bytes(header.as_chunks::<8>().0[0]))
}

pub(crate) fn decode_account_id(bytes: &[u8; ACCOUNT_ID_BYTES]) -> Result<AccountId, DecodeError> {
    let text = std::str::from_utf8(bytes).map_err(|_| DecodeError::InvalidAccountBytes)?;
    AccountId::new(text).map_err(|_| DecodeError::InvalidAccountBytes)
}
