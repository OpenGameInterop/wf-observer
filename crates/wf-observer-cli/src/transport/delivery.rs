//! Per-stream acknowledged snapshot baselines. Service queues retain full publications.

use anyhow::Context as _;
use iroh::endpoint::{RecvStream, SendStream};
use irpc::util::AsyncReadVarintExt as _;
use protocol::v1 as wire;
use std::collections::{BTreeMap, BTreeSet};

use crate::service::Subscription;

// ACKs contain bounded source identifiers, counters and a hash, never payloads.
const ACK_BYTES: usize = 4096;

struct Baseline {
    snapshot: wire::DataEnvelope,
    acknowledged: bool,
}

struct Delivery {
    topics: BTreeSet<wire::TopicRef>,
    baselines: BTreeMap<(wire::SessionRef, wire::TopicRef), Baseline>,
}

impl Delivery {
    fn encode(&mut self, item: wire::SubscriptionItem) -> anyhow::Result<wire::SubscriptionItem> {
        let (cursor, topic) = match &item {
            wire::SubscriptionItem::Topic(topic) => (None, topic),
            wire::SubscriptionItem::Update(update) => match &update.update {
                wire::SubscriptionUpdate::TopicChanged(topic) => {
                    (Some(update.cursor.clone()), topic)
                }
                wire::SubscriptionUpdate::TopicReset { source, .. } => {
                    self.baselines
                        .remove(&(source.session.clone(), source.topic.clone()));
                    return Ok(item);
                }
                wire::SubscriptionUpdate::SessionEnded { session, .. } => {
                    self.baselines.retain(|(key, _), _| key != session);
                    return Ok(item);
                }
                _ => return Ok(item),
            },
            _ => return Ok(item),
        };
        if !self.topics.contains(&topic.source.topic) {
            return Ok(item);
        }
        let key = (topic.source.session.clone(), topic.source.topic.clone());
        let Some(target) = &topic.snapshot else {
            self.baselines.remove(&key);
            return Ok(item);
        };
        let mut payload = wire::SnapshotPayload::Full(target.payload.clone());
        // An update that overtakes its ACK stays a full replacement. This keeps
        // global stream ordering without holding back resets, events or other sessions.
        if let Some(base) = self.baselines.get(&key).filter(|base| {
            base.acknowledged
                && base.snapshot.metadata.source == target.metadata.source
                && base.snapshot.metadata.generation == target.metadata.generation
        }) && let Ok(delta) = wire::SnapshotPayload::between(&base.snapshot, target)
            && postcard::experimental::serialized_size(&delta)?
                < postcard::experimental::serialized_size(&payload)?
        {
            payload = delta;
        }
        self.baselines.insert(
            key,
            Baseline {
                snapshot: target.clone(),
                acknowledged: false,
            },
        );
        Ok(wire::SubscriptionItem::Snapshot(wire::SnapshotFrame {
            cursor,
            metadata: target.metadata.clone(),
            payload,
        }))
    }

    fn acknowledge(&mut self, ack: &wire::SnapshotAck) {
        let key = (
            ack.metadata.source.session.clone(),
            ack.metadata.source.topic.clone(),
        );
        if let Some(baseline) = self.baselines.get_mut(&key)
            && baseline.snapshot.metadata == ack.metadata
            && baseline.snapshot.payload.hash() == ack.hash
        {
            baseline.acknowledged = true;
        }
    }
}

pub(super) async fn serve(
    mut subscription: Subscription,
    send: &mut SendStream,
    recv: &mut RecvStream,
    topics: BTreeSet<wire::TopicRef>,
    message_bytes: u32,
) -> anyhow::Result<()> {
    let mut read_acks = !topics.is_empty();
    let mut delivery = Delivery {
        topics,
        baselines: BTreeMap::new(),
    };
    loop {
        // Keep a partial ACK read alive while publications win the select.
        let received =
            recv.read_length_prefixed::<wire::SnapshotAck>(ACK_BYTES.min(message_bytes as usize));
        tokio::pin!(received);
        loop {
            tokio::select! {
                result = &mut received, if read_acks => {
                    if let Ok(ack) = result {
                        delivery.acknowledge(&ack);
                    } else {
                        read_acks = false;
                        delivery.baselines.clear();
                        delivery.topics.clear();
                    }
                    break;
                }
                _ = send.stopped() => return Ok(()),
                item = subscription.next() => {
                    let item = delivery.encode(item.context("subscription ended without Closed")?)?;
                    let terminal = matches!(item, wire::SubscriptionItem::Closed(_));
                    super::requests::write(send, &Ok::<_, wire::RequestError>(item), message_bytes).await?;
                    if terminal {
                        return Ok(());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support as fixture;
    use wire::SnapshotPayload::{Delta, Full};

    fn data(sequence: u64, value: &serde_json::Value) -> anyhow::Result<wire::DataEnvelope> {
        Ok(wire::DataEnvelope {
            metadata: wire::EnvelopeMetadata {
                source: wire::TopicSource {
                    session: wire::SessionRef {
                        run_id: "run".into(),
                        session_id: "session".into(),
                    },
                    game_id: "fixture".into(),
                    topic: fixture::topic(0),
                },
                generation: 1,
                sequence,
            },
            payload: wire::JsonPayload::from_value(value)?,
        })
    }

    fn frame(data: &wire::DataEnvelope) -> wire::SnapshotFrame {
        wire::SnapshotFrame {
            cursor: Some(wire::ServiceCursor {
                run_id: data.metadata.source.session.run_id.clone(),
                sequence: data.metadata.sequence,
            }),
            metadata: data.metadata.clone(),
            payload: wire::SnapshotPayload::Full(data.payload.clone()),
        }
    }

    fn send(
        delivery: &mut Delivery,
        data: &wire::DataEnvelope,
    ) -> anyhow::Result<wire::SnapshotFrame> {
        let item = frame(data).reconstruct(None).map_err(anyhow::Error::msg)?;
        let wire::SubscriptionItem::Snapshot(frame) = delivery.encode(item)? else {
            anyhow::bail!("missing snapshot frame");
        };
        Ok(frame)
    }

    fn ack(data: &wire::DataEnvelope) -> wire::SnapshotAck {
        wire::SnapshotAck {
            metadata: data.metadata.clone(),
            hash: data.payload.hash(),
        }
    }

    #[test]
    fn deltas_require_applied_baselines_and_survive_resets_and_overtaking_updates()
    -> anyhow::Result<()> {
        let mut delivery = Delivery {
            topics: BTreeSet::from([fixture::topic(0)]),
            baselines: BTreeMap::new(),
        };
        let unchanged = "unchanged".repeat(256);
        let initial = data(
            1,
            &serde_json::json!({"unchanged": unchanged, "items": ["a", "b"], "quantity": u64::MAX.to_string()}),
        )?;
        let overtaking = data(
            2,
            &serde_json::json!({"unchanged": unchanged, "items": ["b", "c", "d"], "quantity": "7"}),
        )?;
        let changed = data(
            3,
            &serde_json::json!({"unchanged": unchanged, "items": ["b"], "quantity": u64::MAX.to_string()}),
        )?;
        assert!(matches!(send(&mut delivery, &initial)?.payload, Full(_)));
        let mut wrong = ack(&initial);
        wrong.hash[0] ^= 1;
        delivery.acknowledge(&wrong);
        assert!(matches!(send(&mut delivery, &overtaking)?.payload, Full(_)));
        delivery.acknowledge(&ack(&initial)); // Superseded ACK cannot confirm the newer snapshot.
        assert!(delivery.baselines.values().all(|base| !base.acknowledged));
        delivery.acknowledge(&ack(&overtaking));
        let delta = send(&mut delivery, &changed)?;
        assert!(matches!(delta.payload, Delta { .. }));
        assert!(
            postcard::experimental::serialized_size(&delta)?
                < postcard::experimental::serialized_size(&frame(&changed))?
        );
        assert_eq!(
            delta.clone().reconstruct(Some(&overtaking)),
            frame(&changed).reconstruct(None)
        );
        assert!(delta.clone().reconstruct(None).is_err());
        assert!(delta.clone().reconstruct(Some(&initial)).is_err());
        let mut corrupt = delta;
        if let Delta { hash, .. } = &mut corrupt.payload {
            hash[0] ^= 1;
        }
        assert!(corrupt.reconstruct(Some(&overtaking)).is_err());

        let small_base = data(4, &serde_json::json!(0))?;
        assert!(matches!(send(&mut delivery, &small_base)?.payload, Full(_)));
        delivery.acknowledge(&ack(&changed));
        delivery.acknowledge(&ack(&small_base));
        let small_target = data(5, &serde_json::json!(1))?;
        assert!(matches!(
            send(&mut delivery, &small_target)?.payload,
            Full(_)
        )); // Patch is larger.

        let reset = wire::SubscriptionItem::Update(wire::UpdateEnvelope {
            cursor: wire::ServiceCursor {
                run_id: "run".into(),
                sequence: 6,
            },
            update: wire::SubscriptionUpdate::TopicReset {
                source: small_target.metadata.source.clone(),
                generation: 2,
                reason: wire::ResetReason::SourceChanged,
            },
        });
        assert_eq!(delivery.encode(reset.clone())?, reset);
        delivery.acknowledge(&ack(&small_target));
        assert!(delivery.baselines.is_empty());
        let mut fresh = changed;
        fresh.metadata.generation = 2;
        fresh.metadata.sequence = 7;
        assert!(matches!(send(&mut delivery, &fresh)?.payload, Full(_)));
        let unavailable = wire::SubscriptionItem::Update(wire::UpdateEnvelope {
            cursor: wire::ServiceCursor {
                run_id: "run".into(),
                sequence: 8,
            },
            update: wire::SubscriptionUpdate::TopicChanged(wire::TopicSnapshot {
                source: fresh.metadata.source.clone(),
                generation: 2,
                snapshot: None,
                health: wire::CapabilityHealth::Unavailable {
                    reason: wire::UnavailableReason::TargetNotReady,
                },
            }),
        });
        delivery.encode(unavailable)?;
        delivery.acknowledge(&ack(&fresh));
        assert!(delivery.baselines.is_empty());
        Ok(())
    }
}
