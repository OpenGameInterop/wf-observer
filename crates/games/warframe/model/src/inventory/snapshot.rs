use super::InventoryFamily;
use crate::{AccountId, ItemKey};
use serde::Deserialize;

/// Counts for one semantic family; duplicate records have already been aggregated.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct InventoryFamilySnapshot {
    pub family: InventoryFamily,
    /// Strictly increasing item-key order, with no zero quantities.
    pub items: Vec<InventoryItemCount>,
}

/// Aggregated ownership of one game-authored item identity.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct InventoryItemCount {
    pub item_key: ItemKey,
    /// Positive unsigned count, encoded in JSON as decimal digits with no sign or
    /// leading zero. Preserve it as text or an integer, never floating point.
    #[serde(with = "super::quantity")]
    pub quantity: u64,
}

/// Complete validated inventory, including explicitly empty families.
///
/// Missing families or malformed data cannot masquerade as an empty inventory.
#[derive(Debug, Clone, ..Eq, ..Serde)]
#[serde(try_from = "UncheckedSnapshot")]
pub struct InventorySnapshot {
    account_id: AccountId,
    families: Vec<InventoryFamilySnapshot>,
}

#[derive(Deserialize)]
struct UncheckedSnapshot {
    account_id: AccountId,
    families: Vec<InventoryFamilySnapshot>,
}

impl TryFrom<UncheckedSnapshot> for InventorySnapshot {
    type Error = InvalidInventory;
    fn try_from(value: UncheckedSnapshot) -> Result<Self, Self::Error> {
        Self::new(value.account_id, value.families)
    }
}

/// An inventory violates the portable topic schema.
#[derive(Debug, derive_more::Error, displaydoc::Display, ..Copy, ..Eq)]
pub enum InvalidInventory {
    /// inventory must contain every family exactly once in schema order
    Families,
    /// inventory item keys must be unique and ordered within each family
    ItemOrder,
    /// inventory quantities must be positive
    ZeroQuantity,
}

impl InventorySnapshot {
    /// Validates a complete replacement inventory.
    ///
    /// # Errors
    ///
    /// Rejects missing/reordered families, duplicate/unordered keys, or zero counts.
    pub fn new(
        account_id: AccountId,
        families: Vec<InventoryFamilySnapshot>,
    ) -> Result<Self, InvalidInventory> {
        if !families
            .iter()
            .map(|f| f.family)
            .eq(InventoryFamily::ALL.iter().copied())
        {
            return Err(InvalidInventory::Families);
        }
        for family in &families {
            if family
                .items
                .windows(2)
                .any(|pair| pair[0].item_key >= pair[1].item_key)
            {
                return Err(InvalidInventory::ItemOrder);
            }
            if family.items.iter().any(|item| item.quantity == 0) {
                return Err(InvalidInventory::ZeroQuantity);
            }
        }
        Ok(Self {
            account_id,
            families,
        })
    }

    /// Opaque Warframe account identifier captured with this inventory.
    ///
    /// Session, generation, and health still govern snapshot freshness.
    #[must_use]
    pub fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    /// Every family, including explicitly empty collections.
    #[must_use]
    pub fn families(&self) -> &[InventoryFamilySnapshot] {
        &self.families
    }

    /// Consumes this inventory for adaptation to another owned representation.
    #[must_use]
    pub fn into_parts(self) -> (AccountId, Vec<InventoryFamilySnapshot>) {
        (self.account_id, self.families)
    }

    /// Finds a family without assuming its numeric discriminant is an array index.
    #[must_use]
    pub fn family(&self, family: InventoryFamily) -> Option<&InventoryFamilySnapshot> {
        self.families.iter().find(|entry| entry.family == family)
    }

    /// Looks up a count; absent item keys represent zero owned items.
    #[must_use]
    pub fn quantity(&self, family: InventoryFamily, item_key: &ItemKey) -> u64 {
        self.family(family)
            .and_then(|family| {
                family
                    .items
                    .binary_search_by(|item| item.item_key.cmp(item_key))
                    .ok()
                    .map(|index| family.items[index].quantity)
            })
            .unwrap_or(0)
    }
}
