//! Local listeners backed by shared, validated upstream topic feeds.

mod feed;
mod listener;
mod manager;
mod reader;
mod validation;

pub use listener::{Subscription, SubscriptionItem, SubscriptionState};
pub(crate) use manager::Manager;

#[cfg(test)]
mod tests;
