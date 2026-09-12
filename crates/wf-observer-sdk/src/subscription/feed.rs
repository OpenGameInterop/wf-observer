use super::{listener::Listener, reader::Reader};
use crate::raw::ClientError;
use parking_lot::Mutex;
use protocol::v1 as wire;
use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Default)]
pub(super) struct FeedState {
    pub(super) sessions: BTreeMap<wire::SessionRef, (u64, wire::SessionInfo)>,
    pub(super) topics: BTreeMap<wire::SessionRef, Arc<wire::TopicSnapshot>>,
}

pub(super) struct Feed {
    pub(super) selection: wire::Subscribe,
    state: Mutex<State>,
    ready: Notify,
    pub(super) stop: CancellationToken,
}

#[derive(Default)]
struct State {
    listeners: Vec<Weak<Listener>>,
    current: Arc<FeedState>,
    ready: bool,
    terminal: Option<Result<wire::SubscriptionEnd, ClientError>>,
}

impl Feed {
    pub(super) fn new(selection: wire::Subscribe) -> Self {
        Self {
            selection,
            state: Mutex::new(State::default()),
            ready: Notify::new(),
            stop: CancellationToken::new(),
        }
    }

    pub(super) fn attach(&self, listener: &Arc<Listener>) -> bool {
        let mut state = self.state.lock();
        if self.stop.is_cancelled() {
            return false;
        }
        state.listeners.push(Arc::downgrade(listener));
        if state.ready {
            listener.state(self.selection.topics[0].clone(), state.current.clone());
        }
        true
    }

    pub(super) fn detach(&self, listener: &Listener) {
        let mut state = self.state.lock();
        state.listeners.retain(|weak| {
            weak.upgrade()
                .is_some_and(|other| !std::ptr::eq(other.as_ref(), listener))
        });
        if state.listeners.is_empty() {
            self.stop.cancel();
            state.current = Arc::default();
        }
    }

    pub(super) async fn wait_ready(&self) -> Result<(), ClientError> {
        loop {
            let notified = self.ready.notified();
            // notify_waiters does not retain permits, so register before inspecting.
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let state = self.state.lock();
                if let Some(terminal) = &state.terminal {
                    return Err(match terminal {
                        Err(error) => error.clone(),
                        Ok(_) => ClientError::protocol("subscription ended during setup"),
                    });
                }
                if self.stop.is_cancelled() {
                    return Err(ClientError::Closed);
                }
                if state.ready {
                    return Ok(());
                }
            }
            tokio::select! {
                () = self.stop.cancelled() => {}
                () = &mut notified => {}
            }
        }
    }

    pub(super) async fn run(
        self: Arc<Self>,
        rpc: irpc::Client<wire::ObserverProtocolV1>,
        closed: CancellationToken,
    ) {
        let result = tokio::select! {
            biased;
            () = closed.cancelled() => Err(ClientError::Closed),
            () = self.stop.cancelled() => return,
            result = self.receive(rpc) => result,
        };
        let result = if closed.is_cancelled() {
            Err(ClientError::Closed)
        } else {
            result
        };
        self.finish(&result);
    }

    async fn receive(
        &self,
        rpc: irpc::Client<wire::ObserverProtocolV1>,
    ) -> Result<wire::SubscriptionEnd, ClientError> {
        let irpc::Request::Remote(sender) = rpc.request().await.map_err(ClientError::transport)?
        else {
            return Err(ClientError::protocol("expected a remote connection"));
        };
        let (_send, recv) = sender
            .write(self.selection.clone())
            .await
            .map_err(ClientError::transport)?;
        let mut reader = Reader::new(recv, self.selection.clone());
        let mut sequence = 0;
        loop {
            match reader.next().await? {
                wire::SubscriptionItem::Begin(cursor) => sequence = cursor.sequence,
                wire::SubscriptionItem::Session(info) => self.change(|state| {
                    state
                        .sessions
                        .insert(info.session.clone(), (sequence, info));
                }),
                wire::SubscriptionItem::Topic(topic) => self.change(|state| {
                    state
                        .topics
                        .insert(topic.source.session.clone(), Arc::new(topic));
                }),
                wire::SubscriptionItem::Ready(_) => {
                    let mut state = self.state.lock();
                    state.ready = true;
                    self.publish_state(&state);
                    self.ready.notify_waiters();
                }
                wire::SubscriptionItem::Closed(reason) => return Ok(reason),
                wire::SubscriptionItem::Update(update) => {
                    sequence = update.cursor.sequence;
                    match update.update {
                        wire::SubscriptionUpdate::SessionStarted(info)
                        | wire::SubscriptionUpdate::SessionChanged(info) => self.change(|state| {
                            state
                                .sessions
                                .insert(info.session.clone(), (sequence, info));
                        }),
                        wire::SubscriptionUpdate::SessionEnded { session, .. } => {
                            self.change(|state| {
                                state.sessions.remove(&session);
                                state.topics.remove(&session);
                            });
                        }
                        wire::SubscriptionUpdate::TopicReset {
                            source, generation, ..
                        } => self.change(|state| {
                            state.topics.insert(
                                source.session.clone(),
                                Arc::new(wire::TopicSnapshot {
                                    source,
                                    generation,
                                    health: wire::CapabilityHealth::Initializing,
                                    snapshot: None,
                                }),
                            );
                        }),
                        wire::SubscriptionUpdate::TopicChanged(topic) => self.change(|state| {
                            state
                                .topics
                                .insert(topic.source.session.clone(), Arc::new(topic));
                        }),
                        wire::SubscriptionUpdate::Event(event) => {
                            let event = Arc::new(event);
                            for listener in self.listeners() {
                                listener.event(event.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    fn change(&self, update: impl FnOnce(&mut FeedState)) {
        let mut state = self.state.lock();
        update(Arc::make_mut(&mut state.current));
        self.publish_state(&state);
    }

    fn publish_state(&self, state: &State) {
        if state.ready {
            for listener in state.listeners.iter().filter_map(Weak::upgrade) {
                listener.state(self.selection.topics[0].clone(), state.current.clone());
            }
        }
    }

    fn listeners(&self) -> Vec<Arc<Listener>> {
        self.state
            .lock()
            .listeners
            .iter()
            .filter_map(Weak::upgrade)
            .collect()
    }

    pub(super) fn finish(&self, terminal: &Result<wire::SubscriptionEnd, ClientError>) {
        let listeners = {
            let mut state = self.state.lock();
            self.stop.cancel();
            state.current = Arc::default();
            state.terminal = Some(terminal.clone());
            state
                .listeners
                .iter()
                .filter_map(Weak::upgrade)
                .collect::<Vec<_>>()
        };
        for listener in listeners {
            listener.finish(Some(terminal.clone()));
        }
        self.ready.notify_waiters();
    }
}
