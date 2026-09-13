use derive_more::Error;
use displaydoc::Display;

/// Distinguishes service rejection, transport loss, and invalid topic data.
#[derive(Debug, Clone, Display, Error)]
pub enum ClientError {
    /// invalid observer endpoint: {_0}
    InvalidEndpoint(#[error(not(source))] String),
    /// local observer discovery failed: {_0}
    LocalDiscovery(#[error(not(source))] String),
    /// reader identity failed: {_0}
    Identity(#[error(not(source))] String),
    /// reader is not authorized; approve its endpoint ID with wf-observer peers allow
    NotAuthorized,
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
    pub(crate) fn transport(error: impl std::error::Error + 'static) -> Self {
        if not_authorized(&error) {
            return Self::NotAuthorized;
        }
        Self::Transport(format!("{error:#}"))
    }

    pub(crate) fn protocol(message: &str) -> Self {
        Self::Protocol(message.into())
    }
}

fn not_authorized(error: &(dyn std::error::Error + 'static)) -> bool {
    if let Some(iroh::endpoint::ConnectionError::ApplicationClosed(close)) =
        error.downcast_ref::<iroh::endpoint::ConnectionError>()
    {
        return close.error_code.into_inner() == u64::from(protocol::v1::NOT_AUTHORIZED_CLOSE_CODE);
    }
    // io::Error::source skips its directly wrapped error, so inspect it too.
    if let Some(inner) = error
        .downcast_ref::<std::io::Error>()
        .and_then(std::io::Error::get_ref)
        && not_authorized(inner)
    {
        return true;
    }
    error.source().is_some_and(not_authorized)
}

impl From<serde_json::Error> for ClientError {
    fn from(error: serde_json::Error) -> Self {
        Self::Decode(std::sync::Arc::new(error))
    }
}
