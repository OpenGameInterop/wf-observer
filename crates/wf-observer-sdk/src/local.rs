//! Discovery of the current user's already-running local service.

use iroh::{
    Endpoint, EndpointAddr,
    endpoint::{BindOpts, Builder, NetReportConfig, PortmapperConfig, presets},
};
use std::net::{Ipv4Addr, Ipv6Addr};

use crate::{
    client::is_loopback_address,
    raw::{Client, ClientError, ClientIdentity},
};

#[derive(serde::Deserialize)]
struct RuntimeRecord {
    schema_version: u32,
    service: ServiceIdentity,
}

#[derive(serde::Deserialize)]
struct ServiceIdentity {
    endpoint_id: String,
    local_ticket: Option<String>,
}

impl Client {
    /// Connects to the current user's local service; does not launch it.
    ///
    /// # Errors
    /// Returns discovery errors if the runtime record is absent or invalid,
    /// or connection errors if the recorded service cannot be reached.
    pub async fn connect_local() -> Result<Self, ClientError> {
        Self::connect_local_with_identity(&ClientIdentity::generate()).await
    }

    /// Discovers the local service and connects with a reusable reader identity.
    /// Remote mode requires approval even over the local ticket.
    ///
    /// # Errors
    /// Returns discovery, transport, or `NotAuthorized` errors.
    pub async fn connect_local_with_identity(
        identity: &ClientIdentity,
    ) -> Result<Self, ClientError> {
        let project = directories::ProjectDirs::from("", "", "wf-observer").ok_or_else(|| {
            ClientError::LocalDiscovery("application directories are unavailable".into())
        })?;
        let directory = project.runtime_dir().unwrap_or_else(|| project.cache_dir());
        let path = directory.join("runtime.json");
        let bytes = std::fs::read(&path).map_err(|error| {
            ClientError::LocalDiscovery(format!(
                "{}: {error}; start wf-observer first",
                path.display()
            ))
        })?;
        Self::connect_with_identity(local_address(&bytes)?, identity).await
    }
}

fn local_address(bytes: &[u8]) -> Result<EndpointAddr, ClientError> {
    let invalid = |message: &str| ClientError::LocalDiscovery(message.into());
    let record: RuntimeRecord = serde_json::from_slice(bytes)
        .map_err(|error| ClientError::LocalDiscovery(error.to_string()))?;
    if record.schema_version != 1 {
        return Err(invalid("unsupported runtime record version"));
    }
    let ticket = record.service.local_ticket.ok_or_else(|| {
        invalid(
            "service has no local connection ticket; run wf-observer start with the current CLI",
        )
    })?;
    let ticket: iroh_tickets::endpoint::EndpointTicket = ticket
        .parse()
        .map_err(|error| ClientError::LocalDiscovery(format!("invalid local ticket: {error}")))?;
    let address = ticket.endpoint_addr();
    let id: iroh::EndpointId =
        record.service.endpoint_id.parse().map_err(|error| {
            ClientError::LocalDiscovery(format!("invalid endpoint ID: {error}"))
        })?;
    if address.id != id || !is_loopback_address(address) {
        return Err(invalid(
            "local ticket must match the service identity and contain only loopback addresses",
        ));
    }
    Ok(address.clone())
}

pub(crate) fn endpoint_builder() -> Result<Builder, ClientError> {
    Endpoint::builder(presets::Minimal)
        .clear_ip_transports()
        .bind_addr((Ipv4Addr::LOCALHOST, 0))
        .map_err(ClientError::transport)?
        .bind_addr_with_opts(
            (Ipv6Addr::LOCALHOST, 0),
            BindOpts::default().set_is_required(false),
        )
        .map(|builder| {
            builder
                .portmapper_config(PortmapperConfig::Disabled)
                .net_report_config(NetReportConfig::minimal())
        })
        .map_err(ClientError::transport)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: iroh::EndpointId, address: EndpointAddr) -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "service": {
                "endpoint_id": id.to_string(),
                "local_ticket": iroh_tickets::endpoint::EndpointTicket::new(address).to_string(),
            },
        })
    }

    #[test]
    fn local_discovery_requires_matching_identity_and_only_loopback_routes() -> anyhow::Result<()> {
        let id = iroh::SecretKey::generate().public();
        let local = EndpointAddr::new(id).with_ip_addr("127.0.0.1:12345".parse()?);
        assert_eq!(
            local_address(&serde_json::to_vec(&record(id, local.clone()))?)?,
            local
        );
        let mut wrong_identity = local.clone();
        wrong_identity.id = iroh::SecretKey::generate().public();
        for address in [
            EndpointAddr::new(id),
            wrong_identity,
            local.clone().with_ip_addr("192.0.2.1:12345".parse()?),
            local.with_relay_url("https://relay.invalid".parse()?),
        ] {
            assert!(local_address(&serde_json::to_vec(&record(id, address))?).is_err());
        }
        let legacy =
            serde_json::json!({"schema_version": 1, "service": {"endpoint_id": id.to_string()}});
        assert!(matches!(
            local_address(&serde_json::to_vec(&legacy)?),
            Err(ClientError::LocalDiscovery(_))
        ));
        Ok(())
    }

    #[tokio::test]
    async fn local_client_binds_only_loopback_without_public_discovery() -> anyhow::Result<()> {
        let endpoint = endpoint_builder()?.bind().await?;
        assert!(!endpoint.bound_sockets().is_empty());
        assert!(
            endpoint
                .bound_sockets()
                .iter()
                .all(|socket| socket.ip().is_loopback())
        );
        assert!(endpoint.address_lookup()?.is_empty());
        assert!(endpoint.addr().relay_urls().next().is_none());
        endpoint.close().await;
        Ok(())
    }
}
