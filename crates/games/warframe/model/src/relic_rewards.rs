//! The currently visible relic picker, not a history of selected or granted rewards.

use crate::{AccountId, ItemKey};

#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct RelicRewardChoice {
    /// Canonical `StoreItem` path. It need not equal an inventory Types path.
    pub item_key: ItemKey,
}

#[derive(Debug, Clone, ..Eq, ..Serde)]
#[serde(try_from = "PickerPayload")]
pub enum RelicRewardPicker {
    Closed,
    /// Picker order: local reward first, then the other players. Equal item
    /// keys in separate slots are preserved. Empty is valid while opening.
    Open {
        choices: Vec<RelicRewardChoice>,
    },
}

#[derive(serde::Deserialize)]
enum PickerPayload {
    Closed,
    Open { choices: Vec<RelicRewardChoice> },
}

impl TryFrom<PickerPayload> for RelicRewardPicker {
    type Error = &'static str;

    fn try_from(value: PickerPayload) -> Result<Self, Self::Error> {
        match value {
            PickerPayload::Closed => Ok(Self::Closed),
            PickerPayload::Open { choices } if choices.len() <= 4 => Ok(Self::Open { choices }),
            PickerPayload::Open { .. } => Err("relic picker has more than four choices"),
        }
    }
}

#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct RelicRewardsSnapshot {
    pub account_id: AccountId,
    pub picker: RelicRewardPicker,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_payload_preserves_slots_and_rejects_invalid_counts_and_keys()
    -> Result<(), Box<dyn std::error::Error>> {
        let choice = serde_json::json!({ "item_key": "/Lotus/StoreItems/Example" });
        for count in 0..=4 {
            let value = serde_json::json!({"Open": {"choices": vec![choice.clone(); count]}});
            let decoded: RelicRewardPicker = serde_json::from_value(value.clone())?;
            assert_eq!(serde_json::to_value(decoded)?, value);
        }
        for bad in [
            serde_json::json!({"Open": {"choices": vec![choice; 5]}}),
            serde_json::json!({"Open": {"choices": [{"item_key": "invalid"}]}}),
            serde_json::json!({"Open": {}}),
        ] {
            assert!(serde_json::from_value::<RelicRewardPicker>(bad).is_err());
        }
        assert_eq!(
            serde_json::from_str::<RelicRewardPicker>("\"Closed\"")?,
            RelicRewardPicker::Closed
        );
        Ok(())
    }
}
