mod catalog;
mod data;
mod delta;
mod error;
mod identity;
mod payload;
mod requests;
mod status;
mod subscription;

/// QUIC application close code for an authenticated peer without reader approval.
pub const NOT_AUTHORIZED_CLOSE_CODE: u32 = 0x5746_0001;

pub use catalog::{CapabilityDescriptor, Catalog, GameDescriptor, ProviderDescriptor};
pub use data::{DataEnvelope, EnvelopeMetadata, EventEnvelope, TopicSnapshot};
pub use delta::{SnapshotAck, SnapshotFrame, SnapshotPayload};
pub use error::{RequestError, Resource};
pub use identity::{ServiceCursor, SessionRef, TopicRef, TopicSource};
pub use payload::JsonPayload;
pub use requests::{
    ALPN_V1, GetCatalog, GetSnapshot, GetStatus, ObserverMessageV1, ObserverProtocolV1, Ping, Pong,
    Subscribe,
};
pub use status::{
    CapabilityHealth, DiscoveryHealth, ServiceStatus, SessionInfo, TargetActivity, TargetProcess,
    TargetStatus, TopicStatus, UnavailableReason,
};
pub use subscription::{
    ResetReason, ResyncReason, SessionEndReason, SessionSelector, SubscriptionEnd,
    SubscriptionItem, SubscriptionUpdate, UpdateEnvelope,
};
