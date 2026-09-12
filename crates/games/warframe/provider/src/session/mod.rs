//! Coordinates demanded topics within one process attachment.
//!
//! Executable identity, login layout, string layout, item-type layout and topic
//! layouts have independent cached checks. Passing one does not enable another.
//! Failed checks retain their own retry deadlines; successful checks last for
//! the attachment. Live ownership and sample consistency are checked each time
//! data is acquired, even after executable validation has passed.

mod health;
mod polling;
mod validation;

pub(crate) use polling::{CHAT, CURRENCIES, INVENTORY, PLAYER, WarframeSession};
