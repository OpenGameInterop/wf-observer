//! Observer SDK shared by Rust and BoltFFI-generated bindings.
//!
//! Ordinary applications acquire a validated value with a bounded one-shot read:
//! ```no_run
//! # async fn example() -> Result<(), wf_observer_sdk::ObserverError> {
//! let client = wf_observer_sdk::connect_local().await?;
//! let game = client.warframe().single_session().await?;
//! let balances = game.currencies().read().await?.balances;
//! println!("Credits: {}", balances.credits);
//! client.shutdown().await?;
//! # Ok(()) }
//! ```
//! UI applications can assign watch states to their own framework's signals:
//! ```no_run
//! # async fn example(game: wf_observer_sdk::WarframeSession) -> Result<(), wf_observer_sdk::ObserverError> {
//! use wf_observer_sdk::{CurrenciesState, TryStreamExt as _};
//! let mut updates = game.currencies().watch().await?.into_stream();
//! while let Some(state) = updates.try_next().await? {
//!     match state {
//!         CurrenciesState::Ready { value } => println!("{}", value.balances.credits),
//!         CurrenciesState::Waiting => println!("Waiting"),
//!         CurrenciesState::Unavailable { reason } => println!("{reason}"),
//!     }
//! }
//! # Ok(()) }
//! ```
//! Handles are inactive. `read()` acquires temporary shared demand; `cached()`
//! only queries the service cache. Watches own demand until dropped or cancelled,
//! and include initial state. Each watch has one pending receiver. Snapshot state
//! may coalesce; chat occurrences retain order and expose explicit gaps.
//!
//! Session handles are pinned to a service run and session. Use `sessions()` and
//! `session(info)` when several games are open; `single_session()` rejects ambiguity.
//! No API automatically retargets a stale handle. [raw] supports custom
//! topic/protocol integrations.
//!
//! Generic envelope consumers can convert explicitly without a separate decoder:
//! ```no_run
//! # fn example(envelope: wf_observer_sdk::DataEnvelope) -> Result<(), wf_observer_sdk::ObserverError> {
//! let inventory = wf_observer_sdk::WarframeInventory::try_from(envelope)?;
//! println!("Account: {}", inventory.account_id);
//! # Ok(()) }
//! ```
//! `WarframeInventory::from_envelope(envelope)` exposes the same conversion to
//! generated bindings. Currencies, player data, and chat provide it too.

#[macro_use(derive)]
extern crate derive_aliases;

mod api;
mod capability;
mod client;
mod derive_alias;
mod error;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
mod local;
mod subscription;
mod topic;
pub mod warframe;
mod watch;

pub use api::{
    CapabilityDescriptor, CapabilityHealth, Catalog, ChatCapability, ChatChannel, ChatMessage,
    ChatObservation, ChatState, ChatTime, ChatUpdate, ChatWatch, CurrenciesCapability,
    CurrenciesState, CurrenciesWatch, CurrencyBalances, DataEnvelope, DiscoveryHealth,
    EnvelopeMetadata, EventEnvelope, GameDescriptor, InventoryCapability, InventoryFamily,
    InventoryFamilySnapshot, InventoryItemCount, InventoryState, InventoryWatch, ObserverClient,
    ObserverError, ObserverSubscription, PlayerCapability, PlayerState, PlayerWatch,
    ProviderDescriptor, RequestError, ResetReason, Resource, ResyncReason, ServiceCursor,
    ServiceStatus, SessionEndReason, SessionInfo, SessionRef, SessionSelector, SubscriptionEnd,
    SubscriptionItem, SubscriptionState, TargetActivity, TargetProcess, TargetStatus, TopicRef,
    TopicSnapshot, TopicSource, TopicStatus, UnavailableReason, Warframe, WarframeChatEvent,
    WarframeCurrencies, WarframeInventory, WarframePlayer, WarframeSession, connect, connect_local,
};
pub use n0_future::{Stream, StreamExt, TryStreamExt};

/// Generic protocol and custom-topic APIs. Most applications use the concrete
/// session and capability handles at the crate root instead.
pub mod raw {
    pub use crate::capability::{Capability, EventCapability};
    pub use crate::client::Client;
    pub use crate::error::ClientError;
    pub use crate::subscription::{Subscription, SubscriptionItem, SubscriptionState};
    pub use crate::topic::{
        EventTopic, SnapshotTopic, Topic, TypedData, decode_event, decode_snapshot,
    };
    pub use crate::warframe;
    pub use crate::watch::{EventObservation, EventState, EventWatch, SnapshotWatch, State};
    pub use iroh::{EndpointAddr, EndpointId};
    pub use protocol::v1 as types;
}
