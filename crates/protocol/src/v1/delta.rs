//! Snapshot delivery between the service and SDK. Applications see reconstructed state.

use super::{
    CapabilityHealth, DataEnvelope, EnvelopeMetadata, JsonPayload, ServiceCursor, SubscriptionItem,
    SubscriptionUpdate, TopicSnapshot, UpdateEnvelope,
};

/// A snapshot that must be acknowledged after installation in the SDK cache.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct SnapshotFrame {
    /// Absent for a bootstrap Topic; otherwise the original update checkpoint.
    pub cursor: Option<ServiceCursor>,
    pub metadata: EnvelopeMetadata,
    pub payload: SnapshotPayload,
}

#[derive(Debug, Clone, ..Eq, ..Serde)]
pub enum SnapshotPayload {
    Full(JsonPayload),
    Delta {
        /// The baseline snapshot's [`EnvelopeMetadata::sequence`].
        /// The baseline must share the target's source and generation.
        base_sequence: u64,
        hash: [u8; 32],
        /// RFC 6902 JSON Patch, encoded as text inside postcard.
        patch: JsonPayload,
    },
}

/// Confirms applied state, not receipt of transport bytes.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct SnapshotAck {
    pub metadata: EnvelopeMetadata,
    pub hash: [u8; 32],
}

impl SnapshotPayload {
    /// Builds a patch between complete snapshots. The sender decides whether it is smaller.
    ///
    /// # Errors
    ///
    /// Returns an error if JSON decoding or patch encoding fails.
    pub fn between(base: &DataEnvelope, target: &DataEnvelope) -> Result<Self, serde_json::Error> {
        Ok(Self::Delta {
            base_sequence: base.metadata.sequence,
            hash: target.payload.hash(),
            patch: target.payload.patch_from(&base.payload)?,
        })
    }
}

impl SnapshotFrame {
    /// Reconstructs a complete, hash-checked snapshot without modifying the baseline.
    ///
    /// # Errors
    ///
    /// A missing baseline, invalid patch or hash mismatch requires a fresh full bootstrap.
    pub fn reconstruct(
        self,
        base: Option<&DataEnvelope>,
    ) -> Result<SubscriptionItem, &'static str> {
        let payload = match self.payload {
            SnapshotPayload::Full(payload) => payload,
            SnapshotPayload::Delta {
                base_sequence,
                hash,
                patch,
            } => {
                let base = base
                    .filter(|base| {
                        base.metadata.source == self.metadata.source
                            && base.metadata.generation == self.metadata.generation
                            && base.metadata.sequence == base_sequence
                            && base_sequence < self.metadata.sequence
                    })
                    .ok_or("snapshot delta baseline mismatch")?;
                let mut value = serde_json::from_str(base.payload.as_str())
                    .map_err(|_| "invalid snapshot baseline")?;
                let patch: json_patch::Patch =
                    serde_json::from_str(patch.as_str()).map_err(|_| "invalid snapshot patch")?;
                // Our differ emits only these operations. In particular, disallow
                // Copy, which could amplify a small incoming patch into a huge value.
                if patch.0.iter().any(|op| {
                    !matches!(
                        op,
                        json_patch::PatchOperation::Add(_)
                            | json_patch::PatchOperation::Remove(_)
                            | json_patch::PatchOperation::Replace(_)
                    )
                }) {
                    return Err("unsupported snapshot patch operation");
                }
                json_patch::patch(&mut value, &patch).map_err(|_| "snapshot patch failed")?;
                let payload =
                    JsonPayload::from_value(&value).map_err(|_| "invalid patched snapshot")?;
                if payload.hash() != hash {
                    return Err("snapshot delta hash mismatch");
                }
                payload
            }
        };
        let topic = TopicSnapshot {
            source: self.metadata.source.clone(),
            generation: self.metadata.generation,
            health: CapabilityHealth::Available,
            snapshot: Some(DataEnvelope {
                metadata: self.metadata,
                payload,
            }),
        };
        Ok(match self.cursor {
            None => SubscriptionItem::Topic(topic),
            Some(cursor) => SubscriptionItem::Update(UpdateEnvelope {
                cursor,
                update: SubscriptionUpdate::TopicChanged(topic),
            }),
        })
    }
}
