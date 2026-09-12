use iroh::{Endpoint, endpoint::presets};
use protocol::v1 as wire;
use std::{future::Future, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

use crate::raw::{
    ClientError, EndpointAddr, SnapshotTopic, Subscription, TypedData, decode_snapshot,
};

/// Connection to one observer service.
///
/// New requests reconnect after transport loss. Subscriptions are never replayed
/// or silently reconnected: invalidate their data and resubscribe to get a new
/// coherent bootstrap. Keep the client alive while using its subscriptions.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Connection>,
    timeout: Duration,
}

struct Connection {
    endpoint: Endpoint,
    rpc: irpc::Client<wire::ObserverProtocolV1>,
    closed: CancellationToken,
    subscriptions: crate::subscription::Manager,
}

impl Client {
    /// Connects and verifies protocol reachability.
    ///
    /// # Errors
    ///
    /// Returns an error if the endpoint cannot bind or the service cannot be reached.
    pub async fn connect(address: EndpointAddr) -> Result<Self, ClientError> {
        let endpoint = Endpoint::bind(presets::N0)
            .await
            .map_err(ClientError::transport)?;
        let rpc =
            irpc_iroh::client::<wire::ObserverProtocolV1>(endpoint.clone(), address, wire::ALPN_V1);
        let client = Self {
            inner: Arc::new(Connection {
                endpoint,
                rpc,
                closed: CancellationToken::new(),
                subscriptions: crate::subscription::Manager::default(),
            }),
            timeout: Duration::from_secs(30),
        };
        if let Err(error) = client.ping().await {
            client.close().await;
            return Err(error);
        }
        Ok(client)
    }

    /// Connects using an endpoint ID or ticket. Whitespace around pasted text is ignored.
    ///
    /// # Errors
    /// Returns an invalid-address, transport, or timeout error.
    pub async fn connect_endpoint(endpoint: &str) -> Result<Self, ClientError> {
        let text = endpoint.trim();
        let address = text
            .parse::<iroh_tickets::endpoint::EndpointTicket>()
            .map(|ticket| ticket.endpoint_addr().clone())
            .or_else(|_| text.parse::<crate::raw::EndpointId>().map(Into::into))
            .map_err(|error| ClientError::InvalidEndpoint(error.to_string()))?;
        Self::connect(address).await
    }

    /// Sets the deadline for requests, subscription setup, and one-shot reads.
    /// Existing clones keep their previous deadline. Watching has no idle timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub(crate) fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Verifies reachability without triggering memory acquisition.
    ///
    /// # Errors
    ///
    /// Returns an error if the client is closed or the exchange fails.
    pub async fn ping(&self) -> Result<(), ClientError> {
        self.request(self.inner.rpc.rpc(wire::Ping)).await?;
        Ok(())
    }

    /// Returns compiled providers and topics.
    ///
    /// # Errors
    ///
    /// Returns a service rejection, closed-client error, or transport error.
    pub async fn catalog(&self) -> Result<wire::Catalog, ClientError> {
        self.request(self.inner.rpc.rpc(wire::GetCatalog))
            .await?
            .map_err(ClientError::Request)
    }

    /// Returns the current discovery/session/capability state.
    ///
    /// # Errors
    ///
    /// Returns a service rejection, closed-client error, or transport error.
    pub async fn status(&self) -> Result<wire::ServiceStatus, ClientError> {
        self.request(self.inner.rpc.rpc(wire::GetStatus))
            .await?
            .map_err(ClientError::Request)
    }

    /// Queries an explicit session/topic cache. Subscribe to initiate acquisition.
    ///
    /// # Errors
    ///
    /// Returns a service rejection (including Idle/NotSampled), transport error,
    /// or an invalid response identity.
    pub async fn snapshot(
        &self,
        session: &wire::SessionRef,
        topic: &wire::TopicRef,
    ) -> Result<wire::DataEnvelope, ClientError> {
        let envelope = self
            .request(self.inner.rpc.rpc(wire::GetSnapshot {
                session: session.clone(),
                topic: topic.clone(),
            }))
            .await?
            .map_err(ClientError::Request)?;
        if envelope.metadata.source.session != *session || envelope.metadata.source.topic != *topic
        {
            return Err(ClientError::protocol(
                "snapshot response has a different identity",
            ));
        }
        Ok(envelope)
    }

    /// Queries and decodes a snapshot using a portable topic definition.
    ///
    /// # Errors
    ///
    /// Returns snapshot request errors or topic/schema decoding errors.
    pub async fn snapshot_typed<T: SnapshotTopic>(
        &self,
        session: &wire::SessionRef,
    ) -> Result<TypedData<T::Snapshot>, ClientError> {
        decode_snapshot::<T>(self.snapshot(session, &T::topic()).await?)
    }

    /// Attaches a local listener after its shared feeds have captured initial state.
    ///
    /// Identical topic/session selections share an upstream subscription. Closing
    /// the last listener releases demand. Initial state may be unavailable; setup
    /// does not wait for a successful game sample. Different topics have independent
    /// baselines and event ordering. All-session and specific-session selections are
    /// shared independently, without acquiring unrequested sessions.
    ///
    /// # Errors
    ///
    /// Returns setup rejection, transport loss, or an invalid initial frame.
    pub async fn subscribe(&self, request: wire::Subscribe) -> Result<Subscription, ClientError> {
        self.subscribe_listener(request, true).await
    }

    pub(crate) async fn subscribe_listener(
        &self,
        request: wire::Subscribe,
        receive_events: bool,
    ) -> Result<Subscription, ClientError> {
        n0_future::time::timeout(
            self.timeout,
            self.inner.subscriptions.subscribe(
                request,
                receive_events,
                self.inner.rpc.clone(),
                self.inner.closed.clone(),
            ),
        )
        .await
        .map_err(|_| ClientError::Timeout)?
    }

    /// Wakes pending operations and gracefully closes the endpoint. Idempotent.
    pub async fn close(&self) {
        self.inner.closed.cancel();
        self.inner.subscriptions.close();
        self.inner.endpoint.close().await;
    }

    async fn request<T>(
        &self,
        future: impl Future<Output = irpc::Result<T>>,
    ) -> Result<T, ClientError> {
        let result = n0_future::time::timeout(self.timeout, async {
            tokio::select! {
                biased;
                () = self.inner.closed.cancelled() => Err(ClientError::Closed),
                result = future => result.map_err(ClientError::transport),
            }
        })
        .await
        .map_err(|_| ClientError::Timeout)?;
        if self.inner.closed.is_cancelled() {
            Err(ClientError::Closed)
        } else {
            result
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.closed.cancel();
        self.subscriptions.close();
    }
}
