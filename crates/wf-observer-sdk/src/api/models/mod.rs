//! Shared protocol types and binding adapters. Payloads are JSON text;
//! u64 counters/generations are decimal strings, preserving unsigned values
//! across every generated language.

mod convert;
pub(crate) mod data;
pub(crate) mod identity;
pub(crate) mod status;
pub(crate) mod subscription;

pub use data::{DataEnvelope, EnvelopeMetadata, EventEnvelope, TopicSnapshot};
pub use identity::ServiceCursor;
pub use protocol::v1::{
    CapabilityDescriptor, CapabilityHealth, Catalog, DiscoveryHealth, GameDescriptor,
    ProviderDescriptor, RequestError, ResetReason, Resource, ResyncReason, SessionEndReason,
    SessionInfo, SessionRef, SessionSelector, SubscriptionEnd, TargetProcess, TopicRef,
    TopicSource, UnavailableReason,
};
pub use status::{ServiceStatus, TargetActivity, TargetStatus, TopicStatus};
pub use subscription::{SubscriptionItem, SubscriptionState};
