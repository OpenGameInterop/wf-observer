//! Generic data containers. Game topics never become RPC operation variants.

use super::{CapabilityHealth, TopicSource};

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct EnvelopeMetadata {
    pub source: TopicSource,
    /// Nonzero reset lifetime for this session/topic, encoded as unsigned decimal text.
    /// Advances after a source change or resumed demand; old data must be discarded.
    /// Ordinary publications keep the same generation.
    pub generation: String,
    /// This publication's service-run revision, encoded as unsigned decimal text.
    /// All topics, sessions and lifecycle changes share the counter, so filtered
    /// subscriptions can have gaps. Cached snapshots retain their original revision.
    /// Deltas use this number to identify the exact snapshot they were built against.
    pub sequence: String,
}

/// Complete replacement snapshot, not a patch or raw memory dump.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct DataEnvelope {
    pub metadata: EnvelopeMetadata,
    pub payload_json: String,
}

/// Transient domain event. Delivery does not replace the cached snapshot.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct EventEnvelope {
    pub metadata: EnvelopeMetadata,
    pub payload_json: String,
}

/// One topic's atomically published health and optional current snapshot.
///
/// A snapshot is present only for Available snapshot-capable topics, and must
/// have the same source/generation. Event-only topics never carry a snapshot.
/// Idle, Initializing, and Unavailable carry none. No stale-value retention is
/// promised in v1; absence is never an empty domain value.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct TopicSnapshot {
    pub source: TopicSource,
    /// Current reset lifetime as unsigned decimal text; see [`EnvelopeMetadata::generation`].
    pub generation: String,
    pub health: CapabilityHealth,
    pub snapshot: Option<DataEnvelope>,
}
