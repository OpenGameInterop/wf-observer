//! Shared concrete consumer API, used directly by Rust and exported through `BoltFFI`.
//!
//! Raw JSON payloads and envelope generation/sequence counters are exposed as text.

pub(crate) mod client;
pub(crate) mod currencies;
pub(crate) mod currencies_watch;
pub(crate) mod error;
pub(crate) mod inventory;
pub(crate) mod inventory_watch;
pub(crate) mod models;
mod runtime;
mod stream;
pub(crate) mod subscription;
pub(crate) mod warframe;

pub use client::{ObserverClient, connect, connect_local};
pub use currencies::{CurrencyBalances, WarframeCurrencies};
pub use currencies_watch::{CurrenciesCapability, CurrenciesState, CurrenciesWatch};
pub use error::ObserverError;
pub use inventory::*;
pub use inventory_watch::{InventoryCapability, InventoryState, InventoryWatch};
pub use models::*;
pub use subscription::ObserverSubscription;
pub use warframe::{Warframe, WarframeSession};
