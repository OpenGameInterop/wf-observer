use derive_more::{Error, From};
use displaydoc::Display;

use super::{RecordError, Rva};
use crate::UnavailableReason;

/// Failure at the bounded target read boundary.
#[derive(Debug, Display, Error)]
pub enum MemoryError {
    /// target address arithmetic overflowed
    AddressOverflow,
    /// target address {0:#x} with length {1} is outside the permitted range
    InvalidTargetRange(u64, usize),
    /// module RVA {0} with length {1} is outside the mapped image
    InvalidModuleRange(Rva, usize),
    /// native target read failed: {0}
    Read(#[error(not(source))] String),
}

/// Mechanical failure during bounded target-memory acquisition or validation.
#[derive(Debug, Display, Error, From)]
pub enum ReadError {
    /// {0}
    #[from]
    Memory(MemoryError),
    /// target layout failed validation at {at}
    LayoutMismatch {
        #[error(not(source))]
        at: &'static str,
    },
    /// target data is invalid at {at}
    InvalidData {
        #[error(not(source))]
        at: &'static str,
    },
    /// target data at {at} changed during acquisition
    Unstable {
        #[error(not(source))]
        at: &'static str,
    },
    /// source limit exceeded at {at}
    LimitExceeded {
        #[error(not(source))]
        at: &'static str,
    },
    /// source arithmetic overflowed at {at}
    Overflow {
        #[error(not(source))]
        at: &'static str,
    },
}

impl From<RecordError> for ReadError {
    fn from(error: RecordError) -> Self {
        match error {
            RecordError::Overflow { at } => Self::overflow(at),
            RecordError::OutOfBounds { at } => Self::invalid(at),
        }
    }
}

/// Maps read failures to public health without exposing addresses or native diagnostics.
impl From<&ReadError> for UnavailableReason {
    fn from(error: &ReadError) -> Self {
        match error {
            ReadError::Unstable { .. } => Self::TargetNotReady,
            ReadError::Memory(MemoryError::Read(_)) => Self::ReadFailed {
                message: "target memory could not be read".into(),
            },
            _ => Self::ValidationFailed {
                message: "target memory failed validation".into(),
            },
        }
    }
}

impl ReadError {
    /// Labels a rejected target layout.
    #[must_use]
    pub const fn layout(at: &'static str) -> Self {
        Self::LayoutMismatch { at }
    }

    /// Labels invalid target data.
    #[must_use]
    pub const fn invalid(at: &'static str) -> Self {
        Self::InvalidData { at }
    }

    /// Labels data that changed during acquisition.
    #[must_use]
    pub const fn changed(at: &'static str) -> Self {
        Self::Unstable { at }
    }

    /// Labels an exhausted acquisition limit.
    #[must_use]
    pub const fn limit(at: &'static str) -> Self {
        Self::LimitExceeded { at }
    }

    /// Labels overflowing source arithmetic.
    #[must_use]
    pub const fn overflow(at: &'static str) -> Self {
        Self::Overflow { at }
    }
}
