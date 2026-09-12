//! Staged provider output and generation-fenced atomic publication.

use super::{
    RETAINED_SNAPSHOT_BYTES,
    batch::{Operation, PollBatch},
    catalog,
    inner::{Inner, Session, Topic},
    state::ServiceState,
};
use protocol::v1 as wire;
use std::collections::BTreeMap;
use tokio::time::Instant;

#[derive(Clone, Debug, ..Eq)]
pub(crate) struct PollTicket {
    pub(crate) session_id: String,
    pub(crate) topics: Vec<Demand>,
}

#[derive(Clone, Debug, ..Eq)]
pub(crate) struct Demand {
    pub(crate) key: wire::TopicRef,
    generation: u64,
    epoch: u64,
}

impl PollTicket {
    /// Freshness epochs fence in-flight output without changing provider demand.
    pub(crate) fn same_demand(&self, previous: &Self) -> bool {
        self.session_id == previous.session_id
            && self.topics.len() == previous.topics.len()
            && self
                .topics
                .iter()
                .zip(&previous.topics)
                .all(|(a, b)| a.key == b.key && a.generation == b.generation)
    }

    pub(crate) fn same_generation(&self, previous: &Self, key: &wire::TopicRef) -> bool {
        self.topics
            .iter()
            .find(|d| &d.key == key)
            .zip(previous.topics.iter().find(|d| &d.key == key))
            .is_some_and(|(a, b)| a.generation == b.generation)
    }

    /// Reflect only resets from our accepted batch, not concurrent demand changes.
    pub(crate) fn rebase_resets(&mut self, resets: &[wire::TopicRef]) {
        for demand in &mut self.topics {
            if resets.contains(&demand.key) {
                demand.generation = demand.generation.saturating_add(1);
                demand.epoch = demand.epoch.saturating_add(1);
            }
        }
    }

    fn current(inner: &Inner, session_id: &str) -> Option<Self> {
        if inner.stopped {
            return None;
        }
        let session = inner.sessions.get(session_id)?;
        Some(Self {
            session_id: session_id.into(),
            topics: session
                .topics
                .iter()
                .filter(|(_, t)| t.demanded)
                .map(|(key, topic)| Demand {
                    key: key.clone(),
                    generation: topic.state.generation,
                    epoch: topic.epoch,
                })
                .collect(),
        })
    }
}

impl ServiceState {
    pub(crate) fn ticket(&self, session_id: &str) -> Option<PollTicket> {
        PollTicket::current(&self.shared.inner.lock(), session_id)
    }

    pub(crate) fn fail_poll(&self, ticket: &PollTicket) {
        let _ = self.discard_poll(ticket, &[], true);
    }

    /// Preserves verified source resets even when the poll's data cannot commit.
    /// Rebases only its own resets for retry bookkeeping; unrelated demand changes stay urgent.
    pub(crate) fn discard_poll(
        &self,
        ticket: &PollTicket,
        resets: &[wire::TopicRef],
        failed: bool,
    ) -> Option<PollTicket> {
        let mut inner = self.shared.inner.lock();
        if inner.stopped {
            return None;
        }
        let session = inner.sessions.get(&ticket.session_id)?;
        let affected: BTreeMap<_, _> = ticket
            .topics
            .iter()
            .filter_map(|demand| {
                let topic = session.topics.get(&demand.key)?;
                if !topic.demanded || topic.state.generation != demand.generation {
                    return None;
                }
                let reset = resets.contains(&demand.key);
                // Expiry changes the epoch without establishing a new source generation.
                (reset || (failed && topic.epoch == demand.epoch)).then_some((&demand.key, reset))
            })
            .collect();
        if affected.iter().any(|(key, reset)| {
            let topic = &session.topics[*key];
            *reset && (topic.state.generation == u64::MAX || topic.epoch == u64::MAX)
        }) {
            inner.close(&wire::SubscriptionEnd::ResyncRequired {
                reason: wire::ResyncReason::SequenceExhausted,
            });
            self.shared.changed.notify_all();
            return None;
        }
        let updates = affected.len() + affected.values().filter(|&&reset| reset).count();
        if !inner.ensure_sequences(updates) {
            self.shared.changed.notify_all();
            return None;
        }
        let mut retry = ticket.clone();
        let mut changes = Vec::with_capacity(updates);
        if let Some(session) = inner.sessions.get_mut(&ticket.session_id) {
            for (key, reset) in affected {
                if let Some(topic) = session.topics.get_mut(key) {
                    if reset {
                        topic.state.generation += 1;
                        topic.epoch += 1;
                        if let Some(demand) = retry.topics.iter_mut().find(|d| &d.key == key) {
                            demand.generation = topic.state.generation;
                            demand.epoch = topic.epoch;
                        }
                        changes.push(wire::SubscriptionUpdate::TopicReset {
                            source: topic.state.source.clone(),
                            generation: topic.state.generation,
                            reason: wire::ResetReason::SourceChanged,
                        });
                    }
                    topic.state.health = wire::CapabilityHealth::Unavailable {
                        reason: wire::UnavailableReason::ProviderFailed {
                            message: "provider acquisition failed; see local service logs".into(),
                        },
                    };
                    topic.state.snapshot = None;
                    topic.deadline = None;
                    changes.push(wire::SubscriptionUpdate::TopicChanged(topic.state.clone()));
                }
            }
        }
        for change in changes {
            inner.emit(&change);
        }
        inner.settle(Instant::now());
        self.shared.changed.notify_all();
        let current = PollTicket::current(&inner, &ticket.session_id)?;
        current.same_demand(&retry).then_some(current)
    }

    /// Returns false when demand/generation changed during the provider call.
    pub(crate) fn commit_poll(
        &self,
        ticket: &PollTicket,
        batch: PollBatch,
        deadline: Instant,
    ) -> Result<bool, wire::RequestError> {
        let pending = batch.pending.into_inner();
        if pending.failed {
            return Err(catalog::invalid("provider ignored rejected publication"));
        }
        let mut inner = self.shared.inner.lock();
        if inner.stopped {
            return Ok(false);
        }
        let Some(session) = inner.sessions.get(&ticket.session_id) else {
            return Ok(false);
        };
        if ticket.topics.iter().any(|d| {
            session.topics.get(&d.key).is_none_or(|t| {
                !t.demanded || t.state.generation != d.generation || t.epoch != d.epoch
            })
        }) {
            return Ok(false);
        }
        if pending.ops.iter().any(|op| matches!(op, Operation::Reset(key)
            if session.topics.get(key).is_some_and(|t| t.state.generation == u64::MAX || t.epoch == u64::MAX))) {
            inner.close(&wire::SubscriptionEnd::ResyncRequired { reason: wire::ResyncReason::SequenceExhausted });
            self.shared.changed.notify_all();
            return Ok(false);
        }
        let staged = StagedPublication::prepare(session, ticket, pending.ops, deadline)?;
        staged.validate(&inner, session)?;
        let metadata = (pending.game_build != session.info.game_build).then(|| {
            let mut info = session.info.clone();
            info.game_build = pending.game_build;
            info
        });
        if let Some(info) = &metadata {
            update_fits(
                &inner,
                wire::SubscriptionUpdate::SessionChanged(info.clone()),
            )?;
        }
        // Reserve an upper bound before mutating anything, so exhaustion cannot
        // close the service halfway through a transaction and then restore data.
        if !inner.ensure_sequences(
            staged.resets.len()
                + staged.topics.len()
                + staged.events.len()
                + usize::from(metadata.is_some()),
        ) {
            return Ok(false);
        }
        if let Some(info) = metadata {
            if let Some(session) = inner.sessions.get_mut(&ticket.session_id) {
                session.info = info.clone();
            }
            inner.emit(&wire::SubscriptionUpdate::SessionChanged(info));
        }
        staged.commit(&mut inner, ticket);
        inner.settle(Instant::now());
        self.shared.changed.notify_all();
        Ok(true)
    }
}

struct StagedPublication {
    topics: BTreeMap<wire::TopicRef, Topic>,
    resets: Vec<wire::TopicRef>,
    events: Vec<(wire::TopicRef, wire::JsonPayload)>,
}

impl StagedPublication {
    fn prepare(
        session: &Session,
        ticket: &PollTicket,
        operations: Vec<Operation>,
        deadline: Instant,
    ) -> Result<Self, wire::RequestError> {
        let mut staged = BTreeMap::new();
        let mut resets = Vec::new();
        let mut events = Vec::new();
        for operation in operations {
            let key = match &operation {
                Operation::Reset(k)
                | Operation::Snapshot(k, _)
                | Operation::Event(k, _)
                | Operation::Health(k, _) => k,
            };
            if !ticket.topics.iter().any(|d| &d.key == key) {
                return Err(catalog::invalid("publication for undemanded topic"));
            }
            let current = session
                .topics
                .get(key)
                .ok_or_else(|| catalog::invalid("publication for undemanded topic"))?;
            let topic = staged.entry(key.clone()).or_insert_with(|| current.clone());
            match operation {
                Operation::Reset(key) => {
                    topic.state.generation = topic
                        .state
                        .generation
                        .checked_add(1)
                        .ok_or_else(|| catalog::invalid("topic generation exhausted"))?;
                    topic.epoch = topic
                        .epoch
                        .checked_add(1)
                        .ok_or_else(|| catalog::invalid("topic epoch exhausted"))?;
                    topic.state.snapshot = None;
                    topic.state.health = wire::CapabilityHealth::Initializing;
                    topic.deadline = Some(deadline);
                    events.retain(|(old, _)| old != &key);
                    resets.push(key);
                }
                Operation::Snapshot(_, payload) => {
                    topic.state.health = wire::CapabilityHealth::Available;
                    topic.deadline = Some(deadline);
                    topic.state.snapshot = Some(wire::DataEnvelope {
                        metadata: wire::EnvelopeMetadata {
                            source: topic.state.source.clone(),
                            generation: topic.state.generation,
                            sequence: 0,
                        },
                        payload,
                    });
                }
                Operation::Event(key, payload) => events.push((key, payload)),
                Operation::Health(_, health) => {
                    topic.deadline =
                        matches!(health, wire::CapabilityHealth::Available).then_some(deadline);
                    if !matches!(health, wire::CapabilityHealth::Available) {
                        topic.state.snapshot = None;
                    }
                    topic.state.health = health;
                }
            }
        }

        Ok(Self {
            topics: staged,
            resets,
            events,
        })
    }

    fn validate(&self, inner: &Inner, session: &Session) -> Result<(), wire::RequestError> {
        for (key, topic) in &self.topics {
            let cap = inner.capability(key)?;
            if matches!(topic.state.health, wire::CapabilityHealth::Available)
                && cap.snapshots
                && topic.state.snapshot.is_none()
            {
                return Err(catalog::invalid(
                    "available snapshot capability has no snapshot",
                ));
            }
            // TopicChanged also covers the smaller bootstrap and snapshot replies.
            let mut state = topic.state.clone();
            if let Some(snapshot) = &mut state.snapshot {
                snapshot.metadata.sequence = u64::MAX;
            }
            update_fits(inner, wire::SubscriptionUpdate::TopicChanged(state))?;
        }
        for (key, payload) in &self.events {
            let state = &self.topics[key].state;
            if !matches!(state.health, wire::CapabilityHealth::Available) {
                return Err(catalog::invalid(
                    "event published by unavailable capability",
                ));
            }
            update_fits(
                inner,
                wire::SubscriptionUpdate::Event(wire::EventEnvelope {
                    metadata: wire::EnvelopeMetadata {
                        source: state.source.clone(),
                        generation: state.generation,
                        sequence: u64::MAX,
                    },
                    payload: payload.clone(),
                }),
            )?;
        }
        let retained: usize = inner
            .sessions
            .values()
            .flat_map(|s| s.topics.values())
            .filter_map(|t| t.state.snapshot.as_ref())
            .map(|s| s.payload.as_str().len())
            .sum();
        let old_bytes: usize = self
            .topics
            .keys()
            .filter_map(|k| session.topics[k].state.snapshot.as_ref())
            .map(|s| s.payload.as_str().len())
            .sum();
        let new_bytes: usize = self
            .topics
            .values()
            .filter_map(|t| t.state.snapshot.as_ref())
            .map(|s| s.payload.as_str().len())
            .sum();
        if retained - old_bytes + new_bytes > RETAINED_SNAPSHOT_BYTES {
            return Err(catalog::limit(wire::Resource::RetainedSnapshotBytes));
        }

        Ok(())
    }

    fn commit(self, inner: &mut Inner, ticket: &PollTicket) {
        let Self {
            topics: staged,
            resets,
            events,
        } = self;
        for key in &resets {
            let topic = &staged[key];
            inner.emit(&wire::SubscriptionUpdate::TopicReset {
                source: topic.state.source.clone(),
                generation: topic.state.generation,
                reason: wire::ResetReason::SourceChanged,
            });
        }
        for (key, mut topic) in staged {
            let old = &inner.sessions[&ticket.session_id].topics[&key].state;
            let changed = old.generation != topic.state.generation
                || old.health != topic.state.health
                || old.snapshot.as_ref().map(|s| &s.payload)
                    != topic.state.snapshot.as_ref().map(|s| &s.payload);
            if changed {
                if let Some(snapshot) = &mut topic.state.snapshot {
                    snapshot.metadata.sequence = inner.sequence + 1;
                }
            } else {
                topic.state.snapshot.clone_from(&old.snapshot);
            }
            let state = topic.state.clone();
            if let Some(session) = inner.sessions.get_mut(&ticket.session_id) {
                session.topics.insert(key, topic);
            }
            if changed {
                inner.emit(&wire::SubscriptionUpdate::TopicChanged(state));
            }
        }
        for (key, payload) in events {
            let topic = &inner.sessions[&ticket.session_id].topics[&key];
            let event = wire::EventEnvelope {
                metadata: wire::EnvelopeMetadata {
                    source: topic.state.source.clone(),
                    generation: topic.state.generation,
                    sequence: inner.sequence + 1,
                },
                payload,
            };
            inner.emit(&wire::SubscriptionUpdate::Event(event));
        }
    }
}

fn update_fits(inner: &Inner, update: wire::SubscriptionUpdate) -> Result<(), wire::RequestError> {
    // A cached snapshot must still fit when the service sequence grows.
    let frame = wire::SubscriptionItem::Update(wire::UpdateEnvelope {
        cursor: wire::ServiceCursor {
            run_id: inner.run_id.clone(),
            sequence: u64::MAX,
        },
        update,
    });
    catalog::message_fits(&Ok::<_, wire::RequestError>(frame), inner.message_bytes)
}
