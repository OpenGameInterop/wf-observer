use super::InventoryFamily;
use crate::api::{DataEnvelope, EnvelopeMetadata, ObserverError};

/// Complete inventory replacement, validated in Rust.
#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct WarframeInventory {
    pub metadata: EnvelopeMetadata,
    /// Owning game account, encoded as 24 lowercase hexadecimal characters.
    pub account_id: String,
    pub families: Vec<InventoryFamilySnapshot>,
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct InventoryFamilySnapshot {
    pub family: InventoryFamily,
    pub items: Vec<InventoryItemCount>,
}

#[boltffi::data]
#[derive(Debug, Clone, ..Eq)]
pub struct InventoryItemCount {
    /// Canonical, case-sensitive /Lotus/... item path.
    pub item_key: String,
    /// Exact unsigned count. JVM callers can use `Long.toUnsignedString` for values
    /// above `Long.MAX_VALUE`; other bindings expose their native unsigned/exact integer.
    pub quantity: u64,
}

impl From<crate::raw::TypedData<crate::warframe::InventorySnapshot>> for WarframeInventory {
    fn from(value: crate::raw::TypedData<crate::warframe::InventorySnapshot>) -> Self {
        let (account_id, families) = value.data.into_parts();
        Self {
            metadata: value.metadata.into(),
            account_id: account_id.into(),
            families: families
                .into_iter()
                .map(|family| InventoryFamilySnapshot {
                    family: family.family,
                    items: family
                        .items
                        .into_iter()
                        .map(|item| InventoryItemCount {
                            item_key: item.item_key.into(),
                            quantity: item.quantity,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

impl TryFrom<DataEnvelope> for WarframeInventory {
    type Error = ObserverError;

    fn try_from(envelope: DataEnvelope) -> Result<Self, Self::Error> {
        Ok(
            crate::raw::decode_snapshot::<crate::warframe::InventoryTopic>(envelope.try_into()?)?
                .into(),
        )
    }
}

#[boltffi::data(impl)]
impl WarframeInventory {
    /// Builds a typed value from a generic envelope. Reads and watches already
    /// return typed values, so they do not require this conversion.
    ///
    /// # Errors
    /// Rejects the wrong game, topic or schema, invalid metadata, and malformed data.
    pub fn from_envelope(envelope: DataEnvelope) -> Result<Self, ObserverError> {
        envelope.try_into()
    }
}
