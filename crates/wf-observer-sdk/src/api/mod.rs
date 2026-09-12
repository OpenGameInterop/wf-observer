//! Shared concrete consumer API, used directly by Rust and exported through `BoltFFI`.
//!
//! Raw JSON payloads and envelope generation/sequence counters are exposed as text.

pub(crate) mod client;
pub(crate) mod error;
pub(crate) mod models;
mod runtime;
pub(crate) mod subscription;

pub use client::{ObserverClient, connect, connect_local};
pub use error::ObserverError;
pub use models::*;
pub use subscription::ObserverSubscription;
