//! Reads game data and turns it into public domain models.
//!
//! A typical topic implementation is split like so:
//!
//! ```text
//! topics/<topic>/mod.rs          Data flow, fact dependencies and exports
//! topics/<topic>/facts.rs        Layout values and executable references
//! topics/<topic>/validation.rs   Executable-layout checks
//! topics/<topic>/acquisition.rs  Bounded reads and sample-consistency checks
//! topics/<topic>/layout.rs       Decoding of copied native bytes
//! warframe-model/src/<topic>/    Public types and domain validation
//! ```
//!
//! Shared login resolution lives in [`crate::roots`], item-path resolution in
//! [`crate::item_type`], and token lookup in [`crate::string_pool`]. Their facts
//! stay with those modules.
//! [`crate::session`] coordinates acquisition and publication. Public models live
//! in `warframe-model`, independently of this provider's memory-reading code.

mod inventory;

pub(crate) use inventory::{InventoryError, read_inventory, validate_inventory_layout};
