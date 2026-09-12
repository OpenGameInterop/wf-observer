use protocol::v1::{DataEnvelope, EnvelopeMetadata, EventEnvelope, TopicRef};
use serde::de::DeserializeOwned;

use crate::raw::ClientError;

/// A topic/schema decoder layered over the generic wire API.
///
/// Implement this in a client adapter for portable domain models. Snapshot and
/// event payloads may have different schemas; neither adds a game-specific RPC.
pub trait Topic: Send + Sync + 'static {
    const PROVIDER_ID: &'static str;
    const GAME_ID: &'static str;
    const NAME: &'static str;
    const SCHEMA_VERSION: u32;

    #[must_use]
    fn topic() -> TopicRef {
        TopicRef {
            provider_id: Self::PROVIDER_ID.into(),
            topic: Self::NAME.into(),
            schema_version: Self::SCHEMA_VERSION,
        }
    }
}

/// A topic that publishes complete current values.
pub trait SnapshotTopic: Topic {
    type Snapshot: DeserializeOwned + Clone + Send + Sync + 'static;
}

/// A topic that publishes individual occurrences.
pub trait EventTopic: Topic {
    type Event: DeserializeOwned + Clone + Send + Sync + 'static;
}

/// Decoded domain data with its original session, generation, and sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedData<T> {
    pub metadata: EnvelopeMetadata,
    pub data: T,
}

/// Decodes a snapshot only after checking its exact topic/schema identity.
///
/// # Errors
///
/// Returns `WrongTopic` for mismatched identity, or Decode for invalid domain JSON.
pub fn decode_snapshot<T: SnapshotTopic>(
    envelope: DataEnvelope,
) -> Result<TypedData<T::Snapshot>, ClientError> {
    check::<T>(&envelope.metadata)?;
    Ok(TypedData {
        data: serde_json::from_str(envelope.payload.as_str())?,
        metadata: envelope.metadata,
    })
}

/// Decodes an event while preserving its origin and publication sequence.
///
/// # Errors
///
/// Returns `WrongTopic` for mismatched identity, or Decode for invalid domain JSON.
pub fn decode_event<T: EventTopic>(
    envelope: EventEnvelope,
) -> Result<TypedData<T::Event>, ClientError> {
    check::<T>(&envelope.metadata)?;
    Ok(TypedData {
        data: serde_json::from_str(envelope.payload.as_str())?,
        metadata: envelope.metadata,
    })
}

fn check<T: Topic>(metadata: &EnvelopeMetadata) -> Result<(), ClientError> {
    if metadata.source.topic != T::topic() || metadata.source.game_id != T::GAME_ID {
        return Err(ClientError::WrongTopic);
    }
    Ok(())
}
