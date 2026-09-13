use anyhow::Context as _;
use iroh::{
    Endpoint, EndpointAddr, SecretKey,
    endpoint::{ConnectionError, presets},
};
use protocol::v1 as wire;
use std::time::Duration;
use tokio::time::timeout;
use wf_observer_sdk::raw::{Client, ClientError, ClientIdentity};

use super::{
    Server,
    connection::SubscriptionLimits,
    server::{endpoint_builder, start_with_builder},
};
use crate::{authorization::Policy, settings::AccessMode, test_support as fixture};

const DEADLINE: Duration = Duration::from_secs(10);

async fn connect(server: &Server, identity: &ClientIdentity) -> Result<Client, ClientError> {
    timeout(
        DEADLINE,
        Client::connect_endpoint_with_identity(&server.local_ticket(), identity),
    )
    .await
    .map_err(|_| ClientError::Timeout)?
}

async fn direct_server(
    key: SecretKey,
    state: &crate::service::ServiceState,
    allowed: &[iroh::EndpointId],
) -> anyhow::Result<super::Server> {
    // Exercise the production remote gate over isolated loopback transports.
    start_with_builder(
        key,
        state.view(),
        Policy::new(AccessMode::Remote, allowed.iter().copied()),
        SubscriptionLimits::default(),
        endpoint_builder(AccessMode::Local)?,
    )
    .await
}

async fn assert_all_requests_denied(
    peer: &Endpoint,
    address: &EndpointAddr,
    state: &crate::service::ServiceState,
) -> anyhow::Result<()> {
    for request in 0..5 {
        let connection = timeout(DEADLINE, peer.connect(address.clone(), wire::ALPN_V1)).await??;
        let rpc = irpc::Client::<wire::ObserverProtocolV1>::boxed(
            irpc_iroh::IrohRemoteConnection::new(connection.clone()),
        );
        match request {
            0 => assert!(timeout(DEADLINE, rpc.rpc(wire::Ping)).await?.is_err()),
            1 => assert!(timeout(DEADLINE, rpc.rpc(wire::GetCatalog)).await?.is_err()),
            2 => assert!(timeout(DEADLINE, rpc.rpc(wire::GetStatus)).await?.is_err()),
            3 => assert!(
                timeout(DEADLINE, rpc.rpc(fixture::request(state, "one")?))
                    .await?
                    .is_err()
            ),
            _ => {
                if let Ok(mut stream) =
                    timeout(DEADLINE, rpc.server_streaming(fixture::selection(), 1)).await?
                {
                    assert!(timeout(DEADLINE, stream.recv()).await?.is_err());
                }
            }
        }
        let closed = timeout(DEADLINE, connection.closed()).await?;
        assert!(
            matches!(closed, ConnectionError::ApplicationClosed(close) if close.error_code.into_inner() == u64::from(wire::NOT_AUTHORIZED_CLOSE_CODE))
        );
    }
    assert_eq!(state.subscription_count(), 0);
    assert!(
        state
            .ticket("one")
            .context("session missing")?
            .topics
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn unapproved_reader_cannot_access_any_rpc_even_over_loopback() -> anyhow::Result<()> {
    let state = fixture::state()?;
    fixture::observe(&state, &["one"])?;
    let server = direct_server(SecretKey::generate(), &state, &[]).await?;
    let peer = Endpoint::bind(presets::Minimal).await?;
    assert_all_requests_denied(&peer, &server.endpoint().addr(), &state).await?;
    let result = timeout(DEADLINE, Client::connect_endpoint(&server.local_ticket())).await?;
    assert!(
        matches!(result, Err(ClientError::NotAuthorized)),
        "SDK must preserve the authorization rejection: {:?}",
        result.err()
    );
    let identity = wf_observer_sdk::create_identity();
    let result = timeout(DEADLINE, identity.connect(server.local_ticket())).await?;
    assert!(
        matches!(result, Err(wf_observer_sdk::ObserverError::NotAuthorized)),
        "FFI must preserve the authorization rejection: {:?}",
        result.err()
    );
    peer.close().await;
    server.shutdown().await
}

#[tokio::test]
async fn saved_identity_keeps_approval_and_revocation_ends_active_subscriptions()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("reader.key");
    let identity = ClientIdentity::load_or_create(&path)?;
    let state = fixture::state()?;
    fixture::observe(&state, &["one"])?;
    let key = SecretKey::generate();
    let server = direct_server(key.clone(), &state, &[identity.endpoint_id()]).await?;
    drop(identity);

    let restored = ClientIdentity::load_or_create(&path)?;
    let client = connect(&server, &restored).await?;
    assert!(!client.catalog().await?.providers.is_empty());
    assert_eq!(client.status().await?.targets.len(), 1);
    let subscription = client.subscribe(fixture::selection()).await?;
    assert_eq!(state.subscription_count(), 1);
    timeout(DEADLINE, server.shutdown()).await??;
    assert_eq!(state.subscription_count(), 0);
    timeout(DEADLINE, async {
        while subscription.current().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    client.close().await;

    let revoked = direct_server(key.clone(), &state, &[]).await?;
    assert_eq!(revoked.endpoint().id(), key.public());
    assert!(matches!(
        connect(&revoked, &restored).await,
        Err(ClientError::NotAuthorized)
    ));
    revoked.shutdown().await?;
    let approved_again = direct_server(key, &state, &[restored.endpoint_id()]).await?;
    let client = connect(&approved_again, &restored).await?;
    client.ping().await?;
    let stranger = ClientIdentity::generate();
    assert!(matches!(
        connect(&approved_again, &stranger).await,
        Err(ClientError::NotAuthorized)
    ));
    client.close().await;
    approved_again.shutdown().await
}
