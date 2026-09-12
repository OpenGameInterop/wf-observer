use super::{Server, connection::SubscriptionLimits, server::start_with_limits};
use crate::{
    service::{PollBatch, ServiceState},
    test_support as fixture,
};
use anyhow::Context as _;
use iroh::{
    Endpoint,
    endpoint::{Connection, presets},
};
use irpc::{channel::mpsc::Receiver, util::AsyncWriteVarintExt as _};
use protocol::v1 as wire;
use provider_sdk::EventSink as _;
use std::time::Duration;
use tokio::time::{Instant, timeout};

type Rpc = irpc::Client<wire::ObserverProtocolV1>;
type Stream = Receiver<Result<wire::SubscriptionItem, wire::RequestError>>;

struct TestServer {
    state: ServiceState,
    server: Server,
    endpoint: Endpoint,
    connection: Connection,
    rpc: Rpc,
}

impl TestServer {
    async fn start() -> anyhow::Result<Self> {
        Self::with_limits(SubscriptionLimits::default()).await
    }

    async fn with_limits(limits: SubscriptionLimits) -> anyhow::Result<Self> {
        let state = fixture::state()?;
        let server = start_with_limits(iroh::SecretKey::generate(), state.view(), limits).await?;
        let endpoint = Endpoint::bind(presets::N0).await?;
        let connection = endpoint
            .connect(server.endpoint().addr(), wire::ALPN_V1)
            .await?;
        let rpc = Rpc::boxed(irpc_iroh::IrohRemoteConnection::new(connection.clone()));
        Ok(Self {
            state,
            server,
            endpoint,
            connection,
            rpc,
        })
    }

    async fn subscribe(&self) -> anyhow::Result<Stream> {
        Ok(self.rpc.server_streaming(fixture::selection(), 1).await?)
    }

    async fn close(self) -> anyhow::Result<()> {
        self.state.shutdown();
        self.endpoint.close().await;
        self.server.shutdown().await
    }
}

async fn next(stream: &mut Stream) -> anyhow::Result<wire::SubscriptionItem> {
    Ok(timeout(Duration::from_secs(5), stream.recv())
        .await??
        .context("missing stream frame")??)
}

async fn bootstrap(stream: &mut Stream) -> anyhow::Result<()> {
    assert!(matches!(
        next(stream).await?,
        wire::SubscriptionItem::Begin(_)
    ));
    loop {
        if matches!(next(stream).await?, wire::SubscriptionItem::Ready(_)) {
            return Ok(());
        }
    }
}

async fn wait_idle(state: &ServiceState) -> anyhow::Result<()> {
    timeout(Duration::from_secs(5), async {
        while state
            .ticket("one")
            .is_some_and(|ticket| !ticket.topics.is_empty())
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn many_subscriptions_share_connection_without_blocking_requests() -> anyhow::Result<()> {
    // Exercise transport credit above the 32 request slots without increasing
    // the production subscription budget.
    let test = TestServer::with_limits(SubscriptionLimits {
        per_connection: 34,
        global: 34,
    })
    .await?;
    fixture::observe(&test.state, &["one"])?;
    let mut held = Vec::new();
    for _ in 0..33 {
        let mut stream = test.subscribe().await?;
        bootstrap(&mut stream).await?;
        held.push(stream);
    }
    test.rpc.rpc(wire::Ping).await?;
    let other_endpoint = Endpoint::bind(presets::N0).await?;
    let other = irpc_iroh::client::<wire::ObserverProtocolV1>(
        other_endpoint.clone(),
        test.server.endpoint().addr(),
        wire::ALPN_V1,
    );
    let mut separate = other.server_streaming(fixture::selection(), 1).await?;
    bootstrap(&mut separate).await?;
    drop(held);
    other_endpoint.close().await;
    wait_idle(&test.state).await?;
    // The first connection remains usable after stream cancellation.
    test.rpc.rpc(wire::Ping).await?;
    let mut resumed = test.subscribe().await?;
    bootstrap(&mut resumed).await?;
    test.endpoint.close().await;
    wait_idle(&test.state).await?;
    test.close().await
}

async fn subscription_error(
    rpc: &Rpc,
    selection: wire::Subscribe,
) -> anyhow::Result<wire::RequestError> {
    let mut stream = timeout(Duration::from_secs(2), rpc.server_streaming(selection, 1)).await??;
    let frame = timeout(Duration::from_secs(2), stream.recv())
        .await??
        .context("missing setup rejection")?;
    let Err(error) = frame else {
        anyhow::bail!("rejected subscription received a bootstrap frame");
    };
    assert!(
        timeout(Duration::from_secs(2), stream.recv())
            .await??
            .is_none()
    );
    Ok(error)
}

async fn wait_subscriptions(state: &ServiceState, count: usize) -> anyhow::Result<()> {
    timeout(Duration::from_secs(5), async {
        while state.subscription_count() != count {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    Ok(())
}

fn topic_status(state: &ServiceState, key: &wire::TopicRef) -> anyhow::Result<wire::TopicStatus> {
    let status = state.view().status()?;
    status
        .targets
        .into_iter()
        .find_map(|target| {
            if let wire::TargetActivity::Observing { topics, .. } = target.activity {
                topics.into_iter().find(|topic| &topic.topic == key)
            } else {
                None
            }
        })
        .context("missing topic status")
}

#[tokio::test]
async fn subscription_limits_preserve_control_requests_and_release_on_cancel_and_disconnect()
-> anyhow::Result<()> {
    let test = TestServer::with_limits(SubscriptionLimits {
        per_connection: 2,
        global: 3,
    })
    .await?;
    fixture::observe(&test.state, &["one"])?;

    // Invalid requests must return their reserved subscription permits too.
    let mut invalid = fixture::selection();
    invalid.topics.clear();
    assert!(matches!(
        subscription_error(&test.rpc, invalid).await?,
        wire::RequestError::InvalidRequest { .. }
    ));
    let mut first = test.subscribe().await?;
    bootstrap(&mut first).await?;
    let mut second = test.subscribe().await?;
    bootstrap(&mut second).await?;

    // Rejection happens before demand: even this topic's generation stays idle.
    let mut undemanded = fixture::selection();
    undemanded.topics = vec![fixture::topic(1)];
    let idle = topic_status(&test.state, &fixture::topic(1))?;
    let exhausted = wire::RequestError::LimitExceeded {
        resource: wire::Resource::Subscriptions,
    };
    assert_eq!(
        subscription_error(&test.rpc, undemanded.clone()).await?,
        exhausted
    );
    assert_eq!(topic_status(&test.state, &fixture::topic(1))?, idle);
    assert_eq!(test.state.subscription_count(), 2);

    let other_endpoint = Endpoint::bind(presets::N0).await?;
    let other = irpc_iroh::client::<wire::ObserverProtocolV1>(
        other_endpoint.clone(),
        test.server.endpoint().addr(),
        wire::ALPN_V1,
    );
    let mut third = other.server_streaming(fixture::selection(), 1).await?;
    bootstrap(&mut third).await?;
    // This peer has a local slot left, but the global budget is exhausted.
    assert_eq!(subscription_error(&other, undemanded).await?, exhausted);
    assert_eq!(topic_status(&test.state, &fixture::topic(1))?, idle);
    assert_eq!(test.state.subscription_count(), 3);
    for rpc in [&test.rpc, &other] {
        timeout(Duration::from_secs(2), rpc.rpc(wire::Ping)).await??;
        timeout(Duration::from_secs(2), rpc.rpc(wire::GetStatus)).await???;
    }

    drop(first);
    wait_subscriptions(&test.state, 2).await?;
    let mut replacement = test.subscribe().await?;
    bootstrap(&mut replacement).await?;
    assert_eq!(test.state.subscription_count(), 3);

    // Disconnect with live receivers: both server-side slots must be released.
    test.endpoint.close().await;
    wait_subscriptions(&test.state, 1).await?;
    let replacement_endpoint = Endpoint::bind(presets::N0).await?;
    let replacement_rpc = irpc_iroh::client::<wire::ObserverProtocolV1>(
        replacement_endpoint.clone(),
        test.server.endpoint().addr(),
        wire::ALPN_V1,
    );
    let mut after_disconnect = Vec::new();
    for _ in 0..2 {
        let mut stream = replacement_rpc
            .server_streaming(fixture::selection(), 1)
            .await?;
        bootstrap(&mut stream).await?;
        after_disconnect.push(stream);
    }
    assert_eq!(test.state.subscription_count(), 3);
    replacement_endpoint.close().await;
    other_endpoint.close().await;
    wait_subscriptions(&test.state, 0).await?;
    test.close().await
}

#[tokio::test]
async fn shutdown_finishes_with_subscriptions_on_an_open_client() -> anyhow::Result<()> {
    let test = TestServer::start().await?;
    fixture::observe(&test.state, &["one"])?;
    let mut streams = Vec::new();
    for _ in 0..4 {
        let mut stream = test.subscribe().await?;
        bootstrap(&mut stream).await?;
        streams.push(stream);
    }
    test.state.shutdown();
    for stream in &mut streams {
        assert_eq!(
            next(stream).await?,
            wire::SubscriptionItem::Closed(wire::SubscriptionEnd::ServiceStopped)
        );
    }
    // The peer retains its endpoint, connection and RPC handles throughout shutdown.
    timeout(Duration::from_secs(10), test.server.shutdown()).await??;
    assert_eq!(test.state.subscription_count(), 0);
    assert!(test.connection.close_reason().is_some());
    test.endpoint.close().await;
    Ok(())
}

#[tokio::test]
async fn stalled_and_oversized_requests_do_not_block_the_connection() -> anyhow::Result<()> {
    let test = TestServer::start().await?;
    test.rpc.rpc(wire::Ping).await?;
    let (mut stalled, _stalled_response) = test.connection.open_bi().await?;
    stalled.write_all(&[0x80]).await?;
    timeout(Duration::from_secs(2), test.rpc.rpc(wire::Ping)).await??;

    let (mut oversized, mut response) = test.connection.open_bi().await?;
    oversized
        .write_varint_u64(u64::from(test.state.view().message_bytes()) + 1)
        .await?;
    oversized.finish()?;
    assert!(
        timeout(Duration::from_secs(2), response.read_to_end(1024))
            .await?
            .is_err()
    );
    test.rpc.rpc(wire::Ping).await?;
    let _ = stalled.reset(0_u32.into());
    test.close().await
}

#[tokio::test]
async fn blocked_writer_times_out_and_releases_its_subscription() -> anyhow::Result<()> {
    let test = TestServer::start().await?;
    fixture::observe(&test.state, &["one"])?;
    let slow_endpoint = Endpoint::builder(presets::N0)
        .transport_config(
            iroh::endpoint::QuicTransportConfig::builder()
                .stream_receive_window(1024_u32.into())
                .receive_window(4096_u32.into())
                .build(),
        )
        .bind()
        .await?;
    let slow = irpc_iroh::client::<wire::ObserverProtocolV1>(
        slow_endpoint.clone(),
        test.server.endpoint().addr(),
        wire::ALPN_V1,
    );
    let mut stream = slow.server_streaming(fixture::selection(), 1).await?;
    bootstrap(&mut stream).await?;
    let padding = "x".repeat(128 * 1024);
    // The peer grants only a small receive window, so this frame cannot finish
    // until it reads. Keep the state queue empty to exercise the send deadline.
    let ticket = test.state.ticket("one").context("missing session")?;
    let batch = PollBatch::new(&fixture::MANIFEST);
    batch
        .events()
        .snapshot(&fixture::CAPS[0], &serde_json::json!({"padding": padding}))?;
    test.state
        .commit_poll(&ticket, batch, Instant::now() + Duration::from_secs(30))?;
    timeout(Duration::from_secs(2), slow.rpc(wire::Ping)).await??;
    timeout(Duration::from_secs(8), async {
        while test
            .state
            .ticket("one")
            .is_some_and(|ticket| !ticket.topics.is_empty())
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    // A timed-out partial frame is reset, not followed by a corrupt Closed frame.
    assert!(stream.recv().await.is_err());
    slow_endpoint.close().await;
    test.close().await
}
