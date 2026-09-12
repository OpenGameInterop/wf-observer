use crate::api::{EnvelopeMetadata, EventEnvelope, ObserverError};

pub use warframe_model::{ChatChannel, ChatMessage, ChatTime, ChatUpdate};

/// Typed chat update with account identity and the standard stream metadata.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct WarframeChatEvent {
    pub metadata: EnvelopeMetadata,
    pub account_id: String,
    pub update: ChatUpdate,
}

impl From<crate::raw::TypedData<warframe_model::ChatEvent>> for WarframeChatEvent {
    fn from(value: crate::raw::TypedData<warframe_model::ChatEvent>) -> Self {
        Self {
            metadata: value.metadata.into(),
            account_id: value.data.account_id.into(),
            update: value.data.update,
        }
    }
}

impl TryFrom<EventEnvelope> for WarframeChatEvent {
    type Error = ObserverError;

    fn try_from(envelope: EventEnvelope) -> Result<Self, Self::Error> {
        Ok(crate::raw::decode_event::<crate::warframe::ChatTopic>(envelope.try_into()?)?.into())
    }
}

#[boltffi::data(impl)]
impl WarframeChatEvent {
    /// Builds a typed value from a generic envelope. Chat watches already return
    /// typed values, so they do not require this conversion.
    ///
    /// # Errors
    /// Rejects the wrong game, topic or schema, invalid metadata, and malformed data.
    pub fn from_envelope(envelope: EventEnvelope) -> Result<Self, ObserverError> {
        envelope.try_into()
    }
}
