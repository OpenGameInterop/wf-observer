//! Shared concrete consumer API, used directly by Rust and exported through `BoltFFI`.
//!
//! Raw JSON payloads and envelope generation/sequence counters are exposed as text.

pub(crate) mod chat;
pub(crate) mod chat_watch;
pub(crate) mod client;
pub(crate) mod currencies;
pub(crate) mod currencies_watch;
pub(crate) mod error;
pub(crate) mod inventory;
pub(crate) mod inventory_watch;
pub(crate) mod models;
pub(crate) mod player;
pub(crate) mod player_watch;
mod runtime;
mod stream;
pub(crate) mod subscription;
pub(crate) mod warframe;

pub use chat::{ChatChannel, ChatMessage, ChatTime, ChatUpdate, WarframeChatEvent};
pub use chat_watch::{ChatCapability, ChatObservation, ChatState, ChatWatch};
pub use client::{ObserverClient, connect, connect_local};
pub use currencies::{CurrencyBalances, WarframeCurrencies};
pub use currencies_watch::{CurrenciesCapability, CurrenciesState, CurrenciesWatch};
pub use error::ObserverError;
pub use inventory::*;
pub use inventory_watch::{InventoryCapability, InventoryState, InventoryWatch};
pub use models::*;
pub use player::WarframePlayer;
pub use player_watch::{PlayerCapability, PlayerState, PlayerWatch};
pub use subscription::ObserverSubscription;
pub use warframe::{Warframe, WarframeSession};
