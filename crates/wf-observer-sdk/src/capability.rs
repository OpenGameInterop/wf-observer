use std::{marker::PhantomData, sync::Arc, time::Duration};

use crate::raw::{
    Client, ClientError, EventTopic, EventWatch, SnapshotTopic, SnapshotWatch, State, TypedData,
    types,
};

/// Inactive, session-bound snapshot capability. Cloning does not acquire data.
pub struct Capability<T: SnapshotTopic> {
    client: Client,
    session: types::SessionRef,
    topic: PhantomData<T>,
}

impl<T: SnapshotTopic> Clone for Capability<T> {
    fn clone(&self) -> Self {
        Self::new(self.client.clone(), self.session.clone())
    }
}

impl<T: SnapshotTopic> Capability<T> {
    /// Binds a portable topic definition to an explicit session without creating demand.
    #[must_use]
    pub fn new(client: Client, session: types::SessionRef) -> Self {
        Self {
            client,
            session,
            topic: PhantomData,
        }
    }

    /// Opens an independent typed watch, including its initial state.
    /// Closing/dropping it releases only its own demand.
    ///
    /// # Errors
    /// Returns subscription setup errors.
    pub async fn watch(&self) -> Result<SnapshotWatch<T>, ClientError> {
        let sub = self
            .client
            .subscribe_listener(
                types::Subscribe {
                    sessions: types::SessionSelector::Session {
                        reference: self.session.clone(),
                    },
                    topics: vec![T::topic()],
                },
                false,
            )
            .await?;
        Ok(SnapshotWatch::new(
            self.client.clone(),
            self.session.clone(),
            sub,
        ))
    }

    /// Obtains a current validated value using temporary shared demand.
    /// Waits through initialization, fails on unavailability, and releases demand
    /// on success, error, or cancellation. May use the existing current generation's
    /// cache; this does not promise a new memory sample after the call.
    /// The client's deadline covers both subscription setup and the first value.
    ///
    /// # Errors
    /// Returns setup, timeout, unavailable, decode, or termination errors.
    pub async fn read(&self) -> Result<Arc<TypedData<T::Snapshot>>, ClientError> {
        self.read_with_timeout(self.client.timeout()).await
    }

    /// Reads once with an explicit total deadline, including setup.
    ///
    /// # Errors
    /// Returns the same errors as [`Self::read`].
    pub async fn read_with_timeout(
        &self,
        timeout: Duration,
    ) -> Result<Arc<TypedData<T::Snapshot>>, ClientError> {
        n0_future::time::timeout(timeout, async {
            let watch = self.watch().await?;
            loop {
                match watch.next().await? {
                    Some(State::Ready(value)) => return Ok(value),
                    Some(State::Waiting) => {}
                    Some(State::Unavailable(reason)) => {
                        return Err(ClientError::Request(types::RequestError::Unavailable {
                            reason,
                        }));
                    }
                    None => return Err(ClientError::Closed),
                }
            }
        })
        .await
        .map_err(|_| ClientError::Timeout)?
    }

    /// Queries the service cache without creating demand. Idle/unsampled is None.
    /// Unavailability remains an error, never an empty domain value.
    ///
    /// # Errors
    /// Returns request, transport, or decoding errors.
    pub async fn cached(&self) -> Result<Option<TypedData<T::Snapshot>>, ClientError> {
        match self.client.snapshot_typed::<T>(&self.session).await {
            Ok(value) => Ok(Some(value)),
            Err(ClientError::Request(
                types::RequestError::Idle | types::RequestError::NotSampled,
            )) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// Inactive, session-bound event capability. Event topics have no read/cache API.
pub struct EventCapability<T: EventTopic> {
    client: Client,
    session: types::SessionRef,
    topic: PhantomData<T>,
}

impl<T: EventTopic> Clone for EventCapability<T> {
    fn clone(&self) -> Self {
        Self::new(self.client.clone(), self.session.clone())
    }
}

impl<T: EventTopic> EventCapability<T> {
    /// Binds an event topic to one session without creating demand.
    #[must_use]
    pub fn new(client: Client, session: types::SessionRef) -> Self {
        Self {
            client,
            session,
            topic: PhantomData,
        }
    }

    /// Watches typed events and source state. No historical replay is promised.
    ///
    /// # Errors
    /// Returns subscription setup errors.
    pub async fn watch(&self) -> Result<EventWatch<T>, ClientError> {
        let sub = self
            .client
            .subscribe(types::Subscribe {
                sessions: types::SessionSelector::Session {
                    reference: self.session.clone(),
                },
                topics: vec![T::topic()],
            })
            .await?;
        Ok(EventWatch::new(
            self.client.clone(),
            self.session.clone(),
            sub,
        ))
    }
}
