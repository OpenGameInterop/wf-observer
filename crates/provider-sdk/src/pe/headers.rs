//! Minimal mapped amd64 PE header inspection, not a full executable validator.

/// Header metadata useful to a provider's build-identification policy.
/// Timestamp and image size are not a unique or cryptographic build identity.
#[derive(Debug, ..Copy, ..Eq)]
pub struct ImageHeaders {
    /// COFF timestamp as stored in the image.
    pub timestamp: u32,
    /// Mapped image size in bytes, including headers.
    pub image_size: u32,
}

/// Failed memory access or an unsupported/malformed mapped image header.
#[derive(Debug, derive_more::Error, displaydoc::Display)]
pub enum HeaderError<E> {
    /// executable header read failed: {0}
    Read(E),
    /// executable header is not a bounded amd64 PE image
    InvalidImage,
}

/// Reads the DOS and required PE32+ header fields through a caller-owned reader.
///
/// The callback must fill the entire buffer on success. At most two reads are
/// performed (64 and 84 bytes). The PE header must start within the first MiB;
/// overflowing addresses, other architectures and invalid image extents fail.
/// This does not validate sections, code, or provider-specific compatibility.
/// Address-space policy (for example, user-space-only access) belongs to the caller.
///
/// # Errors
///
/// Returns the callback's error, or [`HeaderError::InvalidImage`] for a rejected header.
pub fn read_amd64_headers<E>(
    base: u64,
    mut read: impl FnMut(u64, &mut [u8]) -> Result<(), E>,
) -> Result<ImageHeaders, HeaderError<E>> {
    let mut dos_header = [0; 64];
    read(base, &mut dos_header).map_err(HeaderError::Read)?;
    let offset = u32::from_le_bytes([
        dos_header[60],
        dos_header[61],
        dos_header[62],
        dos_header[63],
    ]);
    if dos_header[..2] != *b"MZ" || !(64..=1024 * 1024).contains(&offset) {
        return Err(HeaderError::InvalidImage);
    }
    let address = base
        .checked_add(u64::from(offset))
        .ok_or(HeaderError::InvalidImage)?;
    let mut pe_header = [0; 84];
    read(address, &mut pe_header).map_err(HeaderError::Read)?;
    let image_size =
        u32::from_le_bytes([pe_header[80], pe_header[81], pe_header[82], pe_header[83]]);
    let optional_bytes = u16::from_le_bytes([pe_header[20], pe_header[21]]);
    if pe_header[..4] != *b"PE\0\0"
        || pe_header[4..6] != 0x8664_u16.to_le_bytes()
        || optional_bytes < 60
        || pe_header[24..26] != 0x20b_u16.to_le_bytes()
        || image_size == 0
        || offset
            .checked_add(24 + u32::from(optional_bytes))
            .is_none_or(|end| end > image_size)
        || base.checked_add(u64::from(image_size) - 1).is_none()
    {
        return Err(HeaderError::InvalidImage);
    }
    Ok(ImageHeaders {
        timestamp: u32::from_le_bytes([pe_header[8], pe_header[9], pe_header[10], pe_header[11]]),
        image_size,
    })
}

#[cfg(test)]
mod tests {
    use super::{HeaderError, read_amd64_headers};

    fn image(timestamp: u32) -> [u8; 512] {
        let mut bytes = [0; 512];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&128_u32.to_le_bytes());
        bytes[128..132].copy_from_slice(b"PE\0\0");
        bytes[132..134].copy_from_slice(&0x8664_u16.to_le_bytes());
        bytes[136..140].copy_from_slice(&timestamp.to_le_bytes());
        bytes[148..150].copy_from_slice(&240_u16.to_le_bytes());
        bytes[152..154].copy_from_slice(&0x20b_u16.to_le_bytes());
        bytes[208..212].copy_from_slice(&0x1000_u32.to_le_bytes());
        bytes
    }

    fn read_at(
        bytes: &[u8],
        base: u64,
        address: u64,
        output: &mut [u8],
    ) -> Result<(), &'static str> {
        let start = address
            .checked_sub(base)
            .and_then(|offset| usize::try_from(offset).ok())
            .ok_or("invalid address")?;
        let end = start.checked_add(output.len()).ok_or("overflow")?;
        let source = bytes.get(start..end).ok_or("missing bytes")?;
        output.copy_from_slice(source);
        Ok(())
    }

    #[test]
    fn pe_headers_use_module_relative_reads_without_imposing_user_address_policy()
    -> Result<(), &'static str> {
        for (base, timestamp) in [(0x140_0000_0000, 7), (0xffff_8000_0000_0000, 23)] {
            let bytes = image(timestamp);
            let headers = read_amd64_headers(base, |address, output| {
                read_at(&bytes, base, address, output)
            })
            .map_err(|_| "header rejected")?;
            assert_eq!(headers.timestamp, timestamp);
            assert_eq!(headers.image_size, 0x1000);
        }
        Ok(())
    }

    #[test]
    fn pe_headers_reject_bad_signatures_architecture_bounds_and_missing_memory() {
        let valid = image(7);
        let mut malformed = Vec::new();
        for (offset, value) in [(0, 0), (128, 0), (132, 0), (152, 0), (148, 0)] {
            let mut bytes = valid;
            bytes[offset] = value;
            malformed.push(bytes);
        }
        for offset in [0_u32, 1024 * 1024 + 1] {
            let mut bytes = valid;
            bytes[60..64].copy_from_slice(&offset.to_le_bytes());
            malformed.push(bytes);
        }
        for size in [0_u32, 128] {
            let mut bytes = valid;
            bytes[208..212].copy_from_slice(&size.to_le_bytes());
            malformed.push(bytes);
        }
        for bytes in malformed {
            assert!(matches!(
                read_amd64_headers(0, |address, output| read_at(&bytes, 0, address, output)),
                Err(HeaderError::InvalidImage)
            ));
        }
        assert!(matches!(
            read_amd64_headers(0, |address, output| read_at(
                &valid[..211],
                0,
                address,
                output
            )),
            Err(HeaderError::Read("missing bytes"))
        ));
        let base = u64::MAX - 512;
        assert!(matches!(
            read_amd64_headers(base, |address, output| read_at(
                &valid, base, address, output
            )),
            Err(HeaderError::InvalidImage)
        ));
    }
}
