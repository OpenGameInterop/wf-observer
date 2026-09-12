//! Version-one RPC request shapes.

use irpc::{
    channel::{mpsc, oneshot},
    rpc_requests,
};

use super::{
    Catalog, DataEnvelope, RequestError, ServiceStatus, SessionRef, SessionSelector,
    SubscriptionItem, TopicRef,
};

pub const ALPN_V1: &[u8] = b"wf-observer/1";

#[derive(Debug, ..Copy, ..Eq, ..Serde)]
pub struct Ping;

#[derive(Debug, ..Copy, ..Eq, ..Serde)]
pub struct Pong;

#[derive(Debug, ..Copy, ..Eq, ..Serde)]
pub struct GetCatalog;

#[derive(Debug, ..Copy, ..Eq, ..Serde)]
pub struct GetStatus;

/// Cache-only lookup. Never initiates or waits for a native acquisition.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct GetSnapshot {
    pub session: SessionRef,
    pub topic: TopicRef,
}

/// Requests both supported publication modes of each exact topic/schema.
///
/// The nonempty topic list is validated as a whole before creating demand.
/// Duplicate selectors are deduplicated.
/// All-session subscriptions follow only providers named in the topic list.
/// A session-specific list must belong to that session's provider.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct Subscribe {
    pub sessions: SessionSelector,
    pub topics: Vec<TopicRef>,
}

/// Game schemas evolve inside generic envelopes, never by adding RPC variants.
///
/// Breaking a published wire format requires another ALPN. Unpublished formats
/// may change in place. Provider version, game build, and topic schema versions
/// remain independent.
#[rpc_requests(message = ObserverMessageV1)]
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub enum ObserverProtocolV1 {
    #[rpc(tx = oneshot::Sender<Pong>)]
    Ping(Ping),
    #[rpc(tx = oneshot::Sender<Result<Catalog, RequestError>>)]
    GetCatalog(GetCatalog),
    #[rpc(tx = oneshot::Sender<Result<ServiceStatus, RequestError>>)]
    GetStatus(GetStatus),
    #[rpc(tx = oneshot::Sender<Result<DataEnvelope, RequestError>>)]
    GetSnapshot(GetSnapshot),
    /// Errors reject setup before Begin. After Begin, use a terminal Closed item.
    #[rpc(tx = mpsc::Sender<Result<SubscriptionItem, RequestError>>)]
    Subscribe(Subscribe),
}
