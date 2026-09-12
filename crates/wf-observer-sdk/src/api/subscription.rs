use crate::api::{ObserverError, SubscriptionItem, SubscriptionState, runtime};
use std::sync::Arc;

/// A local listener on feeds shared by its Rust SDK client.
pub struct ObserverSubscription {
    inner: Arc<crate::raw::Subscription>,
    _client: crate::raw::Client,
}

impl ObserverSubscription {
    pub(crate) fn new(listener: crate::raw::Subscription, client: crate::raw::Client) -> Self {
        Self {
            inner: Arc::new(listener),
            _client: client,
        }
    }
}

#[boltffi::export]
impl ObserverSubscription {
    /// Returns current state without consuming pending observations.
    pub fn current(&self) -> Option<SubscriptionState> {
        self.inner.current().map(Into::into)
    }

    /// Receives state, an event, or completion. Only one next call may be pending.
    /// Cancelling the foreign future leaves this listener active.
    ///
    /// # Errors
    /// Reports the SDK's concurrency, upstream, and local event-lag errors.
    pub async fn next(&self) -> Result<Option<SubscriptionItem>, ObserverError> {
        let listener = self.inner.clone();
        runtime::execute(async move { listener.next().await })
            .await?
            .map(|item| item.map(Into::into))
            .map_err(Into::into)
    }

    /// Releases this listener and waits for its pending receive to finish.
    /// Subsequent next calls return None.
    ///
    /// # Errors
    /// Reports failure to execute on the Rust runtime.
    pub async fn shutdown(&self) -> Result<(), ObserverError> {
        let listener = self.inner.clone();
        runtime::execute(async move { listener.shutdown().await }).await
    }
}

impl Drop for ObserverSubscription {
    fn drop(&mut self) {
        self.inner.close();
    }
}
