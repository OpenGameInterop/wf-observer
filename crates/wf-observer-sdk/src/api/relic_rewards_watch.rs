//! Typed relic reward reads and watches. State/lifetimes belong to the Rust SDK.
use crate::api::{ObserverError, UnavailableReason, WarframeRelicRewards, runtime};
use crate::warframe::RelicRewardsTopic;
use std::{sync::Arc, time::Duration};

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub enum RelicRewardsState {
    Waiting,
    Ready { value: WarframeRelicRewards },
    Unavailable { reason: UnavailableReason },
}
impl From<crate::raw::State<warframe_model::RelicRewardsSnapshot>> for RelicRewardsState {
    fn from(state: crate::raw::State<warframe_model::RelicRewardsSnapshot>) -> Self {
        match state {
            crate::raw::State::Waiting => Self::Waiting,
            crate::raw::State::Ready(value) => Self::Ready {
                value: (*value).clone().into(),
            },
            crate::raw::State::Unavailable(reason) => Self::Unavailable { reason },
        }
    }
}

pub struct RelicRewardsCapability {
    inner: crate::raw::Capability<RelicRewardsTopic>,
}
impl RelicRewardsCapability {
    pub(crate) fn new(inner: crate::raw::Capability<RelicRewardsTopic>) -> Self {
        Self { inner }
    }
}
#[boltffi::export]
impl RelicRewardsCapability {
    /// Obtains a validated value with temporary demand and a 30-second deadline.
    /// Cancellation, success, and failure release the temporary listener.
    /// # Errors
    /// Reports request, timeout, unavailable, decode, or termination errors.
    pub async fn read(&self) -> Result<WarframeRelicRewards, ObserverError> {
        self.read_with_timeout(30_000).await
    }
    /// Reads once with a total deadline, including setup, in milliseconds.
    /// # Errors
    /// Reports the same failures as read.
    pub async fn read_with_timeout(
        &self,
        timeout_ms: u32,
    ) -> Result<WarframeRelicRewards, ObserverError> {
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
    pub async fn cached(&self) -> Result<Option<WarframeRelicRewards>, ObserverError> {
        let capability = self.inner.clone();
        runtime::execute(async move { capability.cached().await })
            .await?
            .map(|value| value.map(Into::into))
            .map_err(Into::into)
    }
    /// Opens an independent watch with initial state already queued.
    /// # Errors
    /// Reports subscription setup errors.
    pub async fn watch(&self) -> Result<RelicRewardsWatch, ObserverError> {
        let capability = self.inner.clone();
        runtime::execute(async move { capability.watch().await })
            .await?
            .map(|inner| RelicRewardsWatch {
                inner: Arc::new(inner),
            })
            .map_err(Into::into)
    }
}

pub struct RelicRewardsWatch {
    inner: Arc<crate::raw::SnapshotWatch<RelicRewardsTopic>>,
}
#[boltffi::export]
impl RelicRewardsWatch {
    /// Current typed state. Does not consume pending updates.
    /// # Errors
    /// Reports closure or decoding failure.
    pub fn current(&self) -> Result<RelicRewardsState, ObserverError> {
        self.inner.current().map(Into::into).map_err(Into::into)
    }
    /// Initial state or a replacement. Only one receive may be pending.
    /// Cancelling a pending receive leaves the watch active.
    /// # Errors
    /// Reports concurrent receive, upstream, and decoding failures.
    pub async fn next(&self) -> Result<Option<RelicRewardsState>, ObserverError> {
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
impl Drop for RelicRewardsWatch {
    fn drop(&mut self) {
        self.inner.close();
    }
}
