//! Fixed profile skill pools and purchased ranks; no item-path dependency.
mod acquisition;
mod facts;
mod validation;

pub(crate) use acquisition::read_intrinsics;
pub(crate) use validation::validate_intrinsics_layout;
pub(super) use validation::validate_mastery_sources;
