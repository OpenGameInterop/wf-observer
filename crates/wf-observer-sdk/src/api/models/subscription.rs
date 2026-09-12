//! Managed listener state and events, adapted by the Rust SDK.

use super::{EventEnvelope, SessionInfo, SubscriptionEnd, TopicSnapshot};

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct SubscriptionState {
    pub sessions: Vec<SessionInfo>,
    pub topics: Vec<TopicSnapshot>,
}

/// State notifications may coalesce; events retain their topic's upstream order.
/// Buffered events expire when their session or generation ends.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub enum SubscriptionItem {
    State { value: SubscriptionState },
    Event { envelope: EventEnvelope },
    Closed { reason: SubscriptionEnd },
}

impl From<crate::raw::SubscriptionState> for SubscriptionState {
    fn from(value: crate::raw::SubscriptionState) -> Self {
        Self {
            sessions: value.sessions,
            topics: value
                .topics
                .into_iter()
                .map(|topic| (*topic).clone().into())
                .collect(),
        }
    }
}

impl From<crate::raw::SubscriptionItem> for SubscriptionItem {
    fn from(value: crate::raw::SubscriptionItem) -> Self {
        match value {
            crate::raw::SubscriptionItem::State(state) => Self::State {
                value: state.into(),
            },
            crate::raw::SubscriptionItem::Event(envelope) => Self::Event {
                envelope: (*envelope).clone().into(),
            },
            crate::raw::SubscriptionItem::Closed(reason) => Self::Closed { reason },
        }
    }
}
