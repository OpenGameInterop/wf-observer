//! Reads one account's inventory as a complete replacement snapshot.
//!
//! [`acquisition::read_inventory`]
//! locates inventory within profile data, reads family vectors, and rechecks
//! vector headers and update markers to reject rebuilding or changed samples.
//!
//! [`layout`] aggregates decoded counts by native item-type pointer; acquisition
//! uses [`crate::item_type`] to resolve those pointers and combines counts by path.
//!
//! Inventory layouts live in [`facts`]; shared descriptor and string-pool facts
//! come from [`crate::item_type::facts`] and [`crate::string_pool::facts`].

mod acquisition;
mod facts;
mod layout;
mod validation;

pub(crate) use acquisition::{InventoryError, read_inventory};
pub(crate) use validation::validate_inventory_layout;
