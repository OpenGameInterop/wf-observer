use memory_reader::ProcessMemory;
use std::mem::size_of;

use super::{MemoryError, ObjectOffset, ReadError, Rva, VtableSlot};

/// Provider-selected bounds for one reader's lifetime.
#[derive(Debug, ..Copy, ..Eq)]
pub struct ReadLimits {
    /// Lowest permitted address, inclusive.
    pub min_address: u64,
    /// Highest permitted address, inclusive.
    pub max_address: u64,
    /// Maximum total bytes requested from the memory backend.
    pub bytes: usize,
    /// Maximum number of nonempty backend reads, including failed reads.
    pub reads: usize,
}

impl ReadLimits {
    fn validate_range(self, address: u64, length: usize) -> Result<(), MemoryError> {
        if length == 0 {
            return Ok(());
        }
        let invalid = || MemoryError::InvalidTargetRange(address, length);
        if address < self.min_address || address > self.max_address {
            return Err(invalid());
        }
        let last = address
            .checked_add(u64::try_from(length - 1).map_err(|_| MemoryError::AddressOverflow)?)
            .ok_or(MemoryError::AddressOverflow)?;
        if last > self.max_address {
            return Err(invalid());
        }
        Ok(())
    }
}

/// Budgeted reads with checked module and object addressing.
///
/// Integer helpers decode little-endian values. Vtable helpers explicitly use
/// 64-bit pointers. The caller owns process-identity and acquisition-coherence
/// checks; this reader neither attaches to nor verifies a process.
pub struct TargetReader<'a, M: ProcessMemory + ?Sized> {
    memory: &'a mut M,
    module_base: u64,
    image_size: u32,
    limits: ReadLimits,
}

impl<'a, M: ProcessMemory + ?Sized> TargetReader<'a, M> {
    /// Validates the complete module span without reading target memory.
    ///
    /// # Errors
    /// Rejects empty images, address overflow, or a module outside `limits`.
    pub fn new(
        memory: &'a mut M,
        module_base: u64,
        image_size: u32,
        limits: ReadLimits,
    ) -> Result<Self, ReadError> {
        if image_size == 0 {
            return Err(MemoryError::InvalidModuleRange(Rva::new(0), 0).into());
        }
        let length = usize::try_from(image_size).map_err(|_| MemoryError::AddressOverflow)?;
        limits.validate_range(module_base, length)?;
        Ok(Self {
            memory,
            module_base,
            image_size,
            limits,
        })
    }

    /// Resolves an RVA wholly inside the configured image.
    ///
    /// # Errors
    /// Rejects arithmetic overflow or a span outside the module/address bounds.
    pub fn module_address(&self, rva: Rva, length: usize) -> Result<u64, ReadError> {
        let end = u64::from(rva.get())
            .checked_add(u64::try_from(length).map_err(|_| MemoryError::AddressOverflow)?)
            .ok_or(MemoryError::AddressOverflow)?;
        if end > u64::from(self.image_size) {
            return Err(MemoryError::InvalidModuleRange(rva, length).into());
        }
        let address = self
            .module_base
            .checked_add(u64::from(rva.get()))
            .ok_or(MemoryError::AddressOverflow)?;
        self.limits.validate_range(address, length)?;
        Ok(address)
    }

    /// Reads a module-relative span.
    ///
    /// # Errors
    /// Returns range, budget or backend read failures.
    pub fn read_module(&mut self, rva: Rva, output: &mut [u8]) -> Result<(), ReadError> {
        let address = self.module_address(rva, output.len())?;
        self.read_unchecked(address, output)
    }

    /// Reads a fixed-size module-relative span.
    ///
    /// # Errors
    /// Returns range, budget or backend read failures.
    pub fn read_module_array<const N: usize>(&mut self, rva: Rva) -> Result<[u8; N], ReadError> {
        let mut output = [0_u8; N];
        self.read_module(rva, &mut output)?;
        Ok(output)
    }

    /// Reads an absolute span. Empty reads do not access memory or consume budget.
    ///
    /// # Errors
    /// Returns range, budget or backend read failures. On failure, discard output.
    pub fn read_at(&mut self, address: u64, output: &mut [u8]) -> Result<(), ReadError> {
        self.limits.validate_range(address, output.len())?;
        self.read_unchecked(address, output)
    }

    /// Reads a fixed-size absolute span.
    ///
    /// # Errors
    /// Returns range, budget or backend read failures.
    pub fn read_array<const N: usize>(&mut self, address: u64) -> Result<[u8; N], ReadError> {
        let mut output = [0_u8; N];
        self.read_at(address, &mut output)?;
        Ok(output)
    }

    /// Resolves an object-relative span within the permitted address range.
    ///
    /// # Errors
    /// Rejects arithmetic overflow or addresses outside the configured bounds.
    pub fn object_address(
        &self,
        object: u64,
        offset: ObjectOffset,
        length: usize,
    ) -> Result<u64, ReadError> {
        let address = object
            .checked_add(u64::from(offset.get()))
            .ok_or(MemoryError::AddressOverflow)?;
        self.limits.validate_range(address, length)?;
        Ok(address)
    }

    /// Reads a fixed-size object-relative span.
    ///
    /// # Errors
    /// Returns range, budget or backend read failures.
    pub fn read_object_array<const N: usize>(
        &mut self,
        object: u64,
        offset: ObjectOffset,
    ) -> Result<[u8; N], ReadError> {
        let address = self.object_address(object, offset, N)?;
        self.read_array(address)
    }

    /// Reads one byte.
    ///
    /// # Errors
    /// Returns range, budget or backend read failures.
    pub fn read_u8(&mut self, address: u64) -> Result<u8, ReadError> {
        Ok(u8::from_le_bytes(self.read_array(address)?))
    }

    /// Reads a little-endian 32-bit integer.
    ///
    /// # Errors
    /// Returns range, budget or backend read failures.
    pub fn read_u32(&mut self, address: u64) -> Result<u32, ReadError> {
        Ok(u32::from_le_bytes(self.read_array(address)?))
    }

    /// Reads a little-endian 64-bit integer.
    ///
    /// # Errors
    /// Returns range, budget or backend read failures.
    pub fn read_u64(&mut self, address: u64) -> Result<u64, ReadError> {
        Ok(u64::from_le_bytes(self.read_array(address)?))
    }

    /// Reads an object-relative byte.
    ///
    /// # Errors
    /// Returns range, budget or backend read failures.
    pub fn read_object_u8(&mut self, object: u64, offset: ObjectOffset) -> Result<u8, ReadError> {
        Ok(u8::from_le_bytes(self.read_object_array(object, offset)?))
    }

    /// Reads an object-relative little-endian 32-bit integer.
    ///
    /// # Errors
    /// Returns range, budget or backend read failures.
    pub fn read_object_u32(&mut self, object: u64, offset: ObjectOffset) -> Result<u32, ReadError> {
        Ok(u32::from_le_bytes(self.read_object_array(object, offset)?))
    }

    /// Reads an object-relative little-endian 64-bit integer.
    ///
    /// # Errors
    /// Returns range, budget or backend read failures.
    pub fn read_object_u64(&mut self, object: u64, offset: ObjectOffset) -> Result<u64, ReadError> {
        Ok(u64::from_le_bytes(self.read_object_array(object, offset)?))
    }

    /// Reads a vtable entry using little-endian 64-bit object and entry pointers.
    /// `slot` is a byte offset; ABI and alignment requirements belong to the caller.
    ///
    /// # Errors
    /// Returns range, budget or backend read failures from either pointer read.
    pub fn read_vtable_slot_le64(
        &mut self,
        object: u64,
        slot: VtableSlot,
    ) -> Result<u64, ReadError> {
        let vtable = self.read_u64(object)?;
        let address =
            self.object_address(vtable, ObjectOffset::new(slot.get()), size_of::<u64>())?;
        self.read_u64(address)
    }

    /// Requires an absolute pointer to match a module-relative address.
    ///
    /// # Errors
    /// Returns a range failure or labels a pointer mismatch with `at`.
    pub fn require_module_pointer(
        &self,
        actual: u64,
        expected: Rva,
        at: &'static str,
    ) -> Result<(), ReadError> {
        if actual == self.module_address(expected, 1)? {
            Ok(())
        } else {
            Err(ReadError::invalid(at))
        }
    }

    /// Requires a little-endian 64-bit vtable entry to match a module address.
    ///
    /// # Errors
    /// Returns a bounded-read failure or labels a pointer mismatch with `at`.
    pub fn require_vtable_slot_le64(
        &mut self,
        object: u64,
        slot: VtableSlot,
        expected: Rva,
        at: &'static str,
    ) -> Result<(), ReadError> {
        let actual = self.read_vtable_slot_le64(object, slot)?;
        self.require_module_pointer(actual, expected, at)
    }

    fn read_unchecked(&mut self, address: u64, output: &mut [u8]) -> Result<(), ReadError> {
        if output.is_empty() {
            return Ok(());
        }
        let bytes = self
            .limits
            .bytes
            .checked_sub(output.len())
            .ok_or(ReadError::limit("acquisition byte budget"))?;
        let reads = self
            .limits
            .reads
            .checked_sub(1)
            .ok_or(ReadError::limit("acquisition read budget"))?;
        self.limits.bytes = bytes;
        self.limits.reads = reads;
        self.memory
            .read_into(address, output)
            .map_err(|error| MemoryError::Read(error.to_string()).into())
    }
}
