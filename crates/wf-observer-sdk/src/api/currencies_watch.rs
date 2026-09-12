//! Typed currencies reads and watches. State/lifetimes belong to the Rust SDK.
use crate::api::{ObserverError, UnavailableReason, WarframeCurrencies, runtime};
use crate::warframe::CurrenciesTopic;
use std::{sync::Arc, time::Duration};

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub enum CurrenciesState {
    Waiting,
    Ready { value: WarframeCurrencies },
    Unavailable { reason: UnavailableReason },
}
impl From<crate::raw::State<warframe_model::CurrencySnapshot>> for CurrenciesState {
    fn from(state: crate::raw::State<warframe_model::CurrencySnapshot>) -> Self {
        match state {
            crate::raw::State::Waiting => Self::Waiting,
            crate::raw::State::Ready(value) => Self::Ready {
                value: (*value).clone().into(),
            },
            crate::raw::State::Unavailable(reason) => Self::Unavailable { reason },
        }
    }
}

pub struct CurrenciesCapability {
    inner: crate::raw::Capability<CurrenciesTopic>,
}
impl CurrenciesCapability {
    pub(crate) fn new(inner: crate::raw::Capability<CurrenciesTopic>) -> Self {
        Self { inner }
    }
}
#[boltffi::export]
impl CurrenciesCapability {
    /// Obtains a validated value with temporary demand and a 30-second deadline.
    /// Cancellation, success, and failure release the temporary listener.
    /// # Errors
    /// Reports request, timeout, unavailable, decode, or termination errors.
    pub async fn read(&self) -> Result<WarframeCurrencies, ObserverError> {
        self.read_with_timeout(30_000).await
    }
    /// Reads once with a total deadline, including setup, in milliseconds.
    /// # Errors
    /// Reports the same failures as read.
    pub async fn read_with_timeout(
        &self,
        timeout_ms: u32,
    ) -> Result<WarframeCurrencies, ObserverError> {
        let capability = self.inner.clone();
        runtime::execute(async move {
            capability
                .read_with_timeout(Duration::from_millis(u64::from(timeout_ms)))
                .await
        })
        .await?
        .map(|value| (*value).clone().into())
        .map_err(Into::into)
    }
    /// Reads the service cache without acquiring data. Idle/unsampled is None.
    /// # Errors
    /// Reports request, unavailability, or decode errors.
    pub async fn cached(&self) -> Result<Option<WarframeCurrencies>, ObserverError> {
        let capability = self.inner.clone();
        runtime::execute(async move { capability.cached().await })
            .await?
            .map(|value| value.map(Into::into))
            .map_err(Into::into)
    }
    /// Opens an independent watch with initial state already queued.
    /// # Errors
    /// Reports subscription setup errors.
    pub async fn watch(&self) -> Result<CurrenciesWatch, ObserverError> {
        let capability = self.inner.clone();
        runtime::execute(async move { capability.watch().await })
            .await?
            .map(|inner| CurrenciesWatch {
                inner: Arc::new(inner),
            })
            .map_err(Into::into)
    }
}

pub struct CurrenciesWatch {
    inner: Arc<crate::raw::SnapshotWatch<CurrenciesTopic>>,
}
#[boltffi::export]
impl CurrenciesWatch {
    /// Current typed state. Does not consume pending updates.
    /// # Errors
    /// Reports closure or decoding failure.
    pub fn current(&self) -> Result<CurrenciesState, ObserverError> {
        self.inner.current().map(Into::into).map_err(Into::into)
    }
    /// Initial state or a replacement. Only one receive may be pending.
    /// Cancelling a pending receive leaves the watch active.
    /// # Errors
    /// Reports concurrent receive, upstream, and decoding failures.
    pub async fn next(&self) -> Result<Option<CurrenciesState>, ObserverError> {
        let watch = self.inner.clone();
        runtime::execute(async move { watch.next().await })
            .await?
            .map(|state| state.map(Into::into))
            .map_err(Into::into)
    }
    /// Releases demand immediately and wakes pending receives. Idempotent.
    pub fn cancel(&self) {
        self.inner.close();
    }
    /// Releases demand and waits for pending receives to finish.
    /// # Errors
    /// Reports runtime failure.
    pub async fn shutdown(&self) -> Result<(), ObserverError> {
        let watch = self.inner.clone();
        runtime::execute(async move { watch.shutdown().await }).await
    }
}
impl Drop for CurrenciesWatch {
    fn drop(&mut self) {
        self.inner.close();
    }
}
