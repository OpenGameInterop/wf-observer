use crate::api::{DataEnvelope, EnvelopeMetadata, ObserverError};

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct RelicRewardChoice {
    /// Canonical `StoreItem` path, which may differ from an inventory Types path.
    pub item_key: String,
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub enum RelicRewardPicker {
    Closed,
    /// Up to four slots in picker order. Duplicates are preserved. Empty is a
    /// valid opening transition. This does not identify selected/granted rewards.
    Open {
        choices: Vec<RelicRewardChoice>,
    },
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct WarframeRelicRewards {
    pub metadata: EnvelopeMetadata,
    pub account_id: String,
    pub picker: RelicRewardPicker,
}

impl From<crate::raw::TypedData<warframe_model::RelicRewardsSnapshot>> for WarframeRelicRewards {
    fn from(value: crate::raw::TypedData<warframe_model::RelicRewardsSnapshot>) -> Self {
        Self {
            metadata: value.metadata.into(),
            account_id: value.data.account_id.into(),
            picker: match value.data.picker {
                warframe_model::RelicRewardPicker::Closed => RelicRewardPicker::Closed,
                warframe_model::RelicRewardPicker::Open { choices } => RelicRewardPicker::Open {
                    choices: choices
                        .into_iter()
                        .map(|choice| RelicRewardChoice {
                            item_key: choice.item_key.into(),
                        })
                        .collect(),
                },
            },
        }
    }
}

impl TryFrom<DataEnvelope> for WarframeRelicRewards {
    type Error = ObserverError;
    fn try_from(envelope: DataEnvelope) -> Result<Self, Self::Error> {
        Ok(
            crate::raw::decode_snapshot::<crate::warframe::RelicRewardsTopic>(
                envelope.try_into()?,
            )?
            .into(),
        )
    }
}

#[boltffi::data(impl)]
impl WarframeRelicRewards {
    /// Converts a generic envelope. Reads and watches already return typed values.
    /// # Errors
    /// Rejects incorrect identity, metadata, schema or malformed data.
    pub fn from_envelope(envelope: DataEnvelope) -> Result<Self, ObserverError> {
        envelope.try_into()
    }
}
