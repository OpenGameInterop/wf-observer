//! Service lifecycle and per-topic readiness, separate from compiled support.

use super::{ServiceCursor, SessionRef, TopicRef};

/// A coherent status view at the supplied service checkpoint.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct ServiceStatus {
    pub cursor: ServiceCursor,
    pub application_version: String,
    pub discovery: DiscoveryHealth,
    pub targets: Vec<TargetStatus>,
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub enum DiscoveryHealth {
    Searching,
    Retrying { message: String },
}

/// Display metadata only. PID is not a session identity.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct TargetProcess {
    pub pid: u32,
    pub executable: String,
}

#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct TargetStatus {
    pub provider_id: String,
    pub game_id: String,
    pub target: TargetProcess,
    pub activity: TargetActivity,
}

#[derive(Debug, Clone, ..Eq, ..Serde)]
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

/// Active session metadata included in subscription bootstrap and arrival updates.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct SessionInfo {
    pub session: SessionRef,
    pub provider_id: String,
    pub game_id: String,
    pub target: TargetProcess,
    pub game_build: Option<String>,
}

#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct TopicStatus {
    pub topic: TopicRef,
    /// Current reset lifetime; see [`super::EnvelopeMetadata::generation`].
    pub generation: u64,
    pub health: CapabilityHealth,
}

/// Whether a declared capability currently supplies validated data.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub enum CapabilityHealth {
    /// No subscriber demand. No snapshot is presented as current.
    Idle,
    /// Demanded, but no successful acquisition in the current generation yet.
    Initializing,
    /// Acquiring successfully; event topics need not have emitted any events.
    Available,
    /// Demanded but unable to supply validated data. No current snapshot.
    Unavailable { reason: UnavailableReason },
}

/// Safe, consumer-facing reasons; diagnostic messages must omit memory contents,
/// process addresses, credentials, and other private acquisition details.
#[boltffi::data]
#[derive(Debug, Clone, displaydoc::Display, ..Eq, ..Serde)]
pub enum UnavailableReason {
    /// the game is not ready
    TargetNotReady,
    /// this game build is unsupported
    UnsupportedBuild,
    /// memory acquisition failed: {message}
    ReadFailed { message: String },
    /// acquired data failed validation: {message}
    ValidationFailed { message: String },
    /// provider operation failed: {message}
    ProviderFailed { message: String },
    /// blocked by {dependency}: {failure}
    DependencyUnavailable {
        dependency: String,
        failure: DependencyFailure,
    },
}

/// Failure category for a named prerequisite, independent of any game or topic.
#[boltffi::data]
#[derive(Debug, displaydoc::Display, ..Copy, ..Eq, ..Serde)]
pub enum DependencyFailure {
    /// not ready
    TargetNotReady,
    /// unsupported build
    UnsupportedBuild,
    /// read failed
    ReadFailed,
    /// validation failed
    ValidationFailed,
    /// provider failed
    ProviderFailed,
}
