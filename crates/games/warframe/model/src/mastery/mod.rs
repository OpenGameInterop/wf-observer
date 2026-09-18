//! Account mastery and raw retained item affinity.

mod points;
#[cfg(test)]
mod tests;

use crate::{AccountId, ItemKey};
use serde::Deserialize;

/// Progression retained for one canonical item, including equipment no longer owned.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct MasteryItemProgress {
    pub item_key: ItemKey,
    /// Raw cumulative affinity, which can exceed the item's maximum-rank threshold.
    /// Consumers combine this with item metadata to derive rank or completion.
    /// Encoded as canonical unsigned decimal text in JSON, including zero.
    #[serde(with = "points")]
    pub affinity: u64,
}

/// Complete account progression, with unique items in canonical key order.
#[derive(Debug, Clone, ..Eq, ..Serde)]
#[serde(try_from = "UncheckedSnapshot")]
pub struct MasterySnapshot {
    account_id: AccountId,
    rank: u32,
    #[serde(with = "points")]
    total_points: u64,
    #[serde(with = "points")]
    item_points: u64,
    items: Vec<MasteryItemProgress>,
}

#[derive(Deserialize)]
struct UncheckedSnapshot {
    account_id: AccountId,
    rank: u32,
    #[serde(with = "points")]
    total_points: u64,
    #[serde(with = "points")]
    item_points: u64,
    items: Vec<MasteryItemProgress>,
}

impl TryFrom<UncheckedSnapshot> for MasterySnapshot {
    type Error = InvalidMastery;

    fn try_from(value: UncheckedSnapshot) -> Result<Self, Self::Error> {
        Self::new(
            value.account_id,
            value.rank,
            value.total_points,
            value.item_points,
            value.items,
        )
    }
}

/// A mastery snapshot violates the portable topic schema.
#[derive(Debug, derive_more::Error, displaydoc::Display, ..Copy, ..Eq)]
pub enum InvalidMastery {
    /// mastery item keys must be unique and ordered
    ItemOrder,
    /// item mastery points exceed total mastery points
    Points,
}

impl MasterySnapshot {
    /// Validates a complete replacement. Zero progression and empty collections are valid.
    ///
    /// # Errors
    /// Rejects duplicate/unordered keys or item points greater than total points.
    pub fn new(
        account_id: AccountId,
        rank: u32,
        total_points: u64,
        item_points: u64,
        items: Vec<MasteryItemProgress>,
    ) -> Result<Self, InvalidMastery> {
        if items
            .windows(2)
            .any(|pair| pair[0].item_key >= pair[1].item_key)
        {
            return Err(InvalidMastery::ItemOrder);
        }
        if item_points > total_points {
            return Err(InvalidMastery::Points);
        }
        Ok(Self {
            account_id,
            rank,
            total_points,
            item_points,
            items,
        })
    }

    /// Account captured with this snapshot; metadata still governs freshness.
    #[must_use]
    pub fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    /// Completed mastery rank reported by the game.
    #[must_use]
    pub fn rank(&self) -> u32 {
        self.rank
    }

    /// Mastery points from all account progression pools.
    #[must_use]
    pub fn total_points(&self) -> u64 {
        self.total_points
    }

    /// Game-calculated mastery points contributed by item progression.
    #[must_use]
    pub fn item_points(&self) -> u64 {
        self.item_points
    }

    /// Per-item raw affinity in canonical key order.
    #[must_use]
    pub fn items(&self) -> &[MasteryItemProgress] {
        &self.items
    }

    /// Number of items with retained progression.
    #[must_use]
    pub fn tracked_items(&self) -> u64 {
        self.items.len() as u64
    }

    /// Looks up retained progression; absence is distinct from a zero-affinity record.
    #[must_use]
    pub fn get(&self, item_key: &ItemKey) -> Option<&MasteryItemProgress> {
        self.items
            .binary_search_by(|item| item.item_key.cmp(item_key))
            .ok()
            .map(|index| &self.items[index])
    }

    /// Consumes the snapshot for adaptation to another owned representation.
    #[must_use]
    pub fn into_parts(self) -> (AccountId, u32, u64, u64, Vec<MasteryItemProgress>) {
        (
            self.account_id,
            self.rank,
            self.total_points,
            self.item_points,
            self.items,
        )
    }
}
