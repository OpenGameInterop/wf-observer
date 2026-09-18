//! Complete retained progression from the profile's embedded inventory.
//!
//! Ownership, rebuild/sync markers and vector bounds come from
//! [`crate::profile_inventory`]. Mastery validates its own calculation and codecs;
//! it does not require the inventory topic's family registrations or demand.

mod acquisition;
mod facts;
mod layout;
mod validation;

pub(crate) use acquisition::{MasteryError, read_mastery};
pub(crate) use validation::validate_mastery_layout;
