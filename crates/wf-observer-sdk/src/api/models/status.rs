//! Service lifecycle and per-topic readiness, separate from compiled support.

use super::{CapabilityHealth, DiscoveryHealth, ServiceCursor, TargetProcess, TopicRef};

/// A coherent status view at the supplied service checkpoint.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct ServiceStatus {
    pub cursor: ServiceCursor,
    pub application_version: String,
    pub discovery: DiscoveryHealth,
    pub targets: Vec<TargetStatus>,
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct TargetStatus {
    pub provider_id: String,
    pub game_id: String,
    pub target: TargetProcess,
    pub activity: TargetActivity,
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub enum TargetActivity {
    Attaching,
    Retrying {
        message: String,
    },
    Observing {
        session_id: String,
        /// Independent of provider, wire, and topic versions; absent until resolved.
        game_build: Option<String>,
        topics: Vec<TopicStatus>,
    },
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct TopicStatus {
    pub topic: TopicRef,
    /// Current reset lifetime as unsigned decimal text; see [`super::EnvelopeMetadata::generation`].
    pub generation: String,
    pub health: CapabilityHealth,
}
