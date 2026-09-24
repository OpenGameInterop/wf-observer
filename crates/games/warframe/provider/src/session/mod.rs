//! Coordinates demanded topics within one process attachment.
//!
//! Executable identity, account identity, profile data, profile commit markers,
//! embedded inventory, client/world, strings, item types and topic
//! layouts have independent cached checks. Passing one does not enable another.
//! Failed checks retain their own retry deadlines; successful checks last for
//! the attachment. Live ownership and sample consistency are checked each time
//! data is acquired, even after executable validation has passed.
//!
//! Player needs only account identity. Currencies and chat also need profile
//! data; inventory/mastery additionally need embedded inventory and commit
//! markers. Intrinsics and Star Chart use the commit markers without depending
//! on the inventory getter. Screens need client/UI ownership; relic rewards add
//! account and world ownership, but never profile-data ownership.

mod health;
mod polling;
mod screen_demand;
mod validation;
mod visual;

#[cfg(test)]
mod fixture;

pub(crate) use polling::{
    CHAT, CURRENCIES, INTRINSICS, INVENTORY, MASTERY, PLAYER, STAR_CHART, WarframeSession,
};
pub(crate) use visual::{RELIC_REWARDS, SCREENS};
