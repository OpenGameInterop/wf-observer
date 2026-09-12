//! Adapters for types whose binding representation differs from the protocol.

use super::{
    DataEnvelope, EnvelopeMetadata, EventEnvelope, ServiceCursor, ServiceStatus, TargetActivity,
    TargetStatus, TopicSnapshot, TopicStatus,
};
use crate::api::ObserverError;
use crate::raw::types as wire;

impl From<wire::ServiceCursor> for ServiceCursor {
    fn from(value: wire::ServiceCursor) -> Self {
        Self {
            run_id: value.run_id,
            sequence: value.sequence.to_string(),
        }
    }
}

impl From<wire::EnvelopeMetadata> for EnvelopeMetadata {
    fn from(value: wire::EnvelopeMetadata) -> Self {
        Self {
            source: value.source,
            generation: value.generation.to_string(),
            sequence: value.sequence.to_string(),
        }
    }
}

impl From<wire::DataEnvelope> for DataEnvelope {
    fn from(value: wire::DataEnvelope) -> Self {
        Self {
            metadata: value.metadata.into(),
            payload_json: value.payload.into(),
        }
    }
}

impl TryFrom<DataEnvelope> for wire::DataEnvelope {
    type Error = ObserverError;

    fn try_from(value: DataEnvelope) -> Result<Self, Self::Error> {
        Ok(Self {
            metadata: value.metadata.try_into()?,
            payload: value
                .payload_json
                .try_into()
                .map_err(|error: serde_json::Error| ObserverError::PayloadDecode {
                    message: error.to_string(),
                })?,
        })
    }
}

impl TryFrom<EnvelopeMetadata> for wire::EnvelopeMetadata {
    type Error = ObserverError;

    fn try_from(value: EnvelopeMetadata) -> Result<Self, Self::Error> {
        Ok(Self {
            source: value.source,
            generation: counter(&value.generation)?,
            sequence: counter(&value.sequence)?,
        })
    }
}

impl TryFrom<EventEnvelope> for wire::EventEnvelope {
    type Error = ObserverError;

    fn try_from(value: EventEnvelope) -> Result<Self, Self::Error> {
        Ok(Self {
            metadata: value.metadata.try_into()?,
            payload: value
                .payload_json
                .try_into()
                .map_err(|error: serde_json::Error| ObserverError::PayloadDecode {
                    message: error.to_string(),
                })?,
        })
    }
}

fn counter(text: &str) -> Result<u64, ObserverError> {
    text.parse::<u64>()
        .ok()
        .filter(|number| number.to_string() == text)
        .ok_or_else(|| ObserverError::PayloadDecode {
            message: "invalid unsigned decimal metadata".into(),
        })
}

impl From<wire::EventEnvelope> for EventEnvelope {
    fn from(value: wire::EventEnvelope) -> Self {
        Self {
            metadata: value.metadata.into(),
            payload_json: value.payload.into(),
        }
    }
}

impl From<wire::TopicSnapshot> for TopicSnapshot {
    fn from(value: wire::TopicSnapshot) -> Self {
        Self {
            source: value.source,
            generation: value.generation.to_string(),
            health: value.health,
            snapshot: value.snapshot.map(Into::into),
        }
    }
}

impl From<wire::ServiceStatus> for ServiceStatus {
    fn from(value: wire::ServiceStatus) -> Self {
        Self {
            cursor: value.cursor.into(),
            application_version: value.application_version,
            discovery: value.discovery,
            targets: value.targets.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<wire::TargetStatus> for TargetStatus {
    fn from(value: wire::TargetStatus) -> Self {
        Self {
            provider_id: value.provider_id,
            game_id: value.game_id,
            target: value.target,
            activity: value.activity.into(),
        }
    }
}

impl From<wire::TargetActivity> for TargetActivity {
    fn from(value: wire::TargetActivity) -> Self {
        match value {
            wire::TargetActivity::Attaching => Self::Attaching,
            wire::TargetActivity::Retrying { message } => Self::Retrying { message },
            wire::TargetActivity::Observing {
                session_id,
                game_build,
                topics,
            } => Self::Observing {
                session_id,
                game_build,
                topics: topics.into_iter().map(Into::into).collect(),
            },
        }
    }
}

impl From<wire::TopicStatus> for TopicStatus {
    fn from(value: wire::TopicStatus) -> Self {
        Self {
            topic: value.topic,
            generation: value.generation.to_string(),
            health: value.health,
        }
    }
}
