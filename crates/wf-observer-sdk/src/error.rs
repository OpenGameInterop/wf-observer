use derive_more::Error;
use displaydoc::Display;

/// Distinguishes service rejection, transport loss, and invalid topic data.
#[derive(Debug, Clone, Display, Error)]
pub enum ClientError {
    /// invalid observer endpoint: {_0}
    InvalidEndpoint(#[error(not(source))] String),
    /// local observer discovery failed: {_0}
    LocalDiscovery(#[error(not(source))] String),
    /// observer operation timed out
    Timeout,
    /// observation ended: {_0:?}
    Ended(#[error(not(source))] protocol::v1::SubscriptionEnd),
    /// observer client is closed
    Closed,
    /// another next call is already pending on this listener
    ConcurrentNext,
    /// event listener fell behind; resubscribe for current state
    Lagged,
    /// observer request rejected: {0}
    Request(protocol::v1::RequestError),
    /// observer transport failed: {_0}
    Transport(#[error(not(source))] String),
    /// incomplete or invalid subscription; resubscribe: {_0}
    Protocol(#[error(not(source))] String),
    /// envelope does not match the requested topic and schema
    WrongTopic,
    /// topic JSON could not be decoded: {0}
    Decode(std::sync::Arc<serde_json::Error>),
}

impl ClientError {
    pub(crate) fn transport(error: impl std::fmt::Display) -> Self {
        Self::Transport(format!("{error:#}"))
    }

    pub(crate) fn protocol(message: &str) -> Self {
        Self::Protocol(message.into())
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(error: serde_json::Error) -> Self {
        Self::Decode(std::sync::Arc::new(error))
    }
}
