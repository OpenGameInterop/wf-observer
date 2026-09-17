use crate::api::{DataEnvelope, EnvelopeMetadata, ObserverError};

pub use warframe_model::Screen;

/// Visible interface movies. This is a set in deterministic order, not focus
/// or stacking order. Empty means none visible; unknown state is Unavailable.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct WarframeScreens {
    pub metadata: EnvelopeMetadata,
    pub screens: Vec<Screen>,
}

impl From<crate::raw::TypedData<warframe_model::ScreensSnapshot>> for WarframeScreens {
    fn from(value: crate::raw::TypedData<warframe_model::ScreensSnapshot>) -> Self {
        Self {
            metadata: value.metadata.into(),
            screens: value.data.screens,
        }
    }
}

impl TryFrom<DataEnvelope> for WarframeScreens {
    type Error = ObserverError;
    fn try_from(envelope: DataEnvelope) -> Result<Self, Self::Error> {
        Ok(
            crate::raw::decode_snapshot::<crate::warframe::ScreensTopic>(envelope.try_into()?)?
                .into(),
        )
    }
}

#[boltffi::data(impl)]
impl WarframeScreens {
    /// Converts a generic envelope. Reads and watches already return typed values.
    /// # Errors
    /// Rejects incorrect identity, metadata, schema or malformed data.
    pub fn from_envelope(envelope: DataEnvelope) -> Result<Self, ObserverError> {
        envelope.try_into()
    }
}
