use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use parking_lot::Mutex;
use protocol::v1 as wire;
use std::sync::Arc;
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    task::JoinSet,
};
use tokio_util::task::TaskTracker;

use super::requests;
use crate::service::ServiceView;

const CONNECTIONS: usize = 16;
const REQUESTS: usize = 128;
const REQUESTS_PER_CONNECTION: usize = 32;
pub(super) const REJECTED: u32 = 1;

#[derive(Debug, ..Copy)]
pub(super) struct SubscriptionLimits {
    pub(super) per_connection: usize,
    pub(super) global: usize,
}

impl Default for SubscriptionLimits {
    fn default() -> Self {
        // The SDK opens one stream per topic/scope. Allow four complete Warframe
        // session scopes per connection while bounding aggregate queued state.
        Self {
            per_connection: 16,
            global: 32,
        }
    }
}

#[derive(derive_more::Debug)]
pub(super) struct Handler {
    #[debug(skip)]
    view: ServiceView,
    message_bytes: u32,
    connections: Semaphore,
    requests: Arc<Semaphore>,
    subscriptions: Arc<Semaphore>,
    subscriptions_per_connection: usize,
    tracked: TaskTracker,
}

impl Handler {
    pub(super) fn new(
        view: ServiceView,
        message_bytes: u32,
        tracked: TaskTracker,
        limits: SubscriptionLimits,
    ) -> Self {
        Self {
            view,
            message_bytes,
            connections: Semaphore::new(CONNECTIONS),
            requests: Arc::new(Semaphore::new(REQUESTS)),
            subscriptions: Arc::new(Semaphore::new(limits.global)),
            subscriptions_per_connection: limits.per_connection,
            tracked,
        }
    }
}

impl ProtocolHandler for Handler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let Ok(_connection_slot) = self.connections.try_acquire() else {
            connection.close(REJECTED.into(), b"connection limit");
            return Ok(());
        };
        let per_connection = Arc::new(Semaphore::new(REQUESTS_PER_CONNECTION));
        let streams = Arc::new(Streams {
            connection: connection.clone(),
            active: Mutex::new(0),
            global: self.subscriptions.clone(),
            local: Arc::new(Semaphore::new(self.subscriptions_per_connection)),
        });
        let mut tasks = JoinSet::new();
        loop {
            let incoming = tokio::select! {
                biased;
                Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                    if let Err(error) = result {
                        tracing::warn!(%error, "RPC request task failed");
                    }
                    continue;
                }
                incoming = connection.accept_bi() => incoming,
            };
            let Ok((mut send, mut recv)) = incoming else {
                break;
            };
            let (Ok(global), Ok(local)) = (
                self.requests.clone().try_acquire_owned(),
                per_connection.clone().try_acquire_owned(),
            ) else {
                let _ = send.reset(REJECTED.into());
                let _ = recv.stop(REJECTED.into());
                continue;
            };
            let view = self.view.clone();
            let message_bytes = self.message_bytes;
            let streams = streams.clone();
            tasks.spawn(self.tracked.track_future(async move {
                requests::serve(view, message_bytes, send, recv, (global, local), streams).await;
            }));
        }
        // Aborting a connection's requests drops every subscription before the
        // accepted connection releases its resource slot.
        tasks.shutdown().await;
        Ok(())
    }
}

/// Active subscriptions get stream credit in addition to request-processing headroom.
pub(super) struct Streams {
    connection: Connection,
    active: Mutex<u32>,
    global: Arc<Semaphore>,
    local: Arc<Semaphore>,
}

impl Streams {
    pub(super) fn open(self: &Arc<Self>) -> Result<StreamSlot, wire::RequestError> {
        let limit = || wire::RequestError::LimitExceeded {
            resource: wire::Resource::Subscriptions,
        };
        let global = self
            .global
            .clone()
            .try_acquire_owned()
            .map_err(|_| limit())?;
        let local = self
            .local
            .clone()
            .try_acquire_owned()
            .map_err(|_| limit())?;
        let mut active = self.active.lock();
        let next = active.checked_add(1).ok_or_else(limit)?;
        let headroom = u32::try_from(REQUESTS_PER_CONNECTION).map_err(|_| limit())?;
        let credit = next.checked_add(headroom).ok_or_else(limit)?;
        self.connection.set_max_concurrent_bi_streams(credit.into());
        *active = next;
        Ok(StreamSlot {
            streams: self.clone(),
            _global: global,
            _local: local,
        })
    }
}

pub(super) struct StreamSlot {
    streams: Arc<Streams>,
    _global: OwnedSemaphorePermit,
    _local: OwnedSemaphorePermit,
}

impl Drop for StreamSlot {
    fn drop(&mut self) {
        let mut active = self.streams.active.lock();
        *active -= 1;
        if let Ok(headroom) = u32::try_from(REQUESTS_PER_CONNECTION) {
            self.streams
                .connection
                .set_max_concurrent_bi_streams((*active + headroom).into());
        }
    }
}
