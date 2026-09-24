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
//! Chat is best-effort live delivery. Initial history and retained entries after
//! losing a position are skipped. A new conversation includes its first observed
//! message. Gaps need no recovery; ordinary watch errors still apply.
//!
//! Chat watches provide direction and private peers without a player subscription:
//! ```no_run
//! # async fn chat(game: wf_observer_sdk::WarframeSession) -> Result<(), wf_observer_sdk::ObserverError> {
//! use wf_observer_sdk::ChatObservation;
//! let watch = game.chat().watch().await?;
//! while let Some(item) = watch.next().await? {
//!     if let ChatObservation::Message { value, .. } = item {
//!         println!("{:?} in {} with {:?}: {}", value.direction,
//!             value.conversation_id, value.peer, value.text);
//!     }
//! }
//! watch.shutdown().await?;
//! # Ok(()) }
//! ```
//! Group conversations by source/session, account, generation and the opaque
//! `conversation_id`. `sender` is always the author, including outgoing messages;
//! `peer` is the other private participant when known. Public channels have IDs
//! too, with no peer. IDs may change after removal or acquisition reset. The
//! envelope's generation/sequence already identify emitted events. Text retains
//! original markup; the optional clock supplies only game-local hour/minute.
//!
//! Remote services require an identity approved with `wf-observer peers allow <reader-id>`:
//! ```no_run
//! # async fn remote() -> Result<(), wf_observer_sdk::ObserverError> {
//! let identity = wf_observer_sdk::load_identity("my-app/reader.key".into())?;
//! println!("Reader ID: {}", identity.endpoint_id());
//! let client = identity.connect("SERVICE_ENDPOINT_ID".into()).await?;
//! client.shutdown().await?;
//! # Ok(()) }
//! ```
//! Store keys privately per application/profile. Browsers persist `secret_bytes()`
//! from `create_identity()` and reload with `restore_identity()`. Unapproved readers
//! get [`ObserverError::NotAuthorized`]; approval changes end existing watches.
//!
//! Session handles are pinned to a service run and session. Use `sessions()` and
//! `session(info)` when several games are open; `single_session()` rejects ambiguity.
//! No API automatically retargets a stale handle. [raw] supports custom
//! topic/protocol integrations.
//!
//! Unavailable topics can report [`UnavailableReason::DependencyUnavailable`]:
//! a provider-defined prerequisite label (for example, `profile data`) and a
//! structured [`DependencyFailure`] category. Status, watches, failed reads and
//! cache requests carry the same reason without serving stale data. The label
//! names the first blocking prerequisite, not every downstream layout's health.
//! Other topics continue when their own prerequisites pass. Labels are diagnostic
//! text; use the failure category for programmatic handling.
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
//!
//! Mastery exposes the game's completed rank, point totals and raw retained affinity:
//! ```no_run
//! # async fn mastery(game: wf_observer_sdk::WarframeSession) -> Result<(), wf_observer_sdk::ObserverError> {
//! let value = game.mastery().read().await?;
//! println!("Rank {}: {} mastery points", value.rank, value.total_points);
//! let mut updates = game.mastery().watch().await?.into_stream();
//! # Ok(()) }
//! ```
//! `WarframeMastery::from_envelope` validates generic snapshots. Point totals and
//! affinity are exact unsigned integers in typed clients and decimal strings in
//! raw JSON. Per-item affinity can exceed maximum-rank thresholds; item metadata
//! supplies the rules for deriving item rank and completion.
//!
//! Schema changes are versioned against released APIs; unreleased topics remain
//! at schema version 1. Mastery separates `item_points`, `mission_points`,
//! `railjack_intrinsic_points`, and `drifter_intrinsic_points`; their sum is
//! `total_points`. Railjack mastery can include retained respec credit.
//! `game.intrinsics()` supplies purchased branch ranks and whole unspent points.
//! `game.star_chart()` supplies retained node counts and Steel Path completion
//! flags. Each has independent `read`, `cached`, and `watch` methods. A retained
//! node grants Normal completion credit; absence says nothing about accessibility.

#[macro_use(derive)]
extern crate derive_aliases;

mod api;
mod capability;
mod client;
mod derive_alias;
mod error;
mod identity;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
mod local;
mod subscription;
mod topic;
pub mod warframe;
mod watch;

pub use api::{
    CapabilityDescriptor, CapabilityHealth, Catalog, ChatCapability, ChatChannel, ChatDirection,
    ChatMessage, ChatObservation, ChatState, ChatTime, ChatUpdate, ChatWatch, CurrenciesCapability,
    CurrenciesState, CurrenciesWatch, CurrencyBalances, DataEnvelope, DependencyFailure,
    DiscoveryHealth, EnvelopeMetadata, EventEnvelope, GameDescriptor, InventoryCapability,
    InventoryFamily, InventoryFamilySnapshot, InventoryItemCount, InventoryState, InventoryWatch,
    ObserverClient, ObserverError, ObserverIdentity, ObserverSubscription, PlayerCapability,
    PlayerState, PlayerWatch, ProviderDescriptor, RequestError, ResetReason, Resource,
    ResyncReason, ServiceCursor, ServiceStatus, SessionEndReason, SessionInfo, SessionRef,
    SessionSelector, SubscriptionEnd, SubscriptionItem, SubscriptionState, TargetActivity,
    TargetProcess, TargetStatus, TopicRef, TopicSnapshot, TopicSource, TopicStatus,
    UnavailableReason, Warframe, WarframeChatEvent, WarframeCurrencies, WarframeInventory,
    WarframePlayer, WarframeSession, connect, connect_local, create_identity, load_identity,
    restore_identity,
};
pub use api::{
    DrifterIntrinsics, IntrinsicsCapability, IntrinsicsState, IntrinsicsWatch, RailjackIntrinsics,
    StarChartCapability, StarChartDifficulty, StarChartNodeProgress, StarChartState,
    StarChartWatch, WarframeIntrinsics, WarframeStarChart,
};
pub use api::{
    MasteryCapability, MasteryItemProgress, MasteryState, MasteryWatch, WarframeMastery,
};
pub use api::{
    RelicRewardChoice, RelicRewardPicker, RelicRewardsCapability, RelicRewardsState,
    RelicRewardsWatch, Screen, ScreensCapability, ScreensState, ScreensWatch, WarframeRelicRewards,
    WarframeScreens,
};
pub use n0_future::{Stream, StreamExt, TryStreamExt};

/// Generic protocol and custom-topic APIs. Most applications use the concrete
/// session and capability handles at the crate root instead.
pub mod raw {
    pub use crate::capability::{Capability, EventCapability};
    pub use crate::client::Client;
    pub use crate::error::ClientError;
    pub use crate::identity::ClientIdentity;
    pub use crate::subscription::{Subscription, SubscriptionItem, SubscriptionState};
    pub use crate::topic::{
        EventTopic, SnapshotTopic, Topic, TypedData, decode_event, decode_snapshot,
    };
    pub use crate::warframe;
    pub use crate::watch::{EventObservation, EventState, EventWatch, SnapshotWatch, State};
    pub use iroh::{EndpointAddr, EndpointId};
    pub use protocol::v1 as types;
}
