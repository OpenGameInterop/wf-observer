//! Bounded target reads and record decoding;
mod address;
mod coherence;
mod error;
mod reader;
mod record;

pub use address::{ObjectOffset, Rva, VtableSlot};
pub use coherence::read_stable;
pub use error::{MemoryError, ReadError};
pub use reader::{ReadLimits, TargetReader};
pub use record::{RecordError, RecordView};

#[cfg(test)]
mod reader_tests;
