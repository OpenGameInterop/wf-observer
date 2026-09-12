use super::{Server, start};
use crate::{provider_host::HostedSession, service::ServiceState, test_support as fixture};
use anyhow::Context as _;
use std::time::Duration;
use tokio::time::{Instant, timeout};
use wf_observer_sdk as sdk;

struct TestClient {
    state: ServiceState,
    server: Server,
    client: sdk::ObserverClient,
}
impl TestClient {
    async fn start() -> anyhow::Result<Self> {
        let state = fixture::state()?;
        let server = start(iroh::SecretKey::generate(), state.view()).await?;
        let ticket = iroh_tickets::endpoint::EndpointTicket::new(server.endpoint().addr());
        let client = sdk::connect(ticket.to_string())
            .await
            .map_err(anyhow::Error::msg)?;
        Ok(Self {
            state,
            server,
            client,
        })
    }
    async fn subscribe(&self) -> anyhow::Result<sdk::ObserverSubscription> {
        Ok(self
            .client
            .subscribe(sdk::SessionSelector::All, vec![fixture::topic(0)])
            .await?)
    }
    async fn close(self) -> anyhow::Result<()> {
        self.client.shutdown().await.map_err(anyhow::Error::msg)?;
        self.state.shutdown();
        self.server.shutdown().await
    }
}

async fn next(sub: &sdk::ObserverSubscription) -> anyhow::Result<sdk::SubscriptionItem> {
    timeout(Duration::from_secs(5), sub.next())
        .await??
        .context("missing SDK frame")
}
async fn ready(sub: &sdk::ObserverSubscription) -> anyhow::Result<()> {
    assert!(sub.current().is_some());
    assert!(matches!(
        next(sub).await?,
        sdk::SubscriptionItem::State { .. }
    ));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn sdk_records_payloads_and_structured_errors_cross_the_runtime_boundary()
-> anyhow::Result<()> {
    let test = TestClient::start().await?;
    test.client.ping().await.map_err(anyhow::Error::msg)?;
    assert!(test.client.status().await?.targets.is_empty());
    assert!(!test.client.catalog().await?.providers.is_empty());
    fixture::observe(&test.state, &["one"])?;
    let sub = test.subscribe().await?;
    ready(&sub).await?;
    let mut host = HostedSession::start(&fixture::PROVIDER, fixture::FixtureMemory::new()?)?;
    host.poll(&test.state, "one", Instant::now())?;
    let mut saw_snapshot = false;
    loop {
        match next(&sub).await? {
            sdk::SubscriptionItem::State { value: state } => {
                for snapshot in state.topics.into_iter().filter_map(|topic| topic.snapshot) {
                    assert_eq!(
                        serde_json::from_str::<serde_json::Value>(&snapshot.payload_json)?["count"],
                        host.memory.value
                    );
                    assert!(snapshot.metadata.sequence.parse::<u64>()? > 0);
                    saw_snapshot = true;
                }
            }
            sdk::SubscriptionItem::Event { .. } => break,
            sdk::SubscriptionItem::Closed { reason } => {
                anyhow::bail!("listener closed before event: {reason:?}");
            }
        }
    }
    assert!(saw_snapshot);
    let request = fixture::request(&test.state, "one")?;
    let snapshot = test
        .client
        .snapshot(request.session.clone(), request.topic.clone())
        .await?;
    assert!(!snapshot.payload_json.is_empty());
    let mut wrong = request.topic;
    wrong.schema_version += 1;
    assert!(matches!(
        test.client.snapshot(request.session, wrong).await,
        Err(sdk::ObserverError::Request {
            error: sdk::RequestError::UnsupportedSchema { .. }
        })
    ));
    sub.shutdown().await?;
    assert!(sub.next().await?.is_none());
    test.close().await
}

#[tokio::test]
async fn sdk_close_wakes_next_and_concurrent_next_has_a_defined_error() -> anyhow::Result<()> {
    let test = TestClient::start().await?;
    let sub = test.subscribe().await?;
    ready(&sub).await?;
    let mut first = Box::pin(sub.next());
    let mut second = Box::pin(sub.next());
    let (first_finished, result) = timeout(Duration::from_secs(5), async {
        tokio::select! {
            result = &mut first => (true, result),
            result = &mut second => (false, result),
        }
    })
    .await?;
    assert!(matches!(result, Err(sdk::ObserverError::ConcurrentNext)));
    sub.shutdown().await?;
    let pending = if first_finished {
        second.await
    } else {
        first.await
    };
    assert!(pending?.is_none());
    assert!(sub.next().await?.is_none());

    let another = test.subscribe().await?;
    ready(&another).await?;
    assert!(
        timeout(Duration::from_millis(30), another.next())
            .await
            .is_err()
    );
    // Closing joins cleanup even when a foreign next future was cancelled.
    another.shutdown().await?;
    assert!(another.next().await?.is_none());
    test.close().await
}

#[tokio::test]
async fn sdk_preserves_terminal_lag_and_client_shutdown() -> anyhow::Result<()> {
    let test = TestClient::start().await?;
    test.state.set_message_limit(64 * 1024);
    fixture::observe(&test.state, &["one"])?;
    let sub = test.subscribe().await?;
    ready(&sub).await?;
    let ticket = test.state.ticket("one").context("missing session")?;
    test.state.commit_poll(
        &ticket,
        fixture::lag_batch(&test.state)?,
        Instant::now() + Duration::from_secs(10),
    )?;
    assert!(matches!(
        next(&sub).await?,
        sdk::SubscriptionItem::Closed {
            reason: sdk::SubscriptionEnd::ResyncRequired {
                reason: sdk::ResyncReason::Lagged {
                    resource: sdk::Resource::QueuedBytes
                }
            }
        }
    ));
    assert!(sub.next().await?.is_none());
    let fresh = test.subscribe().await?;
    ready(&fresh).await?;
    let (pending, shutdown) = tokio::join!(fresh.next(), test.client.shutdown());
    shutdown.map_err(anyhow::Error::msg)?;
    assert!(
        matches!(pending, Err(sdk::ObserverError::Closed)),
        "{pending:?}"
    );
    fresh.shutdown().await?;
    test.close().await
}
