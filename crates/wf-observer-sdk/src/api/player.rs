use crate::api::{DataEnvelope, EnvelopeMetadata, ObserverError};

/// Local player information belonging to an explicit account and source generation.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct WarframePlayer {
    pub metadata: EnvelopeMetadata,
    pub account_id: String,
    pub username: String,
}

impl From<crate::raw::TypedData<warframe_model::PlayerSnapshot>> for WarframePlayer {
    fn from(value: crate::raw::TypedData<warframe_model::PlayerSnapshot>) -> Self {
        Self {
            metadata: value.metadata.into(),
            account_id: value.data.account_id.into(),
            username: value.data.username.into(),
        }
    }
}

impl TryFrom<DataEnvelope> for WarframePlayer {
    type Error = ObserverError;

    fn try_from(envelope: DataEnvelope) -> Result<Self, Self::Error> {
        Ok(
            crate::raw::decode_snapshot::<crate::warframe::PlayerTopic>(envelope.try_into()?)?
                .into(),
        )
    }
}

#[boltffi::data(impl)]
impl WarframePlayer {
    /// Builds a typed value from a generic envelope. Reads and watches already
    /// return typed values, so they do not require this conversion.
    ///
    /// # Errors
    /// Rejects the wrong game, topic or schema, invalid metadata, and malformed data.
    pub fn from_envelope(envelope: DataEnvelope) -> Result<Self, ObserverError> {
        envelope.try_into()
    }
}
