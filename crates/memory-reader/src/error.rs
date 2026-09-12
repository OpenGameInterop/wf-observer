//! Target discovery and attachment failures.

use derive_more::Error;
use displaydoc::Display;

/// Failure while discovering target processes.
#[derive(Debug, Display, Error)]
pub enum DiscoveryError {
    /// failed to initialize native process access: {0}
    Initialize(memflow::error::Error),
    /// failed to enumerate native processes: {0}
    Enumerate(memflow::error::Error),
}

/// Failure while opening or validating a discovered target.
#[derive(Debug, Display, Error)]
pub enum AccessError {
    /// target discovery failed: {0}
    Discovery(DiscoveryError),
    /// target process {_0} no longer exists
    TargetNotFound(#[error(not(source))] u32),
    /// target process {_0} exited or changed identity during access
    TargetChanged(#[error(not(source))] u32),
    /// read range starting at {address:#x} with length {length} is invalid or unavailable
    InvalidReadRange {
        /// Requested virtual address.
        address: u64,
        /// Requested byte count.
        length: usize,
    },
    /// failed to open target process {pid}: {source}
    Open {
        /// Target process identifier.
        pid: u32,
        /// Native access failure.
        source: memflow::error::Error,
    },
    /// failed to inspect loaded executable images for target process {pid}: {source}
    Modules {
        /// Target process identifier.
        pid: u32,
        /// Native access failure.
        source: memflow::error::Error,
    },
    /// target process {_0} has no readable executable mapping
    NoExecutableModule(#[error(not(source))] u32),
    /// failed to read target process {pid}: {source}
    Read {
        /// Target process identifier.
        pid: u32,
        /// Native access failure.
        source: memflow::error::Error,
    },
}
