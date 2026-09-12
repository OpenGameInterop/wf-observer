//! Initial state, ordered updates, reset, and terminal subscription outcomes.

use super::{
    EventEnvelope, Resource, ServiceCursor, SessionInfo, SessionRef, TopicSnapshot, TopicSource,
};

/// Scope is explicit; All follows matching sessions across their lifetimes.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub enum SessionSelector {
    All,
    Session { reference: SessionRef },
}

/// Ordered frames after a successful subscription request.
///
/// Begin, zero or more Session/Topic frames, then Ready form one coherent
/// bootstrap. Updates follow Ready. Closed is terminal and cannot be followed by
/// another item. Unexpected EOF/error is an incomplete stream, never success.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub enum SubscriptionItem {
    Begin(ServiceCursor),
    Session(SessionInfo),
    Topic(TopicSnapshot),
    Ready(ServiceCursor),
    Update(UpdateEnvelope),
    Closed(SubscriptionEnd),
}

#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct UpdateEnvelope {
    pub cursor: ServiceCursor,
    pub update: SubscriptionUpdate,
}

/// Generic lifecycle/data changes, not game events.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub enum SubscriptionUpdate {
    SessionStarted(SessionInfo),
    /// Metadata changed without replacing the attachment, e.g. build resolution.
    SessionChanged(SessionInfo),
    SessionEnded {
        session: SessionRef,
        reason: SessionEndReason,
    },
    /// Immediately discards the old generation and puts this topic in Initializing.
    TopicReset {
        source: TopicSource,
        generation: u64,
        reason: ResetReason,
    },
    /// Atomically replaces topic health and its current snapshot.
    TopicChanged(TopicSnapshot),
    Event(EventEnvelope),
}

#[boltffi::data]
#[derive(Debug, ..Copy, ..Eq, ..Serde)]
pub enum ResetReason {
    /// Provider-owned roots or compatibility changed within the same process.
    SourceChanged,
    /// Polling resumes after a period in which data was not being observed.
    DemandResumed,
}

#[boltffi::data]
#[derive(Debug, ..Copy, ..Eq, ..Serde)]
pub enum SessionEndReason {
    TargetExited,
    ProviderFailed,
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub enum SubscriptionEnd {
    /// Only session-specific subscriptions complete when their target session ends.
    SessionEnded,
    ServiceStopped,
    /// Discard cached subscription state; resubscribe for a fresh bootstrap.
    ResyncRequired {
        reason: ResyncReason,
    },
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub enum ResyncReason {
    Lagged {
        resource: Resource,
    },
    /// Sequence or generation space was exhausted; values must never wrap.
    SequenceExhausted,
}
