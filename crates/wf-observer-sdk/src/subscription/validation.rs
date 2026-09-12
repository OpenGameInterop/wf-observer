//! Tracks ordering and identities, not payload caches or replay history.

use crate::raw::ClientError;
use protocol::v1 as wire;
use std::collections::BTreeMap;

const MAX_ACTIVE_METADATA_BYTES: usize = 8 * 1024 * 1024;

pub(super) struct Validation {
    selection: wire::Subscribe,
    checkpoint: Option<wire::ServiceCursor>,
    ready: bool,
    sequence: u64,
    sessions: BTreeMap<String, SeenSession>,
}

struct SeenSession {
    info: wire::SessionInfo,
    topics: BTreeMap<wire::TopicRef, u64>,
}

impl Validation {
    pub(super) fn new(mut selection: wire::Subscribe) -> Self {
        selection.topics.sort();
        selection.topics.dedup();
        Self {
            selection,
            checkpoint: None,
            ready: false,
            sequence: 0,
            sessions: BTreeMap::new(),
        }
    }

    pub(super) fn begun(&self) -> bool {
        self.checkpoint.is_some()
    }

    pub(super) fn accept(&mut self, item: &wire::SubscriptionItem) -> Result<(), ClientError> {
        match item {
            wire::SubscriptionItem::Begin(cursor) => {
                check(
                    !self.begun() && identifier(&cursor.run_id),
                    "invalid or duplicate Begin",
                )?;
                self.checkpoint = Some(cursor.clone());
                self.sequence = cursor.sequence;
            }
            wire::SubscriptionItem::Session(info) => {
                check(self.begun() && !self.ready, "Session outside bootstrap")?;
                self.add_session(info)?;
            }
            wire::SubscriptionItem::Topic(topic) => {
                check(self.begun() && !self.ready, "Topic outside bootstrap")?;
                self.topic(topic, true)?;
            }
            wire::SubscriptionItem::Snapshot(_) => {
                return Err(ClientError::protocol("snapshot was not reconstructed"));
            }
            wire::SubscriptionItem::Ready(cursor) => {
                check(
                    !self.ready && self.checkpoint.as_ref() == Some(cursor),
                    "invalid Ready checkpoint",
                )?;
                for session in self.sessions.values() {
                    let expected = self
                        .selection
                        .topics
                        .iter()
                        .filter(|t| t.provider_id == session.info.provider_id)
                        .count();
                    check(
                        session.topics.len() == expected,
                        "bootstrap omitted a selected topic",
                    )?;
                }
                self.ready = true;
            }
            wire::SubscriptionItem::Update(update) => {
                check(
                    self.ready
                        && self
                            .checkpoint
                            .as_ref()
                            .is_some_and(|c| c.run_id == update.cursor.run_id)
                        && update.cursor.sequence > self.sequence,
                    "out-of-order update",
                )?;
                self.sequence = update.cursor.sequence;
                self.update(&update.update)?;
            }
            wire::SubscriptionItem::Closed(_) => check(self.begun(), "Closed before Begin")?,
        }
        // This is metadata-only. Enforce a local bound even if a peer invents an
        // unending sequence of active sessions without sending SessionEnded.
        let bytes: usize = self
            .sessions
            .values()
            .map(|session| {
                postcard::experimental::serialized_size(&session.info)
                    .unwrap_or(usize::MAX / 2)
                    .saturating_add(256)
                    .saturating_add(
                        session
                            .topics
                            .keys()
                            .map(|key| key.provider_id.len() + key.topic.len() + 256)
                            .sum::<usize>(),
                    )
            })
            .sum();
        check(
            bytes <= MAX_ACTIVE_METADATA_BYTES,
            "active metadata exceeds byte limit",
        )
    }

    fn selected(&self, info: &wire::SessionInfo) -> bool {
        self.checkpoint
            .as_ref()
            .is_some_and(|c| c.run_id == info.session.run_id)
            && identifier(&info.session.session_id)
            && identifier(&info.provider_id)
            && identifier(&info.game_id)
            && self
                .selection
                .topics
                .iter()
                .any(|t| t.provider_id == info.provider_id)
            && match &self.selection.sessions {
                wire::SessionSelector::All => true,
                wire::SessionSelector::Session { reference } => reference == &info.session,
            }
    }

    fn add_session(&mut self, info: &wire::SessionInfo) -> Result<(), ClientError> {
        check(
            self.selected(info) && !self.sessions.contains_key(&info.session.session_id),
            "unexpected or duplicate session",
        )?;
        self.sessions.insert(
            info.session.session_id.clone(),
            SeenSession {
                info: info.clone(),
                topics: BTreeMap::new(),
            },
        );
        Ok(())
    }

    fn source(&self, source: &wire::TopicSource) -> Result<&SeenSession, ClientError> {
        let session = self
            .sessions
            .get(&source.session.session_id)
            .ok_or_else(|| ClientError::protocol("topic precedes its session"))?;
        check(
            session.info.session == source.session
                && session.info.game_id == source.game_id
                && session.info.provider_id == source.topic.provider_id
                && self.selection.topics.contains(&source.topic),
            "unexpected topic source",
        )?;
        Ok(session)
    }

    fn topic(&mut self, topic: &wire::TopicSnapshot, bootstrap: bool) -> Result<(), ClientError> {
        let session = self.source(&topic.source)?;
        let old = session.topics.get(&topic.source.topic);
        check(
            topic.generation > 0
                && if bootstrap {
                    old.is_none()
                } else {
                    old.is_none_or(|g| *g == topic.generation)
                },
            "duplicate topic or generation changed without reset",
        )?;
        if let Some(data) = &topic.snapshot {
            check(
                matches!(topic.health, wire::CapabilityHealth::Available)
                    && data.metadata.source == topic.source
                    && data.metadata.generation == topic.generation
                    && data.metadata.sequence > 0
                    && data.metadata.sequence <= self.sequence,
                "snapshot does not match its topic state",
            )?;
        }
        self.sessions
            .get_mut(&topic.source.session.session_id)
            .ok_or_else(|| ClientError::protocol("missing session"))?
            .topics
            .insert(topic.source.topic.clone(), topic.generation);
        Ok(())
    }

    fn update(&mut self, update: &wire::SubscriptionUpdate) -> Result<(), ClientError> {
        match update {
            wire::SubscriptionUpdate::SessionStarted(info) => self.add_session(info),
            wire::SubscriptionUpdate::SessionChanged(info) => {
                check(self.selected(info), "unexpected session metadata")?;
                let current = self
                    .sessions
                    .get_mut(&info.session.session_id)
                    .ok_or_else(|| ClientError::protocol("unknown session changed"))?;
                check(
                    current.info.provider_id == info.provider_id
                        && current.info.game_id == info.game_id,
                    "session ownership changed",
                )?;
                current.info.clone_from(info);
                Ok(())
            }
            wire::SubscriptionUpdate::SessionEnded { session, .. } => {
                check(
                    self.sessions
                        .get(&session.session_id)
                        .is_some_and(|s| s.info.session == *session),
                    "unknown session ended",
                )?;
                self.sessions.remove(&session.session_id);
                Ok(())
            }
            wire::SubscriptionUpdate::TopicReset {
                source, generation, ..
            } => {
                check(
                    self.source(source)?
                        .topics
                        .get(&source.topic)
                        .is_some_and(|old| generation > old),
                    "reset does not advance a known generation",
                )?;
                self.sessions
                    .get_mut(&source.session.session_id)
                    .ok_or_else(|| ClientError::protocol("missing reset session"))?
                    .topics
                    .insert(source.topic.clone(), *generation);
                Ok(())
            }
            wire::SubscriptionUpdate::TopicChanged(topic) => self.topic(topic, false),
            wire::SubscriptionUpdate::Event(event) => {
                let meta = &event.metadata;
                check(
                    self.source(&meta.source)?.topics.get(&meta.source.topic)
                        == Some(&meta.generation)
                        && meta.sequence == self.sequence,
                    "event has stale generation or sequence",
                )?;
                Ok(())
            }
        }
    }
}

fn check(condition: bool, message: &str) -> Result<(), ClientError> {
    if !condition {
        return Err(ClientError::protocol(message));
    }
    Ok(())
}

fn identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256
}
