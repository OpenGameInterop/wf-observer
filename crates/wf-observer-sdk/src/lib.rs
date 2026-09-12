//! Observer SDK shared by Rust and BoltFFI-generated bindings.
//! Validated feeds share demand across independent subscriptions, reads and watches.

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
mod watch;

pub use api::{
    CapabilityDescriptor, CapabilityHealth, Catalog, DataEnvelope, DiscoveryHealth,
    EnvelopeMetadata, EventEnvelope, GameDescriptor, ObserverClient, ObserverError,
    ObserverSubscription, ProviderDescriptor, RequestError, ResetReason, Resource, ResyncReason,
    ServiceCursor, ServiceStatus, SessionEndReason, SessionInfo, SessionRef, SessionSelector,
    SubscriptionEnd, SubscriptionItem, SubscriptionState, TargetActivity, TargetProcess,
    TargetStatus, TopicRef, TopicSnapshot, TopicSource, TopicStatus, UnavailableReason, connect,
    connect_local,
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
    pub use crate::watch::{EventObservation, EventState, EventWatch, SnapshotWatch, State};
    pub use iroh::{EndpointAddr, EndpointId};
    pub use protocol::v1 as types;
}
