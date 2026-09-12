use crate::api::{
    Catalog, DataEnvelope, ObserverError, ObserverSubscription, ServiceStatus, SessionRef,
    SessionSelector, TopicRef, runtime,
};

/// An observer connection shared by Rust applications and generated bindings.
#[derive(Clone)]
pub struct ObserverClient {
    inner: crate::client::Client,
}

/// Connects using an Iroh endpoint ticket or stable endpoint identifier.
///
/// A free function is portable across every enabled generator backend.
///
/// # Errors
///
/// Returns an error for invalid endpoint text, runtime setup, or connection failure.
#[boltffi::export]
pub async fn connect(endpoint: String) -> Result<ObserverClient, ObserverError> {
    let client = runtime::execute(Box::pin(async move {
        crate::client::Client::connect_endpoint(&endpoint).await
    }))
    .await??;
    Ok(ObserverClient { inner: client })
}

/// Connects to the current user's already-running local service.
/// Browser clients must use an explicit endpoint instead.
///
/// # Errors
/// Returns discovery, runtime, or connection errors.
#[boltffi::export]
pub async fn connect_local() -> Result<ObserverClient, ObserverError> {
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        let client = runtime::execute(Box::pin(crate::client::Client::connect_local())).await??;
        Ok(ObserverClient { inner: client })
    }
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    Err(ObserverError::LocalDiscovery {
        message: "browser clients require an explicit endpoint".into(),
    })
}

#[boltffi::export]
impl ObserverClient {
    /// Returns an inactive Warframe scope. The scope retains its connection.
    #[must_use]
    pub fn warframe(&self) -> crate::api::Warframe {
        crate::api::Warframe::new(self.inner.clone())
    }

    /// Verifies protocol reachability.
    ///
    /// # Errors
    ///
    /// Returns an error if runtime setup or the ping fails.
    pub async fn ping(&self) -> Result<(), ObserverError> {
        let client = self.inner.clone();
        runtime::execute(async move { client.ping().await })
            .await?
            .map_err(Into::into)
    }

    /// Returns compiled providers and capabilities.
    ///
    /// # Errors
    ///
    /// Returns runtime, transport, or service rejection errors.
    pub async fn catalog(&self) -> Result<Catalog, ObserverError> {
        let client = self.inner.clone();
        runtime::execute(async move { client.catalog().await })
            .await?
            .map_err(Into::into)
    }

    /// Returns discovery and current session/capability status.
    ///
    /// # Errors
    ///
    /// Returns runtime, transport, or service rejection errors.
    pub async fn status(&self) -> Result<ServiceStatus, ObserverError> {
        let client = self.inner.clone();
        runtime::execute(async move { client.status().await })
            .await?
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Queries an explicit session/topic cache; does not create demand.
    ///
    /// # Errors
    ///
    /// Returns runtime, transport, or structured snapshot rejection errors.
    pub async fn snapshot(
        &self,
        session: SessionRef,
        topic: TopicRef,
    ) -> Result<DataEnvelope, ObserverError> {
        let client = self.inner.clone();
        runtime::execute(async move { client.snapshot(&session, &topic).await })
            .await?
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Creates an independent listener, sharing the Rust client's upstream feeds.
    ///
    /// # Errors
    ///
    /// Returns runtime, transport, or structured setup rejection errors.
    pub async fn subscribe(
        &self,
        sessions: SessionSelector,
        topics: Vec<TopicRef>,
    ) -> Result<ObserverSubscription, ObserverError> {
        let client = self.inner.clone();
        let stream = runtime::execute(async move {
            client
                .subscribe(crate::raw::types::Subscribe { sessions, topics })
                .await
        })
        .await?
        .map_err(ObserverError::from)?;
        Ok(ObserverSubscription::new(stream, self.inner.clone()))
    }

    /// Wakes pending operations and closes the endpoint.
    ///
    /// Call before disposing the foreign object: disposal cannot await shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error if the async runtime cannot start.
    pub async fn shutdown(&self) -> Result<(), ObserverError> {
        let client = self.inner.clone();
        runtime::execute(async move { client.close().await }).await?;
        Ok(())
    }
}
