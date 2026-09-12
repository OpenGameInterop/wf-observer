//! WF Observer client library.

use anyhow::Context as _;
use iroh::{Endpoint, endpoint::presets};

#[allow(
    unused_imports,
    reason = "derive aliases are configured before client types are added"
)]
#[macro_use(derive)]
extern crate derive_aliases;

mod derive_alias;

pub use iroh::{EndpointAddr, EndpointId};

/// Client connection to a running WF Observer service.
///
/// New requests reconnect automatically after a lost transport connection.
/// Reconnecting across service restarts requires the service to retain its Iroh
/// endpoint identity.
pub struct Client {
    endpoint: Endpoint,
    rpc: irpc::Client<protocol::ObserverProtocol>,
}

impl Client {
    /// Connects to a running WF Observer service.
    ///
    /// # Errors
    ///
    /// Returns an error if the local Iroh endpoint cannot be bound or the
    /// service cannot be reached using the current protocol version.
    pub async fn connect(address: EndpointAddr) -> anyhow::Result<Self> {
        let endpoint = Endpoint::bind(presets::N0).await?;
        let rpc = irpc_iroh::client::<protocol::ObserverProtocol>(
            endpoint.clone(),
            address,
            protocol::ALPN_V0,
        );
        let client = Self { endpoint, rpc };

        client
            .ping()
            .await
            .context("failed to connect to the WF Observer service")?;

        Ok(client)
    }

    /// Verifies that the service is reachable and speaking the expected
    /// protocol version.
    ///
    /// # Errors
    ///
    /// Returns an error if the request cannot be sent or the response cannot be
    /// read.
    pub async fn ping(&self) -> anyhow::Result<()> {
        let _ = self
            .rpc
            .rpc(protocol::Ping)
            .await
            .context("ping request failed")?;

        Ok(())
    }

    /// Gracefully closes the client's Iroh endpoint.
    pub async fn close(&self) {
        self.endpoint.close().await;
    }
}
