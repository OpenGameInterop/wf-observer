use super::ObjectOffset;

/// A field cannot be decoded from the supplied record bytes.
#[derive(Debug, derive_more::Error, displaydoc::Display, ..Copy, ..Eq)]
pub enum RecordError {
    /// record field arithmetic overflowed at {at}
    Overflow {
        /// Caller-supplied diagnostic label, not record contents.
        at: &'static str,
    },
    /// record field is outside the supplied bytes at {at}
    OutOfBounds {
        /// Caller-supplied diagnostic label, not record contents.
        at: &'static str,
    },
}

/// Bounds-checked view over a record with little-endian integer fields.
/// This borrows already-read bytes; it does not access process memory.
#[derive(Debug, ..Copy)]
pub struct RecordView<'a> {
    bytes: &'a [u8],
    at: &'static str,
}

impl<'a> RecordView<'a> {
    /// Labels a borrowed record for errors without inspecting its contents.
    #[must_use]
    pub const fn new(bytes: &'a [u8], at: &'static str) -> Self {
        Self { bytes, at }
    }

    /// Borrows a field wholly within the supplied record.
    ///
    /// # Errors
    ///
    /// Rejects offset/length overflow and fields extending beyond the record.
    pub fn field(self, offset: ObjectOffset, length: usize) -> Result<&'a [u8], RecordError> {
        let start =
            usize::try_from(offset.get()).map_err(|_| RecordError::Overflow { at: self.at })?;
        let end = start
            .checked_add(length)
            .ok_or(RecordError::Overflow { at: self.at })?;
        self.bytes
            .get(start..end)
            .ok_or(RecordError::OutOfBounds { at: self.at })
    }

    /// Decodes one little-endian 32-bit field, including unaligned fields.
    ///
    /// # Errors
    ///
    /// Rejects a field not wholly contained in the record.
    pub fn u32(self, offset: ObjectOffset) -> Result<u32, RecordError> {
        Ok(u32::from_le_bytes(
            self.field(offset, 4)?
                .try_into()
                .map_err(|_| RecordError::OutOfBounds { at: self.at })?,
        ))
    }

    /// Decodes one little-endian 64-bit field, including unaligned fields.
    ///
    /// # Errors
    ///
    /// Rejects a field not wholly contained in the record.
    pub fn u64(self, offset: ObjectOffset) -> Result<u64, RecordError> {
        Ok(u64::from_le_bytes(
            self.field(offset, 8)?
                .try_into()
                .map_err(|_| RecordError::OutOfBounds { at: self.at })?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{ObjectOffset, RecordError, RecordView};

    #[test]
    fn record_fields_are_little_endian_unaligned_and_bounded() -> Result<(), RecordError> {
        let value = 0x0123_4567_89ab_cdef_u64;
        let mut bytes = [0; 9];
        bytes[1..].copy_from_slice(&value.to_le_bytes());
        let record = RecordView::new(&bytes, "test record");
        assert_eq!(record.u64(ObjectOffset::new(1))?, value);
        assert_eq!(record.u32(ObjectOffset::new(5))?, 0x0123_4567);
        assert!(record.field(ObjectOffset::new(9), 0)?.is_empty());
        assert!(matches!(
            record.u64(ObjectOffset::new(2)),
            Err(RecordError::OutOfBounds { .. })
        ));
        assert!(matches!(
            record.field(ObjectOffset::new(1), usize::MAX),
            Err(RecordError::Overflow { .. })
        ));
        Ok(())
    }
}
