use parking_lot::Mutex;
use std::{marker::PhantomData, sync::Arc};

use crate::raw::{
    Client, ClientError, EventTopic, SnapshotTopic, Subscription, SubscriptionItem,
    SubscriptionState, TypedData, decode_event, decode_snapshot, types,
};

/// Current typed availability. Unavailable data is never represented by a default value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State<T> {
    Waiting,
    Ready(Arc<TypedData<T>>),
    Unavailable(types::UnavailableReason),
}

/// Event source health and reset lifetime. A missing generation means a new
/// coherent baseline is being acquired. Compare generations only within this watch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventState {
    pub generation: Option<u64>,
    pub health: types::CapabilityHealth,
}

/// Events retain order; source state may coalesce. Already-delivered events are
/// historical values: consumers choose retention and clear/annotate on source changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventObservation<T> {
    State(EventState),
    Event(Arc<TypedData<T>>),
}

struct Watch {
    // Handles and watches keep the connection alive without requiring caller Arc wrappers.
    _client: Client,
    session: types::SessionRef,
    subscription: Subscription,
}

impl Watch {
    fn current(&self) -> Result<SubscriptionState, ClientError> {
        self.subscription.current().ok_or(ClientError::Closed)
    }

    async fn next(&self) -> Result<Option<SubscriptionItem>, ClientError> {
        match self.subscription.next().await? {
            Some(SubscriptionItem::Closed(reason)) => Err(ClientError::Ended(reason)),
            item => Ok(item),
        }
    }

    fn topic<T: crate::raw::Topic>(
        &self,
        state: SubscriptionState,
    ) -> Result<Option<Arc<types::TopicSnapshot>>, ClientError> {
        let topic = state
            .topics
            .into_iter()
            .find(|topic| topic.source.session == self.session && topic.source.topic == T::topic());
        if topic
            .as_ref()
            .is_some_and(|topic| topic.source.game_id != T::GAME_ID)
        {
            return Err(ClientError::WrongTopic);
        }
        Ok(topic)
    }
}

type Decoded<T> = (Arc<types::TopicSnapshot>, Arc<TypedData<T>>);

/// Independent snapshot watch over the existing shared feed. The initial state
/// is queued by setup, so consumers never need a read-then-subscribe sequence.
/// Events published by the same topic are ignored.
pub struct SnapshotWatch<T: SnapshotTopic> {
    inner: Watch,
    decoded: Mutex<Option<Decoded<T::Snapshot>>>,
}

impl<T: SnapshotTopic> SnapshotWatch<T> {
    pub(crate) fn new(
        client: Client,
        session: types::SessionRef,
        subscription: Subscription,
    ) -> Self {
        Self {
            inner: Watch {
                _client: client,
                session,
                subscription,
            },
            decoded: Mutex::new(None),
        }
    }

    fn decode(&self, state: SubscriptionState) -> Result<State<T::Snapshot>, ClientError> {
        let topic = self.inner.topic::<T>(state)?;
        let mut cached = self.decoded.lock();
        let Some(topic) = topic else {
            *cached = None;
            return Ok(State::Waiting);
        };
        if let Some((previous, value)) = cached.as_ref()
            && Arc::ptr_eq(previous, &topic)
        {
            return Ok(State::Ready(value.clone()));
        }
        *cached = None;
        match &topic.health {
            types::CapabilityHealth::Available => {
                let envelope = topic.snapshot.as_ref().ok_or_else(|| {
                    ClientError::protocol("available snapshot topic has no value")
                })?;
                let value = Arc::new(decode_snapshot::<T>(envelope.clone())?);
                *cached = Some((topic, value.clone()));
                Ok(State::Ready(value))
            }
            types::CapabilityHealth::Unavailable { reason } => {
                Ok(State::Unavailable(reason.clone()))
            }
            types::CapabilityHealth::Idle | types::CapabilityHealth::Initializing => {
                Ok(State::Waiting)
            }
        }
    }

    /// Reads current typed state without consuming updates. Repeated reads of
    /// the same snapshot share decoded storage. Closed watches expose no current data.
    ///
    /// # Errors
    /// Returns closed-watch or decoding errors.
    pub fn current(&self) -> Result<State<T::Snapshot>, ClientError> {
        self.decode(self.inner.current()?)
    }

    /// Receives initial state or a subsequent replacement. Cancellation consumes
    /// no item. Only one receive may be pending. Upstream termination is an error
    /// once, followed by None; local close returns None.
    ///
    /// # Errors
    /// Returns transport, termination, concurrent receive, or decoding errors.
    pub async fn next(&self) -> Result<Option<State<T::Snapshot>>, ClientError> {
        let result = match self.inner.next().await? {
            Some(SubscriptionItem::State(state)) => self.decode(state).map(Some),
            Some(_) => Err(ClientError::protocol("snapshot watch received an event")),
            None => Ok(None),
        };
        if result.is_err() {
            self.close();
        }
        result
    }

    /// Releases demand immediately and wakes pending receives. Idempotent.
    pub fn close(&self) {
        self.inner.subscription.close();
        self.decoded.lock().take();
    }

    /// Releases demand and waits for pending receives to finish.
    pub async fn shutdown(&self) {
        self.close();
        self.inner.subscription.shutdown().await;
    }

    /// Converts this watch into an owned standard Rust stream. Dropping the stream
    /// also drops the watch, including while its next item is pending.
    #[must_use]
    pub fn into_stream(
        self,
    ) -> n0_future::boxed::BoxStream<Result<State<T::Snapshot>, ClientError>> {
        Box::pin(n0_future::stream::unfold(Some(self), |watch| async move {
            let watch = watch?;
            match watch.next().await {
                Ok(Some(state)) => Some((Ok(state), Some(watch))),
                Ok(None) => None,
                Err(error) => Some((Err(error), None)),
            }
        }))
    }
}

/// Independent typed event watch. Source state and ordered occurrences remain distinct.
pub struct EventWatch<T: EventTopic> {
    inner: Watch,
    topic: PhantomData<T>,
}

impl<T: EventTopic> EventWatch<T> {
    pub(crate) fn new(
        client: Client,
        session: types::SessionRef,
        subscription: Subscription,
    ) -> Self {
        Self {
            inner: Watch {
                _client: client,
                session,
                subscription,
            },
            topic: PhantomData,
        }
    }

    fn decode_state(&self, state: SubscriptionState) -> Result<EventState, ClientError> {
        Ok(self.inner.topic::<T>(state)?.map_or(
            EventState {
                generation: None,
                health: types::CapabilityHealth::Initializing,
            },
            |topic| EventState {
                generation: Some(topic.generation),
                health: topic.health.clone(),
            },
        ))
    }

    /// Returns current source health without consuming events.
    ///
    /// # Errors
    /// Returns closed-watch or identity errors.
    pub fn current(&self) -> Result<EventState, ClientError> {
        self.decode_state(self.inner.current()?)
    }

    /// Receives source state or a decoded event; never conflates individual events.
    /// Cancellation consumes no event. Only one receive may be pending.
    ///
    /// # Errors
    /// Returns transport, termination, lag, concurrent receive, or decoding errors.
    pub async fn next(&self) -> Result<Option<EventObservation<T::Event>>, ClientError> {
        let result = match self.inner.next().await? {
            Some(SubscriptionItem::State(state)) => self
                .decode_state(state)
                .map(|state| Some(EventObservation::State(state))),
            Some(SubscriptionItem::Event(envelope)) => decode_event::<T>((*envelope).clone())
                .map(|event| Some(EventObservation::Event(Arc::new(event)))),
            Some(SubscriptionItem::Closed(reason)) => Err(ClientError::Ended(reason)),
            None => Ok(None),
        };
        if result.is_err() {
            self.close();
        }
        result
    }

    /// Releases demand and wakes pending receives. Idempotent.
    pub fn close(&self) {
        self.inner.subscription.close();
    }
    /// Releases demand and joins pending receives.
    pub async fn shutdown(&self) {
        self.inner.subscription.shutdown().await;
    }
    /// Converts into a standard stream whose lifetime owns the watch.
    #[must_use]
    pub fn into_stream(
        self,
    ) -> n0_future::boxed::BoxStream<Result<EventObservation<T::Event>, ClientError>> {
        Box::pin(n0_future::stream::unfold(Some(self), |watch| async move {
            let watch = watch?;
            match watch.next().await {
                Ok(Some(item)) => Some((Ok(item), Some(watch))),
                Ok(None) => None,
                Err(error) => Some((Err(error), None)),
            }
        }))
    }
}
