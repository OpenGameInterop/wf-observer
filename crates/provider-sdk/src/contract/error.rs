//! Failures crossing the provider-host boundary.

use derive_more::Error;
use displaydoc::Display;
use memory_reader::AccessError;

/// A startup, polling, or publication failure; the host decides recovery policy.
#[derive(Debug, Display, Error)]
pub enum ProviderError {
    /// memory access failed: {0}
    Memory(AccessError),
    /// target does not match this provider or memory attachment
    InvalidTarget,
    /// provider operation failed: {_0}
    Failed(#[error(not(source))] String),
}

impl From<AccessError> for ProviderError {
    fn from(error: AccessError) -> Self {
        Self::Memory(error)
    }
}
