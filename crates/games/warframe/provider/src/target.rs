//! Warframe address-space policy and executable identity.

use memory_reader::ProcessMemory;
use provider_sdk::{
    memory::ReadLimits,
    pe::{HeaderError, read_amd64_headers},
};

pub(crate) const READ_LIMITS: ReadLimits = ReadLimits {
    min_address: 1,
    max_address: 0x0000_7fff_ffff_ffff,
    bytes: 32 * 1024 * 1024,
    reads: 64 * 1024,
};

#[derive(Debug, derive_more::Display, ..Copy, ..Eq)]
#[display("{timestamp:08x}-{image_size:08x}")]
pub(crate) struct ExecutableFingerprint {
    pub(crate) timestamp: u32,
    pub(crate) image_size: u32,
}

/// Build associated with the compiled layouts; differing fingerprints still undergo validation.
pub(crate) const BUILD: ExecutableFingerprint = ExecutableFingerprint {
    timestamp: 0x6a85_c6b0,
    image_size: 0x02ce_f000,
};

pub(crate) type FingerprintError = HeaderError<memory_reader::AccessError>;

pub(crate) fn read_executable_fingerprint(
    memory: &mut dyn ProcessMemory,
    base: u64,
) -> Result<ExecutableFingerprint, FingerprintError> {
    let headers = read_amd64_headers(base, |address, output| memory.read_into(address, output))?;
    if base
        .checked_add(u64::from(headers.image_size) - 1)
        .is_none_or(|last| last > READ_LIMITS.max_address)
    {
        return Err(FingerprintError::InvalidImage);
    }
    Ok(ExecutableFingerprint {
        timestamp: headers.timestamp,
        image_size: headers.image_size,
    })
}
