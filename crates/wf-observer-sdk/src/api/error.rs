use crate::api::RequestError;

/// Structured SDK errors; end-of-stream remains a successful None.
#[boltffi::error]
#[derive(Debug, Clone, displaydoc::Display, ..Eq)]
pub enum ObserverError {
    /// invalid observer endpoint: {message}
    InvalidEndpoint { message: String },
    /// local observer discovery failed: {message}
    LocalDiscovery { message: String },
    /// no matching game session is running
    NoSession,
    /// multiple game sessions are running; select one explicitly
    AmbiguousSession,
    /// observer operation timed out
    Timeout,
    /// observation ended: {reason:?}
    Ended { reason: crate::api::SubscriptionEnd },
    /// observer client is closed
    Closed,
    /// another next call is already pending on this subscription
    ConcurrentNext,
    /// event listener fell behind; resubscribe for current state
    Lagged,
    /// observer request rejected: {error:?}
    Request { error: RequestError },
    /// observer runtime failed: {message}
    Runtime { message: String },
    /// observer transport failed: {message}
    Transport { message: String },
    /// incomplete or invalid subscription; resubscribe: {message}
    Protocol { message: String },
    /// topic data could not be decoded: {message}
    PayloadDecode { message: String },
}

impl std::error::Error for ObserverError {}

impl From<crate::raw::ClientError> for ObserverError {
    fn from(error: crate::raw::ClientError) -> Self {
        match error {
            crate::raw::ClientError::InvalidEndpoint(message) => Self::InvalidEndpoint { message },
            crate::raw::ClientError::LocalDiscovery(message) => Self::LocalDiscovery { message },
            crate::raw::ClientError::Timeout => Self::Timeout,
            crate::raw::ClientError::Ended(reason) => Self::Ended { reason },
            crate::raw::ClientError::Closed => Self::Closed,
            crate::raw::ClientError::ConcurrentNext => Self::ConcurrentNext,
            crate::raw::ClientError::Lagged => Self::Lagged,
            crate::raw::ClientError::Request(error) => Self::Request { error },
            crate::raw::ClientError::Transport(message) => Self::Transport { message },
            crate::raw::ClientError::Protocol(message) => Self::Protocol { message },
            crate::raw::ClientError::WrongTopic => Self::PayloadDecode {
                message: "wrong topic or schema".into(),
            },
            crate::raw::ClientError::Decode(error) => Self::PayloadDecode {
                message: error.to_string(),
            },
        }
    }
}
