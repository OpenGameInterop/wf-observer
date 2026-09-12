use derive_more::Display;

/// A 32-bit module-relative virtual address, not an absolute process address.
#[derive(Debug, Display, Hash, ..Copy, ..Ord)]
#[display("0x{_0:x}")]
pub struct Rva(u32);

impl Rva {
    /// Creates an RVA without validating it against a particular image's bounds.
    #[must_use]
    pub const fn new(bytes: u32) -> Self {
        Self(bytes)
    }

    /// Returns the byte distance from the module base.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Adds a byte distance without leaving the module-relative address space.
    #[must_use]
    pub const fn checked_add(self, bytes: u32) -> Option<Self> {
        match self.0.checked_add(bytes) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Resolves one amd64 RIP-relative displacement without wrapping.
    /// Returns `None` unless the displacement is four little-endian bytes and
    /// the result fits in the 32-bit RVA space. Image bounds are checked separately.
    #[must_use]
    pub fn rip_target(self, instruction_bytes: u32, displacement: &[u8]) -> Option<Self> {
        let displacement = i32::from_le_bytes(displacement.try_into().ok()?);
        let target = u64::from(self.0)
            .checked_add(u64::from(instruction_bytes))?
            .checked_add_signed(i64::from(displacement))?;
        u32::try_from(target).ok().map(Self)
    }
}

/// A byte offset from the start of a target object.
#[derive(Debug, Hash, ..Copy, ..Ord)]
pub struct ObjectOffset(u32);

impl ObjectOffset {
    /// Creates an offset without validating it against a particular object.
    #[must_use]
    pub const fn new(bytes: u32) -> Self {
        Self(bytes)
    }

    /// Returns the byte offset.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A byte offset into a vtable, not an entry index.
/// Pointer width, alignment, and ABI validation belong to the caller.
#[derive(Debug, Hash, ..Copy, ..Ord)]
pub struct VtableSlot(u32);

impl VtableSlot {
    /// Creates a byte offset without imposing a particular vtable ABI.
    #[must_use]
    pub const fn new(bytes: u32) -> Self {
        Self(bytes)
    }

    /// Returns the byte offset.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::Rva;

    #[test]
    fn rip_targets_reject_invalid_displacements_and_address_wrap() {
        let instruction = Rva::new(0x100);
        assert_eq!(
            instruction.rip_target(7, &(-16_i32).to_le_bytes()),
            Some(Rva::new(0xf7))
        );
        assert_eq!(instruction.rip_target(7, &[0; 3]), None);
        assert_eq!(Rva::new(0).rip_target(1, &(-2_i32).to_le_bytes()), None);
        assert_eq!(Rva::new(u32::MAX).rip_target(1, &[0; 4]), None);
        assert_eq!(Rva::new(u32::MAX).checked_add(1), None);
    }
}
