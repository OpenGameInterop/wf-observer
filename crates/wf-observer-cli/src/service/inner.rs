//! Locked state transitions; no native calls, sleeps, or transport sends.

use super::{catalog, subscriptions::Subscriber};
use crate::runtime;
use protocol::v1 as wire;
use std::collections::BTreeMap;
use tokio::time::Instant;

pub(super) struct Inner {
    pub catalog: wire::Catalog,
    pub message_bytes: u32,
    pub run_id: String,
    pub sequence: u64,
    pub runtime: runtime::HostStatus,
    pub sessions: BTreeMap<String, Session>,
    pub subscribers: BTreeMap<u64, Subscriber>,
    pub next_subscription: u64,
    pub revision: u64,
    pub stopped: bool,
}

pub(super) struct Session {
    pub info: wire::SessionInfo,
    pub topics: BTreeMap<wire::TopicRef, Topic>,
}

#[derive(Clone)]
pub(super) struct Topic {
    pub state: wire::TopicSnapshot,
    pub demanded: bool,
    pub epoch: u64,
    pub deadline: Option<Instant>,
}

impl Inner {
    pub fn cursor(&self) -> wire::ServiceCursor {
        wire::ServiceCursor {
            run_id: self.run_id.clone(),
            sequence: self.sequence,
        }
    }

    pub fn ensure_sequences(&mut self, count: usize) -> bool {
        if self.stopped {
            return false;
        }
        if u64::try_from(count)
            .ok()
            .and_then(|n| self.sequence.checked_add(n))
            .is_none()
        {
            self.close(&wire::SubscriptionEnd::ResyncRequired {
                reason: wire::ResyncReason::SequenceExhausted,
            });
            return false;
        }
        true
    }

    pub fn advance(&mut self) -> bool {
        if self.stopped {
            return false;
        }
        if let Some(next) = self.sequence.checked_add(1) {
            self.sequence = next;
            true
        } else {
            self.close(&wire::SubscriptionEnd::ResyncRequired {
                reason: wire::ResyncReason::SequenceExhausted,
            });
            false
        }
    }

    pub fn emit(&mut self, update: &wire::SubscriptionUpdate) {
        if !self.advance() {
            return;
        }
        let item = wire::SubscriptionItem::Update(wire::UpdateEnvelope {
            cursor: self.cursor(),
            update: update.clone(),
        });
        let ended = if let wire::SubscriptionUpdate::SessionEnded { session, .. } = update {
            self.sessions
                .get(&session.session_id)
                .map(|s| s.info.clone())
        } else {
            None
        };
        for subscriber in self.subscribers.values() {
            let matches = ended.as_ref().map_or_else(
                || subscriber.matches_update(update),
                |info| subscriber.matches_info(info),
            );
            if subscriber.active && matches {
                subscriber.queue.offer(item.clone());
            }
        }
    }

    pub fn close(&mut self, end: &wire::SubscriptionEnd) {
        self.stopped = true;
        for (_, subscriber) in std::mem::take(&mut self.subscribers) {
            subscriber.queue.finish(end.clone(), true);
        }
        for session in self.sessions.values_mut() {
            for topic in session.topics.values_mut() {
                topic.demanded = false;
                topic.state.snapshot = None;
                topic.state.health = wire::CapabilityHealth::Idle;
                topic.deadline = None;
            }
        }
        self.revision = self.revision.saturating_add(1);
    }

    /// Reconcile authoritative subscriptions, including overflow-induced removal.
    pub fn settle(&mut self, now: Instant) {
        loop {
            self.subscribers
                .retain(|_, subscriber| !subscriber.queue.ended());
            let mut changes = Vec::new();
            for session in self.sessions.values_mut() {
                for (key, topic) in &mut session.topics {
                    let demanded = self
                        .subscribers
                        .values()
                        .any(|s| s.matches(&session.info.session, key));
                    if demanded == topic.demanded {
                        continue;
                    }
                    topic.demanded = demanded;
                    let Some(epoch) = topic.epoch.checked_add(1) else {
                        self.close(&wire::SubscriptionEnd::ResyncRequired {
                            reason: wire::ResyncReason::SequenceExhausted,
                        });
                        return;
                    };
                    topic.epoch = epoch;
                    topic.state.snapshot = None;
                    topic.deadline = demanded.then_some(now + super::state::POLL_GRACE);
                    if demanded {
                        let Some(generation) = topic.state.generation.checked_add(1) else {
                            self.close(&wire::SubscriptionEnd::ResyncRequired {
                                reason: wire::ResyncReason::SequenceExhausted,
                            });
                            return;
                        };
                        topic.state.generation = generation;
                        topic.state.health = wire::CapabilityHealth::Initializing;
                        changes.push(wire::SubscriptionUpdate::TopicReset {
                            source: topic.state.source.clone(),
                            generation,
                            reason: wire::ResetReason::DemandResumed,
                        });
                    } else {
                        topic.state.health = wire::CapabilityHealth::Idle;
                        changes.push(wire::SubscriptionUpdate::TopicChanged(topic.state.clone()));
                    }
                }
            }
            if !changes.is_empty() {
                self.revision = self.revision.saturating_add(1);
            }
            for change in changes {
                self.emit(&change);
            }
            if !self.subscribers.values().any(|s| s.queue.ended()) {
                break;
            }
        }
    }

    pub fn capability(
        &self,
        key: &wire::TopicRef,
    ) -> Result<&wire::CapabilityDescriptor, wire::RequestError> {
        if !catalog::identifier(&key.provider_id)
            || !catalog::identifier(&key.topic)
            || key.schema_version == 0
        {
            return Err(catalog::invalid("invalid topic selector"));
        }
        let provider = self
            .catalog
            .providers
            .iter()
            .find(|p| p.id == key.provider_id)
            .ok_or_else(|| wire::RequestError::UnknownProvider {
                provider_id: key.provider_id.clone(),
            })?;
        let supported: Vec<_> = provider
            .capabilities
            .iter()
            .filter(|c| c.topic == key.topic)
            .collect();
        if supported.is_empty() {
            return Err(wire::RequestError::UnknownTopic {
                provider_id: key.provider_id.clone(),
                topic: key.topic.clone(),
            });
        }
        supported
            .iter()
            .find(|c| c.schema_version == key.schema_version)
            .copied()
            .ok_or_else(|| wire::RequestError::UnsupportedSchema {
                requested: key.clone(),
                supported: supported.iter().map(|c| c.schema_version).collect(),
            })
    }

    pub fn session(&self, key: &wire::SessionRef) -> Result<&Session, wire::RequestError> {
        if !catalog::identifier(&key.run_id) || !catalog::identifier(&key.session_id) {
            return Err(catalog::invalid("invalid session identity"));
        }
        if key.run_id != self.run_id {
            return Err(wire::RequestError::ServiceRestarted {
                run_id: self.run_id.clone(),
            });
        }
        self.sessions
            .get(&key.session_id)
            .ok_or_else(|| wire::RequestError::UnknownSession {
                session_id: key.session_id.clone(),
            })
    }

    pub fn expire(&mut self, now: Instant) {
        let mut changes = Vec::new();
        for session in self.sessions.values_mut() {
            for topic in session.topics.values_mut() {
                if topic.deadline.is_some_and(|deadline| now >= deadline) {
                    topic.deadline = None;
                    let Some(epoch) = topic.epoch.checked_add(1) else {
                        self.close(&wire::SubscriptionEnd::ResyncRequired {
                            reason: wire::ResyncReason::SequenceExhausted,
                        });
                        return;
                    };
                    topic.epoch = epoch;
                    topic.state.snapshot = None;
                    topic.state.health = wire::CapabilityHealth::Unavailable {
                        reason: wire::UnavailableReason::ProviderFailed {
                            message: "sampling deadline exceeded".into(),
                        },
                    };
                    changes.push(wire::SubscriptionUpdate::TopicChanged(topic.state.clone()));
                }
            }
        }
        if !changes.is_empty() {
            self.revision = self.revision.saturating_add(1);
        }
        for change in changes {
            self.emit(&change);
        }
        self.settle(now);
    }

    pub fn sync_runtime(&mut self, status: runtime::HostStatus) -> Result<(), wire::RequestError> {
        if self.stopped || self.runtime == status {
            return Ok(());
        }
        let incoming: BTreeMap<_, _> = status
            .targets
            .iter()
            .filter_map(|target| {
                let runtime::Activity::Observing { session_id } = &target.activity else {
                    return None;
                };
                Some((
                    session_id.clone(),
                    wire::SessionInfo {
                        session: wire::SessionRef {
                            run_id: self.run_id.clone(),
                            session_id: session_id.clone(),
                        },
                        provider_id: target.target.provider_id.clone(),
                        game_id: target.target.game_id.clone(),
                        target: wire::TargetProcess {
                            pid: target.target.process.pid,
                            executable: target.target.executable.clone(),
                        },
                        game_build: self
                            .sessions
                            .get(session_id)
                            .and_then(|s| s.info.game_build.clone()),
                    },
                ))
            })
            .collect();
        let ended: Vec<_> = self
            .sessions
            .keys()
            .filter(|id| !incoming.contains_key(*id))
            .cloned()
            .collect();
        for id in ended {
            if let Some(info) = self.sessions.get(&id).map(|s| s.info.clone()) {
                let retrying = self.runtime.targets.iter().any(|old| {
                    matches!(&old.activity, runtime::Activity::Observing { session_id } if session_id == &id)
                        && status.targets.iter().any(|new| {
                            new.target == old.target
                                && matches!(new.activity, runtime::Activity::Retrying { .. })
                        })
                });
                self.emit(&wire::SubscriptionUpdate::SessionEnded {
                    session: info.session.clone(),
                    reason: if retrying {
                        wire::SessionEndReason::ProviderFailed
                    } else {
                        wire::SessionEndReason::TargetExited
                    },
                });
                self.sessions.remove(&id);
                self.subscribers.retain(|_, subscriber| {
                    if subscriber.selection.sessions
                        == (wire::SessionSelector::Session {
                            reference: info.session.clone(),
                        })
                    {
                        subscriber
                            .queue
                            .finish(wire::SubscriptionEnd::SessionEnded, false);
                        false
                    } else {
                        true
                    }
                });
            }
        }
        for (id, info) in incoming {
            if self.sessions.contains_key(&id) {
                continue;
            }
            self.add_session(&info)?;
        }
        self.runtime = status;
        self.advance();
        self.settle(Instant::now());
        Ok(())
    }

    fn add_session(&mut self, info: &wire::SessionInfo) -> Result<(), wire::RequestError> {
        let provider = self
            .catalog
            .providers
            .iter()
            .find(|p| p.id == info.provider_id)
            .ok_or_else(|| catalog::invalid("unregistered session provider"))?;
        if !self.ensure_sequences(1 + provider.capabilities.len()) {
            return Ok(());
        }
        let provider = self
            .catalog
            .providers
            .iter()
            .find(|p| p.id == info.provider_id)
            .ok_or_else(|| catalog::invalid("unregistered session provider"))?;
        let topics = provider
            .capabilities
            .iter()
            .map(|cap| {
                let key = wire::TopicRef {
                    provider_id: provider.id.clone(),
                    topic: cap.topic.clone(),
                    schema_version: cap.schema_version,
                };
                (
                    key.clone(),
                    Topic {
                        state: wire::TopicSnapshot {
                            source: wire::TopicSource {
                                session: info.session.clone(),
                                game_id: info.game_id.clone(),
                                topic: key,
                            },
                            generation: 1,
                            health: wire::CapabilityHealth::Idle,
                            snapshot: None,
                        },
                        demanded: false,
                        epoch: 0,
                        deadline: None,
                    },
                )
            })
            .collect();
        self.sessions.insert(
            info.session.session_id.clone(),
            Session {
                info: info.clone(),
                topics,
            },
        );
        self.emit(&wire::SubscriptionUpdate::SessionStarted(info.clone()));
        let topics: Vec<_> = self.sessions[&info.session.session_id]
            .topics
            .values()
            .map(|t| t.state.clone())
            .collect();
        for topic in topics {
            self.emit(&wire::SubscriptionUpdate::TopicChanged(topic));
        }
        Ok(())
    }
}
