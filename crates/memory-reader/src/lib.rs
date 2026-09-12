//! Process memory reading primitives.

#![forbid(unsafe_code)]

#[macro_use(derive)]
extern crate derive_aliases;

mod derive_alias;

mod error;
mod memory;
mod native;
mod target;

pub use error::{AccessError, DiscoveryError};
pub use memory::{MemoryModule, ProcessMemory};
pub use native::{AttachedTarget, attach, discover_targets_by};
pub use target::{ProcessInstance, ProcessMetadata, Target};
