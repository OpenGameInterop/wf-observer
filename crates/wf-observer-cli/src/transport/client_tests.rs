use super::{Server, start};
use crate::{
    provider_host::HostedSession,
    service::{PollBatch, ServiceState},
    test_support as fixture,
};
use anyhow::Context as _;
use protocol::v1 as wire;
use provider_sdk::{EventSink as _, HealthSink as _};
use std::{sync::Arc, time::Duration};
use tokio::time::{Instant, timeout};
use wf_observer_sdk::raw::{
    Client, ClientError, Subscription, SubscriptionItem, SubscriptionState, Topic,
};

struct Counter;
#[derive(Debug, Clone, serde::Deserialize, ..Eq)]
struct Count {
    count: u32,
}
impl Topic for Counter {
    const GAME_ID: &'static str = "fixture";
    const PROVIDER_ID: &'static str = "fixture.provider";
    const NAME: &'static str = "fixture.count";
    const SCHEMA_VERSION: u32 = 1;
}
impl wf_observer_sdk::raw::SnapshotTopic for Counter {
    type Snapshot = Count;
}
impl wf_observer_sdk::raw::EventTopic for Counter {
    type Event = Count;
}

struct TestClient {
    state: ServiceState,
    server: Server,
    client: Client,
}
impl TestClient {
    async fn start() -> anyhow::Result<Self> {
        let state = fixture::state()?;
        let server = start(iroh::SecretKey::generate(), state.view()).await?;
        let client = Client::connect(server.endpoint().addr()).await?;
        Ok(Self {
            state,
            server,
            client,
        })
    }
    async fn close(self) -> anyhow::Result<()> {
        self.client.close().await;
        self.state.shutdown();
        self.server.shutdown().await
    }
    async fn subscriptions(&self, count: usize) -> anyhow::Result<()> {
        timeout(Duration::from_secs(5), async {
            while self.state.subscription_count() != count {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await?;
        Ok(())
    }
}

async fn next(sub: &Subscription) -> anyhow::Result<SubscriptionItem> {
    timeout(Duration::from_secs(5), sub.next())
        .await??
        .context("missing observation")
}
async fn state_where(
    sub: &Subscription,
    predicate: impl Fn(&SubscriptionState) -> bool,
) -> anyhow::Result<SubscriptionState> {
    loop {
        if let SubscriptionItem::State(state) = next(sub).await?
            && predicate(&state)
        {
            return Ok(state);
        }
    }
}

#[tokio::test]
async fn components_share_feeds_and_late_listeners_share_current_snapshots() -> anyhow::Result<()> {
    let test = TestClient::start().await?;
    fixture::observe(&test.state, &["one"])?;
    let (first, second) = tokio::try_join!(
        test.client.subscribe(fixture::selection()),
        test.client.subscribe(fixture::selection())
    )?;
    assert_eq!(test.state.subscription_count(), 1);
    assert!(
        first.current().context("closed")?.topics[0]
            .snapshot
            .is_none()
    );
    let mut host = HostedSession::start(&fixture::PROVIDER, fixture::FixtureMemory::new()?)?;
    host.poll(&test.state, "one", Instant::now())?;
    let state = state_where(&first, |state| {
        state.topics.iter().any(|t| t.snapshot.is_some())
    })
    .await?;
    let snapshot = state.topics[0]
        .snapshot
        .clone()
        .context("snapshot missing")?;
    assert_eq!(
        wf_observer_sdk::raw::decode_snapshot::<Counter>(snapshot.clone())?
            .data
            .count,
        7
    );
    assert_eq!(
        test.client
            .snapshot_typed::<Counter>(&snapshot.metadata.source.session)
            .await?
            .data
            .count,
        7
    );
    let mut components = Vec::new();
    for _ in 0..70 {
        components.push(test.client.subscribe(fixture::selection()).await?);
    }
    assert_eq!(test.state.subscription_count(), 1);
    let latest = components[0].current().context("closed")?;
    assert!(Arc::ptr_eq(&latest.topics[0], &state.topics[0]));
    assert_eq!(host.memory.reads, 1);
    first.close();
    drop(components);
    assert_eq!(test.state.subscription_count(), 1);
    assert!(second.current().is_some());
    second.close();
    test.subscriptions(0).await?;
    test.close().await
}

#[tokio::test]
async fn snapshot_and_event_watches_share_mixed_topic_without_closing_on_events()
-> anyhow::Result<()> {
    use wf_observer_sdk::raw::{Capability, EventCapability, EventObservation, State};

    let test = TestClient::start().await?;
    fixture::observe(&test.state, &["one"])?;
    let session = fixture::request(&test.state, "one")?.session;
    let snapshots = Capability::<Counter>::new(test.client.clone(), session.clone())
        .watch()
        .await?;
    let events = EventCapability::<Counter>::new(test.client.clone(), session.clone())
        .watch()
        .await?;
    let generic = test
        .client
        .subscribe(wire::Subscribe {
            sessions: wire::SessionSelector::Session { reference: session },
            topics: vec![Counter::topic()],
        })
        .await?;
    assert_eq!(test.state.subscription_count(), 1);
    assert!(matches!(snapshots.next().await?, Some(State::Waiting)));

    let mut host = HostedSession::start(&fixture::PROVIDER, fixture::FixtureMemory::new()?)?;
    for (elapsed, count) in [(0, 7), (1, 9)] {
        host.memory.value = count;
        host.poll(
            &test.state,
            "one",
            Instant::now() + Duration::from_secs(elapsed),
        )?;
        let snapshot = timeout(Duration::from_secs(5), snapshots.next()).await??;
        assert!(matches!(snapshot, Some(State::Ready(value)) if value.data.count == count));
        timeout(Duration::from_secs(5), async {
            loop {
                match events.next().await?.context("event watch ended")? {
                    EventObservation::State(_) => {}
                    EventObservation::Event(value) => {
                        assert_eq!(value.data.count, count);
                        break;
                    }
                }
            }
            loop {
                if let SubscriptionItem::Event(value) = next(&generic).await? {
                    assert_eq!(
                        wf_observer_sdk::raw::decode_event::<Counter>((*value).clone())?
                            .data
                            .count,
                        count
                    );
                    break;
                }
            }
            anyhow::Ok(())
        })
        .await??;
        assert!(matches!(snapshots.current()?, State::Ready(value) if value.data.count == count));
    }
    assert!(
        timeout(Duration::from_millis(30), snapshots.next())
            .await
            .is_err()
    );
    snapshots.close();
    assert_eq!(test.state.subscription_count(), 1);
    events.close();
    generic.close();
    test.subscriptions(0).await?;
    test.close().await
}

#[tokio::test]
async fn mixed_selection_reuses_topics_and_failure_does_not_close_other_components()
-> anyhow::Result<()> {
    let test = TestClient::start().await?;
    fixture::observe(&test.state, &["one"])?;
    let inventory = test.client.subscribe(fixture::selection()).await?;
    let both = wire::Subscribe {
        sessions: wire::SessionSelector::All,
        topics: vec![fixture::topic(1), fixture::topic(0), fixture::topic(1)],
    };
    let combined = test.client.subscribe(both).await?;
    assert_eq!(test.state.subscription_count(), 2);
    assert_eq!(combined.current().context("closed")?.topics.len(), 2);
    let mut invalid = fixture::selection();
    invalid.topics.push(wire::TopicRef {
        topic: "missing".into(),
        ..fixture::topic(0)
    });
    assert!(matches!(
        test.client.subscribe(invalid).await,
        Err(ClientError::Request(
            wire::RequestError::UnknownTopic { .. }
        ))
    ));
    assert!(inventory.current().is_some());
    combined.close();
    test.subscriptions(1).await?;
    assert_eq!(
        test.state
            .ticket("one")
            .context("missing session")?
            .topics
            .len(),
        1
    );
    inventory.close();
    test.subscriptions(0).await?;
    test.close().await
}

#[tokio::test]
async fn listener_state_replaces_source_data_and_clears_unavailable_and_ended_sessions()
-> anyhow::Result<()> {
    let test = TestClient::start().await?;
    fixture::observe(&test.state, &["one"])?;
    let sub = test.client.subscribe(fixture::selection()).await?;
    let mut generation = sub.current().context("closed")?.topics[0].generation;
    let ticket = test.state.ticket("one").context("missing session")?;
    let batch = PollBatch::new(&fixture::MANIFEST);
    batch.events().reset(&fixture::CAPS[0])?;
    batch
        .events()
        .snapshot(&fixture::CAPS[0], &serde_json::json!({"count": 9}))?;
    test.state
        .commit_poll(&ticket, batch, Instant::now() + Duration::from_secs(30))?;
    let state = state_where(&sub, |state| state.topics[0].snapshot.is_some()).await?;
    assert!(state.topics[0].generation > generation);
    generation = state.topics[0].generation;
    let batch = PollBatch::new(&fixture::MANIFEST);
    batch.health().update(
        &fixture::CAPS[0],
        provider_sdk::CapabilityHealth::Unavailable(
            provider_sdk::UnavailableReason::TargetNotReady,
        ),
    )?;
    test.state.commit_poll(
        &test.state.ticket("one").context("missing session")?,
        batch,
        Instant::now() + Duration::from_secs(30),
    )?;
    state_where(&sub, |state| {
        matches!(
            state.topics[0].health,
            wire::CapabilityHealth::Unavailable { .. }
        )
    })
    .await?;
    let state = sub.current().context("closed")?;
    assert_eq!(state.topics[0].generation, generation);
    assert!(state.topics[0].snapshot.is_none());
    fixture::observe(&test.state, &[])?;
    state_where(&sub, |state| {
        state.sessions.is_empty() && state.topics.is_empty()
    })
    .await?;
    test.state.shutdown();
    assert_eq!(
        next(&sub).await?,
        SubscriptionItem::Closed(wire::SubscriptionEnd::ServiceStopped)
    );
    assert!(sub.current().is_none());
    assert!(sub.next().await?.is_none());
    test.close().await
}

#[tokio::test]
async fn cancellation_and_concurrent_next_belong_to_the_rust_listener() -> anyhow::Result<()> {
    let test = TestClient::start().await?;
    let sub = test.client.subscribe(fixture::selection()).await?;
    let other = test.client.subscribe(fixture::selection()).await?;
    assert!(matches!(next(&sub).await?, SubscriptionItem::State(_)));
    assert!(
        timeout(Duration::from_millis(30), sub.next())
            .await
            .is_err()
    );
    let (pending, check) = tokio::join!(sub.next(), async {
        tokio::task::yield_now().await;
        assert!(matches!(sub.next().await, Err(ClientError::ConcurrentNext)));
        sub.close();
        anyhow::Ok(())
    });
    check?;
    assert!(pending?.is_none());
    assert!(sub.current().is_none());
    assert!(other.current().is_some());
    assert!(matches!(next(&other).await?, SubscriptionItem::State(_)));
    let (pending, ()) = tokio::join!(other.next(), async {
        tokio::task::yield_now().await;
        test.client.close().await;
    });
    assert!(matches!(pending, Err(ClientError::Closed)));
    assert!(other.current().is_none());
    assert!(other.next().await?.is_none());
    test.close().await
}

#[tokio::test]
async fn public_client_reports_upstream_lag_and_can_open_a_fresh_feed() -> anyhow::Result<()> {
    let test = TestClient::start().await?;
    test.state.set_message_limit(64 * 1024);
    fixture::observe(&test.state, &["one"])?;
    let sub = test.client.subscribe(fixture::selection()).await?;
    next(&sub).await?;
    test.state.commit_poll(
        &test.state.ticket("one").context("missing session")?,
        fixture::lag_batch(&test.state)?,
        Instant::now() + Duration::from_secs(30),
    )?;
    assert!(matches!(
        next(&sub).await?,
        SubscriptionItem::Closed(wire::SubscriptionEnd::ResyncRequired { .. })
    ));
    assert!(sub.current().is_none());
    let fresh = test.client.subscribe(fixture::selection()).await?;
    assert!(fresh.current().is_some());
    test.close().await
}

#[tokio::test]
async fn session_scopes_stay_independent_and_dropped_clients_release_all_feeds()
-> anyhow::Result<()> {
    let test = TestClient::start().await?;
    fixture::observe(&test.state, &["one", "two"])?;
    let all = test.client.subscribe(fixture::selection()).await?;
    let one = test
        .client
        .subscribe(wire::Subscribe {
            sessions: wire::SessionSelector::Session {
                reference: fixture::request(&test.state, "one")?.session,
            },
            topics: vec![fixture::topic(0)],
        })
        .await?;
    assert_eq!(test.state.subscription_count(), 2);
    assert_eq!(all.current().context("closed")?.sessions.len(), 2);
    assert_eq!(one.current().context("closed")?.sessions.len(), 1);
    all.close();
    test.subscriptions(1).await?;
    assert!(
        test.state
            .ticket("two")
            .context("missing session")?
            .topics
            .is_empty()
    );
    fixture::observe(&test.state, &["two"])?;
    loop {
        if let SubscriptionItem::Closed(reason) = next(&one).await? {
            assert_eq!(reason, wire::SubscriptionEnd::SessionEnded);
            break;
        }
    }
    assert!(one.current().is_none());
    let again = test.client.subscribe(fixture::selection()).await?;
    let TestClient {
        state,
        server,
        client,
    } = test;
    drop(client);
    assert!(again.current().is_none());
    assert!(matches!(again.next().await, Err(ClientError::Closed)));
    state.shutdown();
    server.shutdown().await
}

#[tokio::test]
async fn one_component_can_select_many_topics_without_exhausting_request_slots()
-> anyhow::Result<()> {
    let capabilities = (0..33)
        .map(|index| provider_sdk::CapabilityDescriptor {
            topic: Box::leak(format!("fixture.{index}").into_boxed_str()),
            ..fixture::CAPS[0]
        })
        .collect::<Vec<_>>();
    let manifest = provider_sdk::ProviderManifest {
        capabilities: Box::leak(capabilities.into_boxed_slice()),
        ..fixture::MANIFEST
    };
    let state = ServiceState::new(&[&manifest])?;
    fixture::observe(&state, &["one"])?;
    let server = super::server::start_with_limits(
        iroh::SecretKey::generate(),
        state.view(),
        super::connection::SubscriptionLimits {
            per_connection: 34,
            global: 34,
        },
    )
    .await?;
    let client = Client::connect(server.endpoint().addr()).await?;
    let subscription = timeout(
        Duration::from_secs(10),
        client.subscribe(wire::Subscribe {
            sessions: wire::SessionSelector::All,
            topics: manifest
                .capabilities
                .iter()
                .map(|cap| wire::TopicRef {
                    provider_id: manifest.id.into(),
                    topic: cap.topic.into(),
                    schema_version: cap.schema_version,
                })
                .collect(),
        }),
    )
    .await??;
    assert_eq!(subscription.current().context("closed")?.topics.len(), 33);
    assert_eq!(state.subscription_count(), 33);
    client.ping().await?;
    client.close().await;
    assert!(subscription.current().is_none());
    state.shutdown();
    server.shutdown().await
}
