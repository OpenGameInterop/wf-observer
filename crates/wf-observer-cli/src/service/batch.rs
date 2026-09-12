//! Bounded, validated provider output staged outside the service lock.

use protocol::v1 as wire;
use provider_sdk::{CapabilityDescriptor, EventSink, HealthSink, ProviderError, ProviderManifest};
use std::cell::RefCell;

const MAX_BATCH_BYTES: usize = 32 * 1024 * 1024;

pub(crate) struct PollBatch {
    manifest: &'static ProviderManifest,
    pub(super) pending: RefCell<Pending>,
}

#[derive(Default)]
pub(super) struct Pending {
    pub(super) ops: Vec<Operation>,
    resets: Vec<wire::TopicRef>,
    pub(super) game_build: Option<String>,
    bytes: usize,
    pub(super) failed: bool,
}

pub(super) enum Operation {
    Reset(wire::TopicRef),
    Snapshot(wire::TopicRef, wire::JsonPayload),
    Event(wire::TopicRef, wire::JsonPayload),
    Health(wire::TopicRef, wire::CapabilityHealth),
}

pub(crate) struct Events<'a>(&'a PollBatch);
pub(crate) struct Health<'a>(&'a PollBatch);

impl PollBatch {
    pub(crate) fn new(manifest: &'static ProviderManifest) -> Self {
        Self {
            manifest,
            pending: RefCell::new(Pending::default()),
        }
    }
    pub(crate) fn events(&self) -> Events<'_> {
        Events(self)
    }
    pub(crate) fn health(&self) -> Health<'_> {
        Health(self)
    }

    /// Declared source invalidations, retained even if data publication fails.
    pub(crate) fn reset_intents(&self) -> Vec<wire::TopicRef> {
        self.pending.borrow().resets.clone()
    }

    pub(crate) fn game_build(&self, build: Option<&str>) -> Result<(), ProviderError> {
        if build.is_some_and(|value| value.len() > 256) {
            return self.reject();
        }
        self.pending.borrow_mut().game_build = build.map(str::to_owned);
        Ok(())
    }

    fn key(&self, cap: &CapabilityDescriptor) -> Result<wire::TopicRef, ProviderError> {
        if !self.manifest.capabilities.iter().any(|c| {
            c.topic == cap.topic
                && c.schema_version == cap.schema_version
                && c.snapshots == cap.snapshots
                && c.events == cap.events
        }) {
            return self.reject();
        }
        Ok(wire::TopicRef {
            provider_id: self.manifest.id.into(),
            topic: cap.topic.into(),
            schema_version: cap.schema_version,
        })
    }

    fn reject<T>(&self) -> Result<T, ProviderError> {
        self.pending.borrow_mut().failed = true;
        Err(ProviderError::Failed("publication rejected by host".into()))
    }

    fn push(&self, operation: Operation, bytes: usize) -> Result<(), ProviderError> {
        let key = match &operation {
            Operation::Reset(key)
            | Operation::Snapshot(key, _)
            | Operation::Event(key, _)
            | Operation::Health(key, _) => key,
        };
        // Include operation storage and owned identities, even for tiny payloads.
        let bytes = bytes + size_of::<Operation>() + key.provider_id.len() + key.topic.len();
        let mut pending = self.pending.borrow_mut();
        if bytes > MAX_BATCH_BYTES.saturating_sub(pending.bytes) {
            pending.failed = true;
            return Err(ProviderError::Failed(
                "publication batch exceeds staging byte limit".into(),
            ));
        }
        pending.bytes += bytes;
        pending.ops.push(operation);
        Ok(())
    }

    fn payload(value: &serde_json::Value) -> Result<wire::JsonPayload, ProviderError> {
        wire::JsonPayload::from_value(value)
            .map_err(|_| ProviderError::Failed("invalid JSON publication".into()))
    }
}

impl EventSink for Events<'_> {
    fn reset(&mut self, cap: &CapabilityDescriptor) -> Result<(), ProviderError> {
        let key = self.0.key(cap)?;
        let mut pending = self.0.pending.borrow_mut();
        if pending.resets.contains(&key) {
            pending.failed = true;
            return Err(ProviderError::Failed(
                "duplicate topic reset in one poll".into(),
            ));
        }
        // At most one reset per declared capability, independent of the data budget.
        pending.resets.push(key.clone());
        pending.ops.push(Operation::Reset(key));
        Ok(())
    }
    fn snapshot(
        &mut self,
        cap: &CapabilityDescriptor,
        value: &serde_json::Value,
    ) -> Result<(), ProviderError> {
        let key = self.0.key(cap)?;
        if cap.snapshots.is_none() {
            return self.0.reject();
        }
        let payload = PollBatch::payload(value)?;
        let bytes = payload.as_str().len();
        self.0.push(Operation::Snapshot(key, payload), bytes)
    }
    fn event(
        &mut self,
        cap: &CapabilityDescriptor,
        value: &serde_json::Value,
    ) -> Result<(), ProviderError> {
        let key = self.0.key(cap)?;
        if !cap.events {
            return self.0.reject();
        }
        let payload = PollBatch::payload(value)?;
        let bytes = payload.as_str().len();
        self.0.push(Operation::Event(key, payload), bytes)
    }
}

impl HealthSink for Health<'_> {
    fn update(
        &mut self,
        cap: &CapabilityDescriptor,
        health: provider_sdk::CapabilityHealth,
    ) -> Result<(), ProviderError> {
        let key = self.0.key(cap)?;
        let health = match health {
            provider_sdk::CapabilityHealth::Available => wire::CapabilityHealth::Available,
            provider_sdk::CapabilityHealth::Unavailable(reason) => {
                wire::CapabilityHealth::Unavailable {
                    reason: match reason {
                        provider_sdk::UnavailableReason::TargetNotReady => {
                            wire::UnavailableReason::TargetNotReady
                        }
                        provider_sdk::UnavailableReason::UnsupportedBuild => {
                            wire::UnavailableReason::UnsupportedBuild
                        }
                        provider_sdk::UnavailableReason::ReadFailed { message } => {
                            wire::UnavailableReason::ReadFailed {
                                message: public_message(&message),
                            }
                        }
                        provider_sdk::UnavailableReason::ValidationFailed { message } => {
                            wire::UnavailableReason::ValidationFailed {
                                message: public_message(&message),
                            }
                        }
                        provider_sdk::UnavailableReason::ProviderFailed { message } => {
                            wire::UnavailableReason::ProviderFailed {
                                message: public_message(&message),
                            }
                        }
                    },
                }
            }
        };
        self.0.push(Operation::Health(key, health), 1024)
    }
}

fn public_message(message: &str) -> String {
    message.chars().take(256).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support as fixture;

    #[test]
    fn resets_survive_exhausted_data_budgets_and_reject_duplicates() -> anyhow::Result<()> {
        let batch = PollBatch::new(&fixture::MANIFEST);
        let key = fixture::topic(0);
        batch.pending.borrow_mut().bytes =
            MAX_BATCH_BYTES - size_of::<Operation>() - key.provider_id.len() - key.topic.len() - 1;
        let mut events = batch.events();
        events.snapshot(&fixture::CAPS[0], &serde_json::json!(0))?;
        assert_eq!(batch.pending.borrow().bytes, MAX_BATCH_BYTES);
        events.reset(&fixture::CAPS[0])?;
        assert!(
            events
                .snapshot(&fixture::CAPS[0], &serde_json::json!(1))
                .is_err()
        );
        events.reset(&fixture::CAPS[1])?;
        assert!(events.reset(&fixture::CAPS[0]).is_err());
        let mut unknown = fixture::CAPS[0];
        unknown.topic = "undeclared";
        assert!(events.reset(&unknown).is_err());
        assert_eq!(
            batch.reset_intents(),
            vec![fixture::topic(0), fixture::topic(1)]
        );
        let pending = batch.pending.borrow();
        assert!(pending.failed);
        assert!(matches!(
            pending.ops.as_slice(),
            [
                Operation::Snapshot(..),
                Operation::Reset(_),
                Operation::Reset(_)
            ]
        ));
        Ok(())
    }
}
