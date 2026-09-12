//! RAII subscription ownership and authoritative per-session demand selection.

use super::{
    RETAINED_SNAPSHOT_BYTES, catalog,
    inner::Inner,
    queue::Queue,
    state::{ServiceView, Shared},
};
use protocol::v1 as wire;
use std::{
    collections::VecDeque,
    sync::{Arc, Weak},
};
use tokio::time::Instant;

pub(super) struct Subscriber {
    pub selection: wire::Subscribe,
    pub queue: Arc<Queue>,
    pub active: bool,
}

impl Subscriber {
    pub fn matches(&self, session: &wire::SessionRef, topic: &wire::TopicRef) -> bool {
        self.matches_session(session) && self.selection.topics.contains(topic)
    }

    fn matches_session(&self, session: &wire::SessionRef) -> bool {
        match &self.selection.sessions {
            wire::SessionSelector::All => true,
            wire::SessionSelector::Session { reference } => reference == session,
        }
    }

    pub fn matches_info(&self, info: &wire::SessionInfo) -> bool {
        self.matches_session(&info.session)
            && self
                .selection
                .topics
                .iter()
                .any(|t| t.provider_id == info.provider_id)
    }

    pub fn matches_update(&self, update: &wire::SubscriptionUpdate) -> bool {
        match update {
            wire::SubscriptionUpdate::SessionStarted(info)
            | wire::SubscriptionUpdate::SessionChanged(info) => self.matches_info(info),
            wire::SubscriptionUpdate::SessionEnded { session, .. } => {
                // Session IDs are tracked by the handle's current selected providers.
                self.matches_session(session)
            }
            wire::SubscriptionUpdate::TopicReset { source, .. } => {
                self.matches(&source.session, &source.topic)
            }
            wire::SubscriptionUpdate::TopicChanged(topic) => {
                self.matches(&topic.source.session, &topic.source.topic)
            }
            wire::SubscriptionUpdate::Event(event) => {
                self.matches(&event.metadata.source.session, &event.metadata.source.topic)
            }
        }
    }
}

pub(crate) struct Subscription {
    shared: Weak<Shared>,
    id: u64,
    queue: Arc<Queue>,
}

impl ServiceView {
    pub(crate) fn subscribe(
        &self,
        mut selection: wire::Subscribe,
    ) -> Result<Subscription, wire::RequestError> {
        let message_bytes = self.message_bytes();
        catalog::message_fits(&selection, message_bytes)?;
        selection.topics.sort();
        selection.topics.dedup();
        let mut inner = self.shared.inner.lock();
        inner.expire(Instant::now());
        if inner.stopped {
            return Err(catalog::invalid("service stopped"));
        }
        if selection.topics.is_empty() {
            return Err(catalog::invalid("empty topic selection"));
        }
        if let wire::SessionSelector::Session { reference: session } = &selection.sessions {
            inner.session(session)?;
        }
        for topic in &selection.topics {
            inner.capability(topic)?;
            if let wire::SessionSelector::Session { reference: session } = &selection.sessions
                && inner.session(session)?.info.provider_id != topic.provider_id
            {
                return Err(catalog::invalid("topic does not belong to session"));
            }
        }
        let id = inner
            .next_subscription
            .checked_add(1)
            .ok_or_else(|| catalog::invalid("subscription identity exhausted"))?;
        inner.next_subscription = id;
        let queue = Queue::new(message_bytes);
        inner.subscribers.insert(
            id,
            Subscriber {
                selection,
                queue: queue.clone(),
                active: false,
            },
        );
        inner.settle(Instant::now());
        let captured = inner.capture(id);
        match captured {
            Ok(frames) => {
                queue.initial(frames);
                if let Some(subscriber) = inner.subscribers.get_mut(&id) {
                    subscriber.active = true;
                }
                self.shared.changed.notify_all();
                Ok(Subscription {
                    shared: Arc::downgrade(&self.shared),
                    id,
                    queue,
                })
            }
            Err(error) => {
                inner.subscribers.remove(&id);
                inner.settle(Instant::now());
                self.shared.changed.notify_all();
                Err(error)
            }
        }
    }
}

impl Subscription {
    pub(crate) async fn next(&mut self) -> Option<wire::SubscriptionItem> {
        loop {
            // Register before checking state; Notify retains a wake permit.
            let ready = self.queue.ready.notified();
            let (item, complete) = self.queue.pop();
            if item.is_some() || complete {
                return item;
            }
            ready.await;
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            let mut inner = shared.inner.lock();
            inner.subscribers.remove(&self.id);
            inner.settle(Instant::now());
            shared.changed.notify_all();
        }
    }
}

impl Inner {
    fn capture(&self, id: u64) -> Result<VecDeque<wire::SubscriptionItem>, wire::RequestError> {
        let mut frames = VecDeque::new();
        let mut bytes = 0_usize;
        let mut push = |frame: wire::SubscriptionItem| -> Result<(), wire::RequestError> {
            catalog::message_fits(&Ok::<_, wire::RequestError>(&frame), self.message_bytes)?;
            let size =
                postcard::experimental::serialized_size(&Ok::<_, wire::RequestError>(&frame))
                    .map_err(|_| catalog::invalid("bootstrap encoding failed"))?;
            bytes = bytes
                .checked_add(size)
                .ok_or_else(|| catalog::limit(wire::Resource::InitialBytes))?;
            if bytes > RETAINED_SNAPSHOT_BYTES {
                return Err(catalog::limit(wire::Resource::InitialBytes));
            }
            frames.push_back(frame);
            Ok(())
        };
        push(wire::SubscriptionItem::Begin(self.cursor()))?;
        let subscriber = self
            .subscribers
            .get(&id)
            .ok_or_else(|| catalog::invalid("subscription closed during setup"))?;
        for session in self
            .sessions
            .values()
            .filter(|s| subscriber.matches_info(&s.info))
        {
            push(wire::SubscriptionItem::Session(session.info.clone()))?;
            for topic in session
                .topics
                .values()
                .filter(|t| subscriber.matches(&t.state.source.session, &t.state.source.topic))
            {
                push(wire::SubscriptionItem::Topic(topic.state.clone()))?;
            }
        }
        push(wire::SubscriptionItem::Ready(self.cursor()))?;
        Ok(frames)
    }
}
