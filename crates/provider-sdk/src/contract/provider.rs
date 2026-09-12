//! Identification and session startup for one compiled provider.

use memory_reader::{ProcessMemory, ProcessMetadata, Target};

use crate::{ProviderError, ProviderManifest, ProviderSession};

/// Identifies processes and creates parsing state without owning native memory.
pub trait Provider: Send + Sync {
    /// Returns the compiled manifest without requiring an attachment.
    fn manifest(&self) -> &'static ProviderManifest;

    /// Matches metadata without I/O and returns the canonical executable mapping
    /// to probe, not a game ID. Identification alone does not prove read access.
    fn identify_process(&self, process: &ProcessMetadata<'_>) -> Option<&'static str>;

    /// Starts one session using a temporary borrow of the host's attachment.
    ///
    /// Startup must do bounded work, without sleeping or waiting for game changes.
    /// The returned session must not retain the memory handle or publication sinks.
    /// The host verifies memory before and after startup, even on error, and
    /// discards the session and attachment if verification fails.
    ///
    /// # Errors
    ///
    /// Returns an error for an incompatible target or provider initialization failure.
    fn start(
        &self,
        target: &Target,
        memory: &mut dyn ProcessMemory,
    ) -> Result<Box<dyn ProviderSession>, ProviderError>;
}
