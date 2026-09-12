//! Generic data containers. Game topics never become RPC operation variants.

use super::{CapabilityHealth, JsonPayload, TopicSource};

#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct EnvelopeMetadata {
    pub source: TopicSource,
    /// Nonzero reset lifetime for this session/topic, not a publication revision.
    /// Advances after a source change or resumed demand; old data must be discarded.
    /// Ordinary publications keep the same generation.
    pub generation: u64,
    /// This publication's revision in the service run's update order.
    /// All topics, sessions and lifecycle changes share the counter, so filtered
    /// subscriptions can have gaps. Cached snapshots retain their original revision.
    /// Deltas use this number to identify the exact snapshot they were built against.
    pub sequence: u64,
}

/// Complete replacement snapshot.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct DataEnvelope {
    pub metadata: EnvelopeMetadata,
    pub payload: JsonPayload,
}

/// Transient domain event. Delivery does not replace the cached snapshot.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct EventEnvelope {
    pub metadata: EnvelopeMetadata,
    pub payload: JsonPayload,
}

/// One topic's atomically published health and optional current snapshot.
///
/// A snapshot is present only for Available snapshot-capable topics, and must
/// have the same source/generation. Event-only topics never carry a snapshot.
/// Idle, Initializing, and Unavailable carry none. No stale-value retention is
/// promised in v1; absence is never an empty domain value.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct TopicSnapshot {
    pub source: TopicSource,
    /// Current reset lifetime; see [`EnvelopeMetadata::generation`].
    pub generation: u64,
    pub health: CapabilityHealth,
    pub snapshot: Option<DataEnvelope>,
}
