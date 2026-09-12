//! Generated-language bindings for the WF Observer client.
//!
//! This crate owns only the FFI boundary. Transport behavior, reconnects, and
//! protocol types remain in `wf-observer-sdk` and `protocol`.
//!
//! If `BoltFFI` can expose a canonical Rust type without changing its natural
//! design, export that type directly. If the binding generator would dictate
//! the canonical type's design, keep the adaptation in this crate instead.

mod runtime;

use std::sync::Arc;

use iroh_tickets::endpoint::EndpointTicket;
use wf_observer_sdk::EndpointId;

/// Client exposed to generated language bindings.
pub struct ObserverClient {
    inner: Arc<wf_observer_sdk::Client>,
}

/// Connects using an Iroh endpoint ticket or stable endpoint identifier.
///
/// This is a free function because every supported `BoltFFI` backend can
/// represent an asynchronous function returning a class, while asynchronous
/// class initializers are not portable across those backends.
///
/// # Errors
///
/// Returns an error for invalid endpoint text, async-runtime setup failure,
/// or an unsuccessful connection.
#[boltffi::export]
pub async fn connect(endpoint: String) -> Result<ObserverClient, String> {
    let address = parse_endpoint(&endpoint)?;
    let client = Box::pin(runtime::execute(wf_observer_sdk::Client::connect(address)))
        .await?
        .map_err(|error| format!("{error:#}"))?;

    Ok(ObserverClient {
        inner: Arc::new(client),
    })
}

#[boltffi::export]
impl ObserverClient {
    /// Verifies that the service is reachable and speaking the expected protocol.
    ///
    /// # Errors
    ///
    /// Returns an error if the async runtime cannot start or the ping fails.
    pub async fn ping(&self) -> Result<(), String> {
        let client = Arc::clone(&self.inner);
        runtime::execute(async move { client.ping().await })
            .await?
            .map_err(|error| format!("{error:#}"))
    }

    /// Gracefully closes the underlying Iroh endpoint.
    ///
    /// Call this before releasing the generated foreign object. `BoltFFI`'s
    /// generated object disposal drops the Rust handle but cannot await a
    /// graceful transport shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error if the async runtime cannot start.
    pub async fn shutdown(&self) -> Result<(), String> {
        let client = Arc::clone(&self.inner);
        runtime::execute(async move { client.close().await }).await?;
        Ok(())
    }
}

fn parse_endpoint(endpoint: &str) -> Result<wf_observer_sdk::EndpointAddr, String> {
    if let Ok(ticket) = endpoint.parse::<EndpointTicket>() {
        return Ok(ticket.endpoint_addr().clone());
    }

    endpoint
        .parse::<EndpointId>()
        .map(Into::into)
        .map_err(|error| format!("invalid WF Observer endpoint or endpoint ticket: {error}"))
}

#[cfg(test)]
mod tests {
    use super::parse_endpoint;

    #[test]
    fn rejects_invalid_endpoint_text() {
        assert!(parse_endpoint("not-an-endpoint").is_err());
    }
}
