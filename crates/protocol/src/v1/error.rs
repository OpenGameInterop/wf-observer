//! Application errors are separate from connection/framing failures.

use derive_more::Error;
use displaydoc::Display;

use super::{TopicRef, UnavailableReason};

/// Application-level request rejection. No acquisition occurs on the RPC task.
#[boltffi::data]
#[derive(Debug, Clone, Display, Error, ..Eq, ..Serde)]
#[allow(clippy::doc_markdown, reason = "displaydoc field placeholders are format syntax")]
pub enum RequestError {
    /// service restarted; current run is {run_id}
    ServiceRestarted { run_id: String },
    /// unknown session: {session_id}
    UnknownSession { session_id: String },
    /// unknown provider: {provider_id}
    UnknownProvider { provider_id: String },
    /// unknown topic {topic} for provider {provider_id}
    UnknownTopic { provider_id: String, topic: String },
    /// unsupported schema for {requested:?}; supported versions: {supported:?}
    UnsupportedSchema {
        requested: TopicRef,
        supported: Vec<u32>,
    },
    /// snapshots are not supported for {topic:?}
    SnapshotsUnsupported { topic: TopicRef },
    /// topic is idle; subscribe to initiate acquisition
    Idle,
    /// the first sample is not available yet
    NotSampled,
    /// capability is unavailable: {reason}
    Unavailable {
        #[error(not(source))]
        reason: UnavailableReason,
    },
    /// invalid request: {message}
    InvalidRequest { message: String },
    /// service resource limit exceeded: {resource:?}
    LimitExceeded { resource: Resource },
}

/// Identifies a bounded resource without exposing host implementation details.
#[boltffi::data]
#[derive(Debug, ..Copy, ..Eq, ..Serde)]
#[allow(clippy::enum_variant_names, reason = "Error resources name their accounting unit")]
pub enum Resource {
    MessageBytes,
    QueuedBytes,
    InitialBytes,
    RetainedSnapshotBytes,
    Subscriptions,
}
