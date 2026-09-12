use super::{
    Subscription, SubscriptionItem,
    feed::{Feed, FeedState},
    listener::Listener,
    validation::Validation,
};
use crate::raw::ClientError;
use anyhow::Context as _;
use iroh::{Endpoint, endpoint::presets};
use irpc::util::AsyncWriteVarintExt as _;
use protocol::v1 as wire;
use std::{sync::Arc, time::Duration};

fn selection() -> wire::Subscribe {
    wire::Subscribe {
        sessions: wire::SessionSelector::All,
        topics: vec![wire::TopicRef {
            provider_id: "test".into(),
            topic: "test.counter".into(),
            schema_version: 1,
        }],
    }
}

fn session() -> wire::SessionInfo {
    wire::SessionInfo {
        session: wire::SessionRef {
            run_id: "run".into(),
            session_id: "session".into(),
        },
        provider_id: "test".into(),
        game_id: "test".into(),
        game_build: None,
        target: wire::TargetProcess {
            pid: 1,
            executable: "test.exe".into(),
        },
    }
}

fn cursor(sequence: u64) -> wire::ServiceCursor {
    wire::ServiceCursor {
        run_id: "run".into(),
        sequence,
    }
}

async fn snapshot_bootstrap(
    send: &mut iroh::endpoint::SendStream,
    initial: &wire::DataEnvelope,
) -> anyhow::Result<()> {
    for item in [
        wire::SubscriptionItem::Begin(cursor(initial.metadata.sequence)),
        wire::SubscriptionItem::Session(session()),
        wire::SubscriptionItem::Snapshot(wire::SnapshotFrame {
            cursor: None,
            metadata: initial.metadata.clone(),
            payload: wire::SnapshotPayload::Full(initial.payload.clone()),
        }),
        wire::SubscriptionItem::Ready(cursor(initial.metadata.sequence)),
    ] {
        send.write_length_prefixed(Ok::<_, wire::RequestError>(item))
            .await?;
    }
    Ok(())
}

fn snapshot_data(sequence: u64) -> anyhow::Result<wire::DataEnvelope> {
    Ok(wire::DataEnvelope {
        metadata: event(1, sequence, "")?.metadata.clone(),
        payload: wire::JsonPayload::from_value(&serde_json::json!({
            "quantity": sequence.to_string(), "unchanged": "x".repeat(1024),
        }))?,
    })
}

#[tokio::test]
async fn snapshots_are_acknowledged_after_install_and_bad_deltas_get_a_full_bootstrap()
-> anyhow::Result<()> {
    use irpc::util::AsyncReadVarintExt as _;

    let snapshots = (1..=3)
        .map(snapshot_data)
        .collect::<anyhow::Result<Vec<_>>>()?;
    let server = Endpoint::builder(presets::N0)
        .alpns(vec![wire::ALPN_V1.to_vec()])
        .bind()
        .await?;
    let endpoint = server.clone();
    let values = snapshots.clone();
    let (advance, mut advanced) = tokio::sync::mpsc::channel(1);
    let worker = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
        let connection = endpoint
            .accept()
            .await
            .context("missing connection")?
            .await?;
        let (mut send, mut recv) = connection.accept_bi().await?;
        recv.read_length_prefixed::<wire::ObserverProtocolV1>(1024)
            .await?;
        send.write_length_prefixed(wire::Pong).await?;
        send.finish()?;
        for initial in [&values[0], &values[2]] {
            let (mut send, mut recv) = connection.accept_bi().await?;
            assert!(matches!(
                recv.read_length_prefixed::<wire::ObserverProtocolV1>(1024)
                    .await?,
                wire::ObserverProtocolV1::Subscribe(_)
            ));
            snapshot_bootstrap(&mut send, initial).await?;
            if initial.metadata.sequence == 3 {
                advanced.recv().await.context("consumer ended early")?;
                send.write_length_prefixed(Ok::<_, wire::RequestError>(
                    wire::SubscriptionItem::Closed(wire::SubscriptionEnd::ServiceStopped),
                ))
                .await?;
                send.finish()?;
                break;
            }
            for index in 0..2 {
                let ack = recv.read_length_prefixed::<wire::SnapshotAck>(4096).await?;
                assert_eq!(ack.metadata, values[index].metadata);
                assert_eq!(ack.hash, values[index].payload.hash());
                advanced.recv().await.context("consumer ended early")?;
                let target = &values[index + 1];
                let mut payload = wire::SnapshotPayload::between(&values[index], target)?;
                if index == 1
                    && let wire::SnapshotPayload::Delta { hash, .. } = &mut payload
                {
                    hash[0] ^= 1;
                }
                send.write_length_prefixed(Ok::<_, wire::RequestError>(
                    wire::SubscriptionItem::Snapshot(wire::SnapshotFrame {
                        cursor: Some(cursor(target.metadata.sequence)),
                        metadata: target.metadata.clone(),
                        payload,
                    }),
                ))
                .await?;
            }
            // Recovery closes this stream and starts another subscription on the same connection.
            send.stopped().await?;
        }
        connection.closed().await;
        anyhow::Ok(())
    }));
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let client = crate::raw::Client::connect(server.addr()).await?;
        let sub = client.subscribe(selection()).await?;
        for expected in snapshots {
            loop {
                if let Some(SubscriptionItem::State(state)) = sub.next().await?
                    && let Some(snapshot) = state
                        .topics
                        .iter()
                        .find_map(|topic| topic.snapshot.as_ref())
                {
                    assert_eq!(snapshot, &expected);
                    break;
                }
            }
            advance.send(()).await?;
        }
        assert_eq!(
            sub.next().await?,
            Some(SubscriptionItem::Closed(
                wire::SubscriptionEnd::ServiceStopped
            ))
        );
        client.close().await;
        anyhow::Ok(())
    })
    .await;
    drop(advance);
    if !matches!(&result, Ok(Ok(()))) {
        worker.abort();
    }
    server.close().await;
    let joined = tokio::time::timeout(Duration::from_secs(5), worker).await;
    result??;
    joined??
}

#[test]
fn live_metadata_has_a_local_budget_independent_of_bootstrap() -> anyhow::Result<()> {
    let mut validation = Validation::new(selection());
    validation.accept(&wire::SubscriptionItem::Begin(cursor(0)))?;
    validation.accept(&wire::SubscriptionItem::Ready(cursor(0)))?;
    validation.accept(&wire::SubscriptionItem::Update(wire::UpdateEnvelope {
        cursor: cursor(1),
        update: wire::SubscriptionUpdate::SessionStarted(session()),
    }))?;
    for index in 0..16 {
        let mut info = session();
        info.session.session_id = index.to_string();
        info.game_build = Some("x".repeat(512 * 1024));
        let result = validation.accept(&wire::SubscriptionItem::Update(wire::UpdateEnvelope {
            cursor: cursor(index + 2),
            update: wire::SubscriptionUpdate::SessionStarted(info),
        }));
        if index == 15 {
            assert!(matches!(result, Err(ClientError::Protocol(_))));
        } else {
            result?;
        }
    }
    Ok(())
}

#[test]
fn invalid_bootstrap_and_stale_generations_require_resubscription() -> anyhow::Result<()> {
    let mut invalid = Validation::new(selection());
    assert!(
        invalid
            .accept(&wire::SubscriptionItem::Ready(cursor(0)))
            .is_err()
    );

    let mut validation = Validation::new(selection());
    validation.accept(&wire::SubscriptionItem::Begin(cursor(0)))?;
    validation.accept(&wire::SubscriptionItem::Session(session()))?;
    let topic = wire::TopicSnapshot {
        source: wire::TopicSource {
            session: session().session,
            game_id: "test".into(),
            topic: selection().topics.remove(0),
        },
        generation: 1,
        health: wire::CapabilityHealth::Initializing,
        snapshot: None,
    };
    validation.accept(&wire::SubscriptionItem::Topic(topic.clone()))?;
    validation.accept(&wire::SubscriptionItem::Ready(cursor(0)))?;
    validation.accept(&wire::SubscriptionItem::Update(wire::UpdateEnvelope {
        cursor: cursor(1),
        update: wire::SubscriptionUpdate::TopicReset {
            source: topic.source.clone(),
            generation: 2,
            reason: wire::ResetReason::SourceChanged,
        },
    }))?;
    assert!(
        validation
            .accept(&wire::SubscriptionItem::Update(wire::UpdateEnvelope {
                cursor: cursor(2),
                update: wire::SubscriptionUpdate::TopicChanged(topic),
            }))
            .is_err()
    );
    Ok(())
}

fn local_listener(feed: &Arc<Feed>) -> Subscription {
    listener_with_events(feed, true)
}

fn listener_with_events(feed: &Arc<Feed>, receive_events: bool) -> Subscription {
    let sub = Subscription {
        inner: Arc::new(Listener::new(receive_events)),
    };
    assert!(feed.attach(&sub.inner));
    assert!(sub.inner.register(feed.clone()));
    sub
}

fn event(
    generation: u64,
    sequence: u64,
    payload: &str,
) -> anyhow::Result<Arc<wire::EventEnvelope>> {
    Ok(Arc::new(wire::EventEnvelope {
        metadata: wire::EnvelopeMetadata {
            source: wire::TopicSource {
                session: session().session,
                game_id: "test".into(),
                topic: selection().topics.remove(0),
            },
            generation,
            sequence,
        },
        payload: wire::JsonPayload::from_value(&serde_json::json!(payload))?,
    }))
}

#[tokio::test]
async fn slow_event_listener_does_not_stop_a_shared_feed_or_other_listeners() -> anyhow::Result<()>
{
    let feed = Arc::new(Feed::new(selection()));
    let slow = local_listener(&feed);
    let fast = local_listener(&feed);
    let snapshots = listener_with_events(&feed, false);
    let event = event(1, 1, &"x".repeat(1024 * 1024))?;
    for _ in 0..16 {
        slow.inner.event(event.clone());
        fast.inner.event(event.clone());
        snapshots.inner.event(event.clone());
        assert_eq!(
            fast.next().await?,
            Some(SubscriptionItem::Event(event.clone()))
        );
    }
    assert!(matches!(slow.next().await, Err(ClientError::Lagged)));
    assert!(slow.current().is_none());
    assert!(slow.next().await?.is_none());
    assert!(snapshots.current().is_some());
    assert!(
        tokio::time::timeout(Duration::from_millis(10), snapshots.next())
            .await
            .is_err()
    );
    assert!(!feed.stop.is_cancelled());
    fast.close();
    snapshots.close();
    assert!(feed.stop.is_cancelled());
    Ok(())
}

#[tokio::test]
async fn local_state_coalesces_and_expires_events_from_replaced_sources() -> anyhow::Result<()> {
    let feed = Arc::new(Feed::new(selection()));
    let sub = local_listener(&feed);
    let old = event(1, 1, "old account")?;
    sub.inner.event(old.clone());
    let mut state = FeedState::default();
    state.sessions.insert(session().session, (2, session()));
    state.topics.insert(
        session().session,
        Arc::new(wire::TopicSnapshot {
            source: old.metadata.source.clone(),
            generation: 2,
            health: wire::CapabilityHealth::Initializing,
            snapshot: None,
        }),
    );
    sub.inner
        .state(selection().topics.remove(0), Arc::new(state));
    let fresh = event(2, 3, "new account")?;
    sub.inner.event(fresh.clone());
    assert!(
        matches!(sub.next().await?, Some(SubscriptionItem::State(state)) if state.topics[0].generation == 2)
    );
    assert_eq!(
        sub.next().await?,
        Some(SubscriptionItem::Event(fresh.clone()))
    );
    sub.inner.event(fresh);
    sub.inner
        .state(selection().topics.remove(0), Arc::default());
    sub.inner
        .state(selection().topics.remove(0), Arc::default());
    assert!(
        matches!(sub.next().await?, Some(SubscriptionItem::State(state)) if state.sessions.is_empty() && state.topics.is_empty())
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(10), sub.next())
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn cancelling_setup_releases_a_feed_waiting_for_its_bootstrap() -> anyhow::Result<()> {
    use irpc::util::AsyncReadVarintExt as _;

    let server = Endpoint::builder(presets::N0)
        .alpns(vec![wire::ALPN_V1.to_vec()])
        .bind()
        .await?;
    let endpoint = server.clone();
    let (started, waiting) = tokio::sync::oneshot::channel();
    let (stopped, released) = tokio::sync::oneshot::channel();
    let worker = tokio::spawn(async move {
        let connection = endpoint
            .accept()
            .await
            .context("missing connection")?
            .await?;
        let (mut send, mut recv) = connection.accept_bi().await?;
        assert!(matches!(
            recv.read_length_prefixed::<wire::ObserverProtocolV1>(1024)
                .await?,
            wire::ObserverProtocolV1::Ping(_)
        ));
        send.write_length_prefixed(wire::Pong).await?;
        send.finish()?;
        let (mut send, mut recv) = connection.accept_bi().await?;
        assert!(matches!(
            recv.read_length_prefixed::<wire::ObserverProtocolV1>(1024)
                .await?,
            wire::ObserverProtocolV1::Subscribe(_)
        ));
        send.write_length_prefixed(Ok::<_, wire::RequestError>(wire::SubscriptionItem::Begin(
            cursor(0),
        )))
        .await?;
        let _ = started.send(());
        tokio::time::timeout(Duration::from_secs(5), send.stopped()).await??;
        let _ = stopped.send(());
        anyhow::Ok(())
    });
    let client = crate::raw::Client::connect(server.addr()).await?;
    let mut setup = Box::pin(client.subscribe(selection()));
    tokio::select! {
        result = &mut setup => { anyhow::bail!("setup completed without Ready: {:?}", result.err()); }
        started = waiting => started?,
    }
    drop(setup);
    released.await?;
    worker.await??;
    client.close().await;
    server.close().await;
    Ok(())
}
