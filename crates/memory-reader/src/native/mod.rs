//! Native target discovery and read-only attachment.

mod access;
mod attachment;
mod discovery;

pub use attachment::{AttachedTarget, attach};
pub use discovery::discover_targets_by;

#[cfg(test)]
mod tests;
