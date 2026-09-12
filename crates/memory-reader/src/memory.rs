//! Read-only access to an attached process.

use crate::{AccessError, Target};

/// One loaded module in a process's virtual address space.
#[derive(Clone, Debug, ..Eq)]
pub struct MemoryModule {
    /// Module name reported by the operating system.
    pub name: String,
    /// Module path reported by the operating system, or an empty string if unavailable.
    pub path: String,
    /// Base virtual address in the target process.
    pub base: u64,
    /// Size in bytes reported for the module.
    pub size: u64,
}

/// Read-only memory operations for one retained process instance.
///
/// Call [`Self::verify`] before and after each complete acquisition, including
/// failed acquisitions. If verification fails, discard the acquired data and
/// any retained parsing state, and retire the attachment. Individual reads need
/// not check identity; PID-based backends can access a replacement process until
/// the next verification detects reuse.
///
/// These checks do not freeze the process: memory and module mappings can change
/// during a read, and multiple reads are not an atomic snapshot.
///
/// This interface exposes no writes. The native backend's operating-system
/// handle permissions are controlled by its dependency, not by this trait.
pub trait ProcessMemory {
    /// Returns the identity and executable selected during discovery.
    fn target(&self) -> &Target;

    /// Revalidates the process instance and its executable-mapping access probe.
    ///
    /// # Errors
    ///
    /// Returns an error if the process instance changed or the probe cannot be read.
    fn verify(&mut self) -> Result<(), AccessError>;

    /// Enumerates the process's loaded modules.
    ///
    /// # Errors
    ///
    /// Returns an error if the process instance changed or enumeration failed.
    fn modules(&mut self) -> Result<Vec<MemoryModule>, AccessError>;

    /// Reads the entire buffer from a target virtual address.
    ///
    /// Success means every requested byte was read. On error the buffer may have
    /// been partially modified, and callers must discard its contents. An empty
    /// buffer performs no memory read. This method need not check process identity;
    /// use [`Self::verify`] at acquisition boundaries.
    ///
    /// # Errors
    ///
    /// Returns an error if the address range overflows or any requested byte
    /// could not be read.
    fn read_into(&mut self, address: u64, buffer: &mut [u8]) -> Result<(), AccessError>;
}
