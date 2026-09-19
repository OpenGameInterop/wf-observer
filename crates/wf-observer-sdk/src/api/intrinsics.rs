use crate::api::{DataEnvelope, EnvelopeMetadata, ObserverError};
pub use warframe_model::{DrifterIntrinsics, RailjackIntrinsics};

/// Complete account intrinsics replacement with subscription freshness metadata.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct WarframeIntrinsics {
    pub metadata: EnvelopeMetadata,
    pub account_id: String,
    pub railjack: RailjackIntrinsics,
    pub drifter: DrifterIntrinsics,
}

impl From<crate::raw::TypedData<warframe_model::IntrinsicsSnapshot>> for WarframeIntrinsics {
    fn from(value: crate::raw::TypedData<warframe_model::IntrinsicsSnapshot>) -> Self {
        let (account_id, railjack, drifter) = value.data.into_parts();
        Self {
            metadata: value.metadata.into(),
            account_id: account_id.into(),
            railjack,
            drifter,
        }
    }
}

impl TryFrom<DataEnvelope> for WarframeIntrinsics {
    type Error = ObserverError;
    fn try_from(envelope: DataEnvelope) -> Result<Self, Self::Error> {
        Ok(
            crate::raw::decode_snapshot::<crate::warframe::IntrinsicsTopic>(envelope.try_into()?)?
                .into(),
        )
    }
}

#[boltffi::data(impl)]
impl WarframeIntrinsics {
    /// Converts a generic envelope after validating topic identity and progression.
    /// # Errors
    /// Rejects wrong game/topic/schema, invalid metadata or malformed progression.
    pub fn from_envelope(envelope: DataEnvelope) -> Result<Self, ObserverError> {
        envelope.try_into()
    }
}
