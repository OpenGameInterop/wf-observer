use super::{PollBatch, ServiceState, subscriptions::Subscription};
use crate::test_support as fixture;
use anyhow::Context as _;
use protocol::v1 as wire;
use provider_sdk::{CapabilityHealth, EventSink as _, HealthSink as _};
use std::time::Duration;
use tokio::time::Instant;

async fn next(sub: &mut Subscription) -> anyhow::Result<wire::SubscriptionItem> {
    tokio::time::timeout(Duration::from_secs(1), sub.next())
        .await?
        .context("subscription ended")
}

async fn bootstrap(sub: &mut Subscription) -> anyhow::Result<Vec<wire::SubscriptionItem>> {
    let mut frames = Vec::new();
    loop {
        let frame = next(sub).await?;
        let ready = matches!(frame, wire::SubscriptionItem::Ready(_));
        frames.push(frame);
        if ready {
            return Ok(frames);
        }
    }
}

fn batch(value: u32) -> anyhow::Result<PollBatch> {
    let batch = PollBatch::new(&fixture::MANIFEST);
    batch
        .events()
        .snapshot(&fixture::CAPS[0], &serde_json::json!({"count": value}))?;
    Ok(batch)
}

fn topic_state(state: &ServiceState, index: usize) -> wire::TopicSnapshot {
    state.shared.inner.lock().sessions["one"].topics[&fixture::topic(index)]
        .state
        .clone()
}

fn deadline(state: &ServiceState, index: usize) -> Option<Instant> {
    state.shared.inner.lock().sessions["one"].topics[&fixture::topic(index)].deadline
}

#[tokio::test]
async fn bootstrap_is_coherent_and_following_subscriptions_find_later_sessions()
-> anyhow::Result<()> {
    let state = fixture::state()?;
    let mut sub = state.view().subscribe(fixture::selection())?;
    let frames = bootstrap(&mut sub).await?;
    let [
        wire::SubscriptionItem::Begin(begin),
        wire::SubscriptionItem::Ready(ready),
    ] = frames.as_slice()
    else {
        anyhow::bail!("expected empty bootstrap");
    };
    assert_eq!(begin, ready);
    fixture::observe(&state, &["one", "two"])?;
    assert!(matches!(
        next(&mut sub).await?,
        wire::SubscriptionItem::Update(wire::UpdateEnvelope {
            update: wire::SubscriptionUpdate::SessionStarted(_),
            ..
        })
    ));
    let ticket = state.ticket("one").context("missing session")?;
    assert_eq!(ticket.topics.len(), 1);
    let mut second = state.view().subscribe(fixture::selection())?;
    let frames = bootstrap(&mut second).await?;
    let Some(wire::SubscriptionItem::Begin(begin)) = frames.first() else {
        anyhow::bail!("missing Begin");
    };
    let Some(wire::SubscriptionItem::Ready(ready)) = frames.last() else {
        anyhow::bail!("missing Ready");
    };
    assert_eq!(begin, ready);
    assert_eq!(
        frames
            .iter()
            .filter(|f| matches!(f, wire::SubscriptionItem::Session(_)))
            .count(),
        2
    );
    assert_eq!(
        frames
            .iter()
            .filter(|f| matches!(f, wire::SubscriptionItem::Topic(_)))
            .count(),
        2
    );
    Ok(())
}

#[test]
fn selections_are_bounded_by_frames_and_validated_against_the_catalog() -> anyhow::Result<()> {
    let state = fixture::state()?;
    let topics: Vec<_> = {
        let mut inner = state.shared.inner.lock();
        let provider = &mut inner.catalog.providers[0];
        let cap = provider.capabilities[1].clone();
        provider
            .capabilities
            .extend((2..40).map(|index| wire::CapabilityDescriptor {
                topic: format!("fixture.topic{index}"),
                ..cap.clone()
            }));
        provider
            .capabilities
            .iter()
            .map(|cap| wire::TopicRef {
                provider_id: provider.id.clone(),
                topic: cap.topic.clone(),
                schema_version: cap.schema_version,
            })
            .collect()
    };
    fixture::observe(&state, &["one"])?;
    let mut selection = fixture::selection();
    selection.topics = topics.iter().rev().chain(&topics).cloned().collect();
    let _sub = state.view().subscribe(selection.clone())?;
    assert_eq!(
        state.ticket("one").context("missing session")?.topics.len(),
        40
    );
    selection.topics[0].topic = "undeclared".into();
    assert!(matches!(
        state.view().subscribe(selection.clone()),
        Err(wire::RequestError::UnknownTopic { .. })
    ));
    state.set_message_limit(64);
    assert_eq!(
        state.view().subscribe(selection).err(),
        Some(wire::RequestError::LimitExceeded {
            resource: wire::Resource::MessageBytes,
        })
    );
    Ok(())
}

#[test]
fn subscriber_union_and_drop_control_snapshot_freshness() -> anyhow::Result<()> {
    let state = fixture::state()?;
    fixture::observe(&state, &["one"])?;
    let request = fixture::request(&state, "one")?;
    assert_eq!(
        state.view().snapshot(&request),
        Err(wire::RequestError::Idle)
    );
    assert!(
        state
            .ticket("one")
            .context("session missing")?
            .topics
            .is_empty()
    );
    let first = state.view().subscribe(fixture::selection())?;
    let ticket = state.ticket("one").context("session missing")?;
    assert_eq!(
        state.view().snapshot(&request),
        Err(wire::RequestError::NotSampled)
    );
    let second = state.view().subscribe(fixture::selection())?;
    assert_eq!(state.ticket("one"), Some(ticket.clone()));
    state.commit_poll(&ticket, batch(8)?, Instant::now() + Duration::from_secs(10))?;
    assert!(state.view().snapshot(&request).is_ok());
    drop(first);
    assert_eq!(state.ticket("one"), Some(ticket));
    drop(second);
    assert!(
        state
            .ticket("one")
            .context("session missing")?
            .topics
            .is_empty()
    );
    assert_eq!(
        state.view().snapshot(&request),
        Err(wire::RequestError::Idle)
    );
    Ok(())
}

#[test]
fn delayed_polls_cannot_cross_demand_or_source_generations() -> anyhow::Result<()> {
    let state = fixture::state()?;
    fixture::observe(&state, &["one"])?;
    let sub = state.view().subscribe(fixture::selection())?;
    let old = state.ticket("one").context("session missing")?;
    drop(sub);
    let _resumed = state.view().subscribe(fixture::selection())?;
    assert!(!state.commit_poll(&old, batch(10)?, Instant::now() + Duration::from_secs(10))?);
    let current = state.ticket("one").context("session missing")?;
    let reset = PollBatch::new(&fixture::MANIFEST);
    reset.events().reset(&fixture::CAPS[0])?;
    reset
        .events()
        .snapshot(&fixture::CAPS[0], &serde_json::json!({"count": 11}))?;
    assert!(state.commit_poll(&current, reset, Instant::now() + Duration::from_secs(10))?);
    assert!(!state.commit_poll(
        &current,
        batch(9)?,
        Instant::now() + Duration::from_secs(10)
    )?);
    let data = state.view().snapshot(&fixture::request(&state, "one")?)?;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(data.payload.as_str())?["count"],
        11
    );
    Ok(())
}

#[test]
fn overdue_acquisition_invalidates_only_its_session_and_rejects_late_output() -> anyhow::Result<()>
{
    let state = fixture::state()?;
    fixture::observe(&state, &["one", "two"])?;
    let _sub = state.view().subscribe(fixture::selection())?;
    let one = state.ticket("one").context("session missing")?;
    let two = state.ticket("two").context("session missing")?;
    let now = Instant::now();
    state.commit_poll(&one, batch(1)?, now + Duration::from_secs(1))?;
    state.commit_poll(&two, batch(2)?, now + Duration::from_secs(100))?;
    state.expire(now + Duration::from_secs(2));
    assert!(matches!(
        state.view().snapshot(&fixture::request(&state, "one")?),
        Err(wire::RequestError::Unavailable { .. })
    ));
    assert!(
        state
            .view()
            .snapshot(&fixture::request(&state, "two")?)
            .is_ok()
    );
    assert!(!state.commit_poll(&one, batch(3)?, now + Duration::from_secs(100))?);
    Ok(())
}

#[test]
fn freshness_expires_silent_topics_despite_other_poll_output() -> anyhow::Result<()> {
    for available in [false, true] {
        let state = fixture::state()?;
        fixture::observe(&state, &["one"])?;
        let mut selection = fixture::selection();
        selection.topics.push(fixture::topic(1));
        let _sub = state.view().subscribe(selection)?;
        let ticket = state.ticket("one").context("missing session")?;
        let original = deadline(&state, 0).context("missing initial deadline")?;
        if available {
            state.commit_poll(&ticket, batch(7)?, original)?;
        }
        let silent = topic_state(&state, 0);
        let later = original + Duration::from_secs(100);
        let active = PollBatch::new(&fixture::MANIFEST);
        active
            .events()
            .snapshot(&fixture::CAPS[1], &serde_json::json!(8))?;
        state.commit_poll(&ticket, active, later)?;
        for build in [None, Some("metadata-only")] {
            let empty = PollBatch::new(&fixture::MANIFEST);
            empty.game_build(build)?;
            state.commit_poll(&ticket, empty, later)?;
            assert_eq!(deadline(&state, 0), Some(original));
            assert_eq!(topic_state(&state, 0), silent);
        }
        state.expire(original);
        assert!(matches!(
            topic_state(&state, 0).health,
            wire::CapabilityHealth::Unavailable { .. }
        ));
        assert!(topic_state(&state, 0).snapshot.is_none());
        assert_eq!(
            topic_state(&state, 1).health,
            wire::CapabilityHealth::Available
        );
        let current = state.ticket("one").context("missing retry")?;
        let health = PollBatch::new(&fixture::MANIFEST);
        health
            .health()
            .update(&fixture::CAPS[0], CapabilityHealth::Available)?;
        let cursor = state.view().status()?.cursor;
        assert!(state.commit_poll(&current, health, later).is_err());
        assert_eq!(state.view().status()?.cursor, cursor);
        assert_eq!(deadline(&state, 0), None);
    }
    Ok(())
}

#[test]
fn topic_failure_preserves_other_snapshots_and_accepts_healthy_publication() -> anyhow::Result<()> {
    let state = fixture::state()?;
    fixture::observe(&state, &["one"])?;
    let mut selection = fixture::selection();
    selection.topics.push(fixture::topic(1));
    let _sub = state.view().subscribe(selection)?;
    let ticket = state.ticket("one").context("missing session")?;
    let baseline = batch(7)?;
    baseline
        .events()
        .snapshot(&fixture::CAPS[1], &serde_json::json!(8))?;
    let first_deadline = Instant::now() + Duration::from_secs(10);
    state.commit_poll(&ticket, baseline, first_deadline)?;
    let healthy = topic_state(&state, 1);
    let later = first_deadline + Duration::from_secs(10);
    for publish_healthy in [false, true] {
        let output = PollBatch::new(&fixture::MANIFEST);
        output.health().update(
            &fixture::CAPS[0],
            CapabilityHealth::Unavailable(provider_sdk::UnavailableReason::UnsupportedBuild),
        )?;
        if publish_healthy {
            output
                .events()
                .snapshot(&fixture::CAPS[1], &serde_json::json!(9))?;
        }
        assert!(state.commit_poll(&ticket, output, later)?);
        assert!(topic_state(&state, 0).snapshot.is_none());
        assert_eq!(deadline(&state, 0), None);
        assert_eq!(
            topic_state(&state, 1).health,
            wire::CapabilityHealth::Available
        );
        assert_eq!(topic_state(&state, 1).generation, healthy.generation);
        if publish_healthy {
            assert_eq!(deadline(&state, 1), Some(later));
            assert_ne!(topic_state(&state, 1).snapshot, healthy.snapshot);
        } else {
            assert_eq!(topic_state(&state, 1), healthy);
            assert_eq!(deadline(&state, 1), Some(first_deadline));
        }
    }
    Ok(())
}

#[test]
fn freshness_renewal_preserves_sequences_and_respects_operation_order() -> anyhow::Result<()> {
    let state = fixture::state()?;
    fixture::observe(&state, &["one"])?;
    let mut selection = fixture::selection();
    selection.topics.push(fixture::topic(1));
    let _sub = state.view().subscribe(selection)?;
    let ticket = state.ticket("one").context("missing session")?;
    let now = Instant::now();
    let baseline = batch(7)?;
    baseline
        .events()
        .snapshot(&fixture::CAPS[1], &serde_json::json!(8))?;
    state.commit_poll(&ticket, baseline, now + Duration::from_secs(1))?;
    let before = [topic_state(&state, 0), topic_state(&state, 1)];
    let cursor = state.view().status()?.cursor;
    let renewed = now + Duration::from_secs(100);
    let unchanged = batch(7)?;
    unchanged
        .health()
        .update(&fixture::CAPS[1], CapabilityHealth::Available)?;
    state.commit_poll(&ticket, unchanged, renewed)?;
    assert_eq!(state.view().status()?.cursor, cursor);
    for (index, topic) in before.into_iter().enumerate() {
        assert_eq!(topic_state(&state, index), topic);
        assert_eq!(deadline(&state, index), Some(renewed));
    }
    state.expire(now + Duration::from_secs(2));
    assert_eq!(state.view().status()?.cursor, cursor);

    for snapshot_last in [false, true] {
        let ordered = PollBatch::new(&fixture::MANIFEST);
        let unavailable =
            CapabilityHealth::Unavailable(provider_sdk::UnavailableReason::TargetNotReady);
        if snapshot_last {
            ordered
                .health()
                .update(&fixture::CAPS[0], unavailable.clone())?;
        }
        ordered
            .events()
            .snapshot(&fixture::CAPS[0], &serde_json::json!(9))?;
        if !snapshot_last {
            ordered.health().update(&fixture::CAPS[0], unavailable)?;
        }
        state.commit_poll(&ticket, ordered, renewed)?;
        assert_eq!(deadline(&state, 0), snapshot_last.then_some(renewed));
        assert_eq!(topic_state(&state, 0).snapshot.is_some(), snapshot_last);
    }
    let reset = PollBatch::new(&fixture::MANIFEST);
    reset.events().reset(&fixture::CAPS[0])?;
    reset
        .health()
        .update(&fixture::CAPS[0], CapabilityHealth::Available)?;
    let cursor = state.view().status()?.cursor;
    assert!(state.commit_poll(&ticket, reset, renewed).is_err());
    assert_eq!(state.view().status()?.cursor, cursor);
    assert_eq!(deadline(&state, 0), Some(renewed));
    let reset = PollBatch::new(&fixture::MANIFEST);
    reset.events().reset(&fixture::CAPS[0])?;
    let grace = renewed + Duration::from_secs(1);
    state.commit_poll(&ticket, reset, grace)?;
    assert_eq!(
        topic_state(&state, 0).health,
        wire::CapabilityHealth::Initializing
    );
    assert!(topic_state(&state, 0).snapshot.is_none());
    assert_eq!(deadline(&state, 0), Some(grace));
    let current = state.ticket("one").context("missing reset ticket")?;
    state.commit_poll(
        &current,
        PollBatch::new(&fixture::MANIFEST),
        grace + Duration::from_secs(100),
    )?;
    assert_eq!(deadline(&state, 0), Some(grace));
    Ok(())
}

#[test]
fn freshness_events_require_successful_scan_heartbeats() -> anyhow::Result<()> {
    static EVENT_ONLY: provider_sdk::ProviderManifest = provider_sdk::ProviderManifest {
        capabilities: &[provider_sdk::CapabilityDescriptor {
            snapshots: false,
            ..fixture::CAPS[0]
        }],
        ..fixture::MANIFEST
    };
    for manifest in [&fixture::MANIFEST, &EVENT_ONLY] {
        let state = ServiceState::new(&[manifest])?;
        fixture::observe(&state, &["one"])?;
        let _sub = state.view().subscribe(fixture::selection())?;
        let ticket = state.ticket("one").context("missing session")?;
        let cap = &manifest.capabilities[0];
        let original = Instant::now() + Duration::from_secs(1);
        let later = original + Duration::from_secs(100);
        let baseline = PollBatch::new(manifest);
        if cap.snapshots {
            baseline.events().snapshot(cap, &serde_json::json!(7))?;
        } else {
            baseline.health().update(cap, CapabilityHealth::Available)?;
        }
        state.commit_poll(&ticket, baseline, original)?;
        let before = topic_state(&state, 0);
        let event = PollBatch::new(manifest);
        event.events().event(cap, &serde_json::json!(8))?;
        state.commit_poll(&ticket, event, later)?;
        assert_eq!(deadline(&state, 0), Some(original));
        assert_eq!(topic_state(&state, 0), before);
        let cursor = state.view().status()?.cursor;
        let quiet_scan = PollBatch::new(manifest);
        quiet_scan
            .health()
            .update(cap, CapabilityHealth::Available)?;
        state.commit_poll(&ticket, quiet_scan, later)?;
        assert_eq!(state.view().status()?.cursor, cursor);
        assert_eq!(deadline(&state, 0), Some(later));
        state.expire(original);
        assert_eq!(state.view().status()?.cursor, cursor);
        state.expire(later);
        let current = state.ticket("one").context("missing retry")?;
        let event = PollBatch::new(manifest);
        event.events().event(cap, &serde_json::json!(9))?;
        let cursor = state.view().status()?.cursor;
        assert!(state.commit_poll(&current, event, later).is_err());
        assert_eq!(state.view().status()?.cursor, cursor);
        assert_eq!(deadline(&state, 0), None);
    }
    Ok(())
}

#[tokio::test]
async fn discarded_resets_survive_expiry_once_and_only_for_their_topic() -> anyhow::Result<()> {
    for expired in 0..2 {
        let state = fixture::state()?;
        fixture::observe(&state, &["one"])?;
        let mut selection = fixture::selection();
        selection.topics.push(fixture::topic(1));
        let mut sub = state.view().subscribe(selection)?;
        bootstrap(&mut sub).await?;
        let ticket = state.ticket("one").context("missing session")?;
        let baseline = batch(7)?;
        baseline
            .events()
            .snapshot(&fixture::CAPS[1], &serde_json::json!({"count": 7}))?;
        let now = Instant::now();
        state.commit_poll(&ticket, baseline, now + Duration::from_secs(100))?;
        next(&mut sub).await?;
        next(&mut sub).await?;
        let generation = topic_state(&state, 0).generation;
        state
            .shared
            .inner
            .lock()
            .sessions
            .get_mut("one")
            .context("missing session")?
            .topics
            .get_mut(&fixture::topic(expired))
            .context("missing topic")?
            .deadline = Some(now);
        state.expire(now);
        next(&mut sub).await?;
        let other = topic_state(&state, 1);

        let resets = [fixture::topic(0), fixture::topic(0)];
        let retry = state.discard_poll(&ticket, &resets, false);
        // Expiry of either topic fences publication but does not change demand.
        assert!(retry.is_some());
        assert_eq!(retry, state.ticket("one"));
        assert_eq!(topic_state(&state, 1), other);
        assert!(matches!(
            next(&mut sub).await?,
            wire::SubscriptionItem::Update(wire::UpdateEnvelope {
                update: wire::SubscriptionUpdate::TopicReset {
                    source, generation: actual, reason: wire::ResetReason::SourceChanged,
                }, ..
            }) if source.topic == fixture::topic(0) && actual == generation + 1
        ));
        let failed = topic_state(&state, 0);
        assert_eq!(failed.generation, generation + 1);
        assert!(failed.snapshot.is_none());
        assert!(matches!(
            failed.health,
            wire::CapabilityHealth::Unavailable {
                reason: wire::UnavailableReason::ProviderFailed { .. }
            }
        ));
        assert!(matches!(
            next(&mut sub).await?,
            wire::SubscriptionItem::Update(wire::UpdateEnvelope {
                update: wire::SubscriptionUpdate::TopicChanged(topic), ..
            }) if topic == failed
        ));
        let cursor = state.view().status()?.cursor;
        assert!(state.discard_poll(&ticket, &resets, false).is_none());
        assert_eq!(state.view().status()?.cursor, cursor);
        let retry = state.ticket("one").context("missing retry")?;
        assert!(state.commit_poll(&retry, batch(11)?, now + Duration::from_secs(100))?);
        assert_eq!(
            state
                .view()
                .snapshot(&fixture::request(&state, "one")?)?
                .metadata
                .generation,
            generation + 1
        );
        assert_eq!(state.discard_poll(&retry, &[], true), Some(retry));
        assert_eq!(topic_state(&state, 0).generation, generation + 1);
        assert!(topic_state(&state, 0).snapshot.is_none());
    }
    Ok(())
}

#[test]
fn discarded_polls_cannot_acknowledge_new_demand_or_reset_a_resumed_generation()
-> anyhow::Result<()> {
    let state = fixture::state()?;
    fixture::observe(&state, &["one"])?;
    let first = state.view().subscribe(fixture::selection())?;
    let ticket = state.ticket("one").context("missing session")?;
    let mut selection = fixture::selection();
    selection.topics = vec![fixture::topic(1)];
    let second = state.view().subscribe(selection)?;
    let other = topic_state(&state, 1);
    let cursor = state.view().status()?.cursor;
    assert!(
        state
            .discard_poll(&ticket, &[fixture::topic(1)], false)
            .is_none()
    );
    assert_eq!(state.view().status()?.cursor, cursor);
    assert!(
        state
            .discard_poll(&ticket, &[fixture::topic(0)], false)
            .is_none()
    );
    assert_eq!(topic_state(&state, 1), other);
    drop(first);
    drop(second);
    let cursor = state.view().status()?.cursor;
    assert!(
        state
            .discard_poll(&ticket, &[fixture::topic(0)], true)
            .is_none()
    );
    assert_eq!(state.view().status()?.cursor, cursor);
    let _resumed = state.view().subscribe(fixture::selection())?;
    let resumed = topic_state(&state, 0);
    let cursor = state.view().status()?.cursor;
    assert!(
        state
            .discard_poll(&ticket, &[fixture::topic(0)], true)
            .is_none()
    );
    assert_eq!(topic_state(&state, 0), resumed);
    assert_eq!(state.view().status()?.cursor, cursor);
    fixture::observe(&state, &[])?;
    assert!(
        state
            .discard_poll(&ticket, &[fixture::topic(0)], true)
            .is_none()
    );
    state.shutdown();
    assert!(
        state
            .discard_poll(&ticket, &[fixture::topic(0)], true)
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn discarded_reset_exhaustion_closes_before_mutating_generations() -> anyhow::Result<()> {
    for exhausted in 0..3 {
        let state = fixture::state()?;
        fixture::observe(&state, &["one"])?;
        let mut sub = state.view().subscribe(fixture::selection())?;
        bootstrap(&mut sub).await?;
        {
            let mut inner = state.shared.inner.lock();
            if exhausted == 2 {
                inner.sequence = u64::MAX - 1;
            } else {
                let topic = inner
                    .sessions
                    .get_mut("one")
                    .context("missing session")?
                    .topics
                    .get_mut(&fixture::topic(0))
                    .context("missing topic")?;
                if exhausted == 0 {
                    topic.state.generation = u64::MAX;
                } else {
                    topic.epoch = u64::MAX;
                }
            }
        }
        let ticket = state.ticket("one").context("missing session")?;
        let generation = topic_state(&state, 0).generation;
        assert!(
            state
                .discard_poll(&ticket, &[fixture::topic(0)], true)
                .is_none()
        );
        assert_eq!(topic_state(&state, 0).generation, generation);
        assert!(state.ticket("one").is_none());
        assert_eq!(
            next(&mut sub).await?,
            wire::SubscriptionItem::Closed(wire::SubscriptionEnd::ResyncRequired {
                reason: wire::ResyncReason::SequenceExhausted
            })
        );
    }
    Ok(())
}

#[tokio::test]
async fn near_limit_frames_fit_live_and_bootstrap_but_a_backlog_is_bounded() -> anyhow::Result<()> {
    let state = fixture::state()?;
    state.set_message_limit(4096);
    fixture::observe(&state, &["one"])?;
    let mut sub = state.view().subscribe(fixture::selection())?;
    bootstrap(&mut sub).await?;
    let ticket = state.ticket("one").context("missing session")?;
    for value in 0..3 {
        let output = PollBatch::new(&fixture::MANIFEST);
        output.events().snapshot(
            &fixture::CAPS[0],
            &serde_json::json!({"value": value, "padding": "x".repeat(3584)}),
        )?;
        assert!(state.commit_poll(&ticket, output, Instant::now() + Duration::from_secs(10))?);
        if value == 0 {
            assert!(matches!(
                next(&mut sub).await?,
                wire::SubscriptionItem::Update(wire::UpdateEnvelope {
                    update: wire::SubscriptionUpdate::TopicChanged(wire::TopicSnapshot {
                        snapshot: Some(_),
                        ..
                    }),
                    ..
                })
            ));
            let mut late = state.view().subscribe(fixture::selection())?;
            assert!(bootstrap(&mut late).await?.iter().any(|item| matches!(
                item,
                wire::SubscriptionItem::Topic(wire::TopicSnapshot {
                    snapshot: Some(_),
                    ..
                })
            )));
        }
    }
    assert!(matches!(
        next(&mut sub).await?,
        wire::SubscriptionItem::Closed(wire::SubscriptionEnd::ResyncRequired {
            reason: wire::ResyncReason::Lagged {
                resource: wire::Resource::QueuedBytes
            }
        })
    ));
    Ok(())
}

#[tokio::test]
async fn lag_releases_its_demand_without_rejecting_other_publications() -> anyhow::Result<()> {
    let state = fixture::state()?;
    state.set_message_limit(64 * 1024);
    fixture::observe(&state, &["one"])?;
    let mut sub = state.view().subscribe(fixture::selection())?;
    let mut selection = fixture::selection();
    selection.topics = vec![fixture::topic(1)];
    let mut other = state.view().subscribe(selection)?;
    bootstrap(&mut sub).await?;
    bootstrap(&mut other).await?;
    let ticket = state.ticket("one").context("session missing")?;
    let small = batch(1)?;
    let message = serde_json::json!({"text": "x".repeat(128)});
    for _ in 0..65 {
        small.events().event(&fixture::CAPS[0], &message)?;
    }
    assert!(state.commit_poll(&ticket, small, Instant::now() + Duration::from_secs(10))?);
    assert!(matches!(
        next(&mut sub).await?,
        wire::SubscriptionItem::Update(_)
    ));
    for _ in 0..65 {
        assert!(matches!(
            next(&mut sub).await?,
            wire::SubscriptionItem::Update(wire::UpdateEnvelope {
                update: wire::SubscriptionUpdate::Event(_),
                ..
            })
        ));
    }
    let burst = fixture::lag_batch(&state)?;
    burst
        .events()
        .snapshot(&fixture::CAPS[1], &serde_json::json!(7))?;
    assert!(state.commit_poll(&ticket, burst, Instant::now() + Duration::from_secs(10))?);
    assert_eq!(
        next(&mut sub).await?,
        wire::SubscriptionItem::Closed(wire::SubscriptionEnd::ResyncRequired {
            reason: wire::ResyncReason::Lagged {
                resource: wire::Resource::QueuedBytes
            }
        })
    );
    assert!(sub.next().await.is_none());
    assert_eq!(topic_state(&state, 0).health, wire::CapabilityHealth::Idle);
    let expected = topic_state(&state, 1);
    assert_eq!(expected.health, wire::CapabilityHealth::Available);
    assert!(expected.snapshot.is_some());
    assert!(matches!(
        next(&mut other).await?,
        wire::SubscriptionItem::Update(wire::UpdateEnvelope {
            update: wire::SubscriptionUpdate::TopicChanged(topic), ..
        }) if topic == expected
    ));
    assert_eq!(
        state.ticket("one").context("session missing")?.topics.len(),
        1
    );
    drop(other);
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
async fn session_end_and_shutdown_wake_pending_readers() -> anyhow::Result<()> {
    for retrying in [false, true] {
        let state = fixture::state()?;
        fixture::observe(&state, &["one", "two"])?;
        let mut selected = fixture::selection();
        selected.sessions = wire::SessionSelector::Session {
            reference: fixture::request(&state, "one")?.session,
        };
        let mut one = state.view().subscribe(selected)?;
        let mut all = state.view().subscribe(fixture::selection())?;
        bootstrap(&mut one).await?;
        bootstrap(&mut all).await?;
        let mut status = state.shared.inner.lock().runtime.clone();
        let reason = if retrying {
            status.targets[0].activity = crate::runtime::Activity::Retrying {
                error: "verification failed".into(),
            };
            wire::SessionEndReason::ProviderFailed
        } else {
            status.targets.remove(0);
            wire::SessionEndReason::TargetExited
        };
        state.update_lifecycle(status)?;
        assert!(matches!(
            next(&mut one).await?,
            wire::SubscriptionItem::Update(wire::UpdateEnvelope {
                update: wire::SubscriptionUpdate::SessionEnded { reason: actual, .. },
                ..
            }) if actual == reason
        ));
        assert_eq!(
            next(&mut one).await?,
            wire::SubscriptionItem::Closed(wire::SubscriptionEnd::SessionEnded)
        );
        assert!(
            !state
                .ticket("two")
                .context("session missing")?
                .topics
                .is_empty()
        );
        state.shutdown();
        assert_eq!(
            next(&mut all).await?,
            wire::SubscriptionItem::Closed(wire::SubscriptionEnd::ServiceStopped)
        );
    }
    Ok(())
}

#[test]
fn rejected_setup_and_publication_do_not_leak_demand_or_partial_data() -> anyhow::Result<()> {
    let state = fixture::state()?;
    fixture::observe(&state, &["one"])?;
    state.set_message_limit(64);
    assert!(matches!(
        state.view().subscribe(fixture::selection()),
        Err(wire::RequestError::LimitExceeded {
            resource: wire::Resource::MessageBytes
        })
    ));
    assert!(
        state
            .ticket("one")
            .context("session missing")?
            .topics
            .is_empty()
    );
    state.set_message_limit(4096);
    let _sub = state.view().subscribe(fixture::selection())?;
    let ticket = state.ticket("one").context("session missing")?;
    let oversized = serde_json::json!("x".repeat(4094));
    assert_eq!(serde_json::to_string(&oversized)?.len(), 4096);
    for event in [false, true] {
        let invalid = batch(7)?;
        invalid.game_build(Some("rejected-build"))?;
        if event {
            invalid.events().event(&fixture::CAPS[0], &oversized)?;
        } else {
            invalid.events().snapshot(&fixture::CAPS[0], &oversized)?;
        }
        let cursor = state.view().status()?.cursor;
        assert_eq!(
            state.commit_poll(&ticket, invalid, Instant::now()),
            Err(wire::RequestError::LimitExceeded {
                resource: wire::Resource::MessageBytes,
            })
        );
        assert_eq!(state.view().status()?.cursor, cursor);
    }
    assert!(
        state.shared.inner.lock().sessions["one"]
            .info
            .game_build
            .is_none()
    );
    let oversized_metadata = batch(7)?;
    assert!(
        oversized_metadata
            .game_build(Some(&"x".repeat(1024)))
            .is_err()
    );
    assert!(
        state
            .commit_poll(&ticket, oversized_metadata, Instant::now())
            .is_err()
    );
    assert_eq!(
        state.view().snapshot(&fixture::request(&state, "one")?),
        Err(wire::RequestError::NotSampled)
    );
    let original = Instant::now() + Duration::from_secs(1);
    state.commit_poll(&ticket, batch(7)?, original)?;
    let before = topic_state(&state, 0);
    for demanded in [false, true] {
        let _second = demanded
            .then(|| {
                let mut selection = fixture::selection();
                selection.topics = vec![fixture::topic(1)];
                state.view().subscribe(selection)
            })
            .transpose()?;
        let other = topic_state(&state, 1);
        let unauthorized = batch(8)?;
        unauthorized
            .events()
            .snapshot(&fixture::CAPS[1], &serde_json::json!(9))?;
        let cursor = state.view().status()?.cursor;
        assert!(
            state
                .commit_poll(&ticket, unauthorized, original + Duration::from_secs(100))
                .is_err()
        );
        assert_eq!(state.view().status()?.cursor, cursor);
        assert_eq!(topic_state(&state, 0), before);
        assert_eq!(topic_state(&state, 1), other);
        assert_eq!(deadline(&state, 0), Some(original));
    }
    Ok(())
}
