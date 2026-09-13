use anyhow::Context as _;
use iroh::{Endpoint, EndpointAddr, SecretKey, endpoint::presets};
use std::{net::UdpSocket, time::Duration};
use tokio::time::timeout;
use wf_observer_sdk::raw::Client;

use crate::{settings::AccessMode, test_support as fixture};

use super::start;

#[tokio::test]
async fn local_mode_serves_the_sdk_without_public_discovery() -> anyhow::Result<()> {
    let state = fixture::state()?;
    let server = start(SecretKey::generate(), state.view(), AccessMode::Local).await?;
    assert!(!server.endpoint().bound_sockets().is_empty());
    assert!(
        server
            .endpoint()
            .bound_sockets()
            .iter()
            .all(|socket| socket.ip().is_loopback())
    );
    assert!(server.endpoint().address_lookup()?.is_empty());
    assert!(server.endpoint().addr().relay_urls().next().is_none());
    let client = timeout(
        Duration::from_secs(5),
        Client::connect_endpoint(&server.local_ticket()),
    )
    .await??;
    assert!(!client.catalog().await?.providers.is_empty());
    client.close().await;
    server.shutdown().await
}

#[tokio::test]
async fn switching_from_remote_closes_readers_and_keeps_identity_and_local_access()
-> anyhow::Result<()> {
    let state = fixture::state()?;
    let key = SecretKey::generate();
    let remote = start(key.clone(), state.view(), AccessMode::Remote).await?;
    assert!(
        remote
            .endpoint()
            .bound_sockets()
            .iter()
            .all(|socket| socket.ip().is_unspecified())
    );
    assert!(!remote.endpoint().address_lookup()?.is_empty());
    let peer = Endpoint::bind(presets::Minimal).await?;
    let address: iroh_tickets::endpoint::EndpointTicket = remote.local_ticket().parse()?;
    let connection = peer
        .connect(address.endpoint_addr().clone(), protocol::v1::ALPN_V1)
        .await?;
    remote.shutdown().await?;
    timeout(Duration::from_secs(5), connection.closed()).await?;
    let local = start(key.clone(), state.view(), AccessMode::Local).await?;
    assert_eq!(local.endpoint().id(), key.public());
    let client = Client::connect_endpoint(&local.local_ticket()).await?;
    client.ping().await?;
    client.close().await;
    peer.close().await;
    local.shutdown().await
}

#[tokio::test]
async fn only_remote_mode_accepts_a_non_loopback_destination() -> anyhow::Result<()> {
    // UDP connect chooses a route without sending data. Offline hosts can lack one.
    let route = UdpSocket::bind("0.0.0.0:0")?;
    if route.connect("192.0.2.1:9").is_err() {
        eprintln!("no IPv4 route; loopback isolation is covered separately");
        return Ok(());
    }
    let ip = route.local_addr()?.ip();
    if ip.is_loopback() || ip.is_unspecified() {
        eprintln!("no non-loopback IPv4 interface; loopback isolation is covered separately");
        return Ok(());
    }
    let state = fixture::state()?;
    let peer = Endpoint::bind(presets::Minimal).await?;
    for mode in [AccessMode::Local, AccessMode::Remote] {
        let server = start(SecretKey::generate(), state.view(), mode).await?;
        let port = server
            .endpoint()
            .bound_sockets()
            .iter()
            .find(|socket| socket.is_ipv4())
            .context("missing IPv4 socket")?
            .port();
        let address = EndpointAddr::new(server.endpoint().id()).with_ip_addr((ip, port).into());
        let result = timeout(
            Duration::from_secs(2),
            peer.connect(address, protocol::v1::ALPN_V1),
        )
        .await;
        match mode {
            AccessMode::Local => assert!(
                !matches!(result, Ok(Ok(_))),
                "local service accepted a non-loopback destination"
            ),
            AccessMode::Remote => {
                result??.close(0_u32.into(), b"test complete");
            }
        }
        server.shutdown().await?;
    }
    peer.close().await;
    Ok(())
}
