use super::feed::{Feed, FeedState};
use crate::raw::ClientError;
use parking_lot::Mutex;
use protocol::v1 as wire;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
use tokio::sync::Notify;

/// Current state across the listener's selected topics and sessions.
/// Snapshots share their storage with other listeners in this Rust client.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubscriptionState {
    pub sessions: Vec<wire::SessionInfo>,
    pub topics: Vec<Arc<wire::TopicSnapshot>>,
}

/// Local observations. State notifications may coalesce; events retain their
/// upstream order within each topic. Separate topics are not an atomic sample.
/// Buffered events expire when their session or generation ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionItem {
    State(SubscriptionState),
    Event(Arc<wire::EventEnvelope>),
    Closed(wire::SubscriptionEnd),
}

/// An independent listener on SDK-owned shared feeds.
/// Closing it releases only its interest; the last listener closes each feed.
pub struct Subscription {
    pub(super) inner: Arc<Listener>,
}

pub(super) struct Listener {
    receive_events: bool,
    queue: Mutex<Queue>,
    reading: tokio::sync::Mutex<()>,
    ready: Notify,
}

#[derive(Default)]
struct Queue {
    feeds: Vec<Arc<Feed>>,
    states: BTreeMap<wire::TopicRef, Arc<FeedState>>,
    changed: bool,
    events: VecDeque<(usize, Arc<wire::EventEnvelope>)>,
    event_bytes: usize,
    terminal: Option<Result<wire::SubscriptionEnd, ClientError>>,
    closed: bool,
}

impl Queue {
    fn state(&self) -> SubscriptionState {
        let mut sessions = BTreeMap::new();
        let mut topics = Vec::new();
        for state in self.states.values() {
            for (id, value) in &state.sessions {
                let current = sessions.entry(id).or_insert(value);
                if value.0 > current.0 {
                    *current = value;
                }
            }
            topics.extend(state.topics.values().cloned());
        }
        SubscriptionState {
            sessions: sessions.values().map(|(_, info)| info.clone()).collect(),
            topics,
        }
    }
}

impl Listener {
    pub(super) fn new(receive_events: bool) -> Self {
        Self {
            receive_events,
            queue: Mutex::default(),
            reading: tokio::sync::Mutex::default(),
            ready: Notify::new(),
        }
    }

    pub(super) fn check_open(&self) -> Result<(), ClientError> {
        let queue = self.queue.lock();
        if !queue.closed {
            return Ok(());
        }
        Err(match &queue.terminal {
            Some(Err(error)) => error.clone(),
            Some(Ok(_)) => ClientError::protocol("subscription ended during setup"),
            None => ClientError::Closed,
        })
    }

    pub(super) fn register(&self, feed: Arc<Feed>) -> bool {
        let mut queue = self.queue.lock();
        if queue.closed {
            return false;
        }
        queue.feeds.push(feed);
        true
    }

    pub(super) fn state(&self, topic: wire::TopicRef, state: Arc<FeedState>) {
        let mut queue = self.queue.lock();
        if !queue.closed {
            let mut removed_bytes = 0;
            queue.events.retain(|(size, event)| {
                let source = &event.metadata.source;
                let keep = source.topic != topic
                    || state
                        .topics
                        .get(&source.session)
                        .is_some_and(|current| current.generation == event.metadata.generation);
                if !keep {
                    removed_bytes += size;
                }
                keep
            });
            queue.event_bytes -= removed_bytes;
            queue.states.insert(topic, state);
            queue.changed = true;
            self.ready.notify_one();
        }
    }

    pub(super) fn event(&self, event: Arc<wire::EventEnvelope>) {
        if !self.receive_events {
            return;
        }
        let size = postcard::experimental::serialized_size(event.as_ref()).unwrap_or(usize::MAX);
        let mut queue = self.queue.lock();
        if queue.closed {
            return;
        }
        let budget = usize::try_from(irpc::rpc::MAX_MESSAGE_SIZE).unwrap_or(usize::MAX);
        if size > budget.saturating_sub(queue.event_bytes) {
            drop(queue);
            self.finish(Some(Err(ClientError::Lagged)));
            return;
        }
        queue.event_bytes += size;
        queue.events.push_back((size, event));
        self.ready.notify_one();
    }

    pub(super) fn finish(&self, terminal: Option<Result<wire::SubscriptionEnd, ClientError>>) {
        let feeds = {
            let mut queue = self.queue.lock();
            if queue.closed {
                if terminal.is_none() {
                    queue.terminal = None;
                }
                return;
            }
            queue.closed = true;
            queue.terminal = terminal;
            queue.states.clear();
            queue.events.clear();
            queue.event_bytes = 0;
            queue.changed = false;
            std::mem::take(&mut queue.feeds)
        };
        for feed in feeds {
            feed.detach(self);
        }
        self.ready.notify_one();
    }
}

impl Subscription {
    /// Returns the SDK's latest state, or None after closure or failure.
    /// Reading current state does not consume pending events or create demand.
    #[must_use]
    pub fn current(&self) -> Option<SubscriptionState> {
        let queue = self.inner.queue.lock();
        (!queue.closed).then(|| queue.state())
    }

    /// Receives current state, an event, or terminal completion.
    /// Cancelling this future leaves the listener active and consumes no item.
    ///
    /// # Errors
    /// Rejects concurrent next calls and reports upstream failure or local event lag.
    pub async fn next(&self) -> Result<Option<SubscriptionItem>, ClientError> {
        let _reading = self
            .inner
            .reading
            .try_lock()
            .map_err(|_| ClientError::ConcurrentNext)?;
        loop {
            let ready = self.inner.ready.notified();
            {
                let mut queue = self.inner.queue.lock();
                if let Some(terminal) = queue.terminal.take() {
                    return terminal.map(|reason| Some(SubscriptionItem::Closed(reason)));
                }
                if queue.closed {
                    return Ok(None);
                }
                if queue.changed {
                    queue.changed = false;
                    return Ok(Some(SubscriptionItem::State(queue.state())));
                }
                if let Some((size, event)) = queue.events.pop_front() {
                    queue.event_bytes -= size;
                    return Ok(Some(SubscriptionItem::Event(event)));
                }
            }
            ready.await;
        }
    }

    /// Releases this listener's interests and wakes its pending next call.
    /// Idempotent; unrelated listeners remain active.
    pub fn close(&self) {
        self.inner.finish(None);
    }

    /// Closes the listener and waits for any pending receive call to release it.
    pub async fn shutdown(&self) {
        self.close();
        let _reading = self.inner.reading.lock().await;
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.close();
    }
}
