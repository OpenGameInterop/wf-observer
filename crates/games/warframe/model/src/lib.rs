//! Portable Warframe models with shared binding types.

#[macro_use(derive)]
extern crate derive_aliases;

#[cfg(test)]
mod progression_tests;

mod derive_alias;

mod account_id;
pub mod chat;
pub mod currencies;
pub mod intrinsics;
pub mod inventory;
mod item_key;
pub mod mastery;
pub mod player;
mod player_name;
pub mod relic_rewards;
pub mod screens;
pub mod star_chart;

pub use account_id::{AccountId, InvalidAccountId};
pub use chat::{ChatChannel, ChatEvent, ChatMessage, ChatTime, ChatUpdate};
pub use currencies::{CurrencyBalances, CurrencySnapshot};
pub use intrinsics::{
    DrifterIntrinsics, IntrinsicsSnapshot, InvalidIntrinsics, RailjackIntrinsics,
};
pub use inventory::{
    InvalidInventory, InventoryFamily, InventoryFamilySnapshot, InventoryItemCount,
    InventorySnapshot,
};
pub use item_key::{InvalidItemKey, ItemKey};
pub use mastery::{InvalidMastery, MasteryItemProgress, MasteryPointBreakdown, MasterySnapshot};
pub use player::PlayerSnapshot;
pub use player_name::{InvalidPlayerName, PlayerName};
pub use relic_rewards::{RelicRewardChoice, RelicRewardPicker, RelicRewardsSnapshot};
pub use screens::{Screen, ScreensSnapshot};
pub use star_chart::{
    InvalidStarChart, StarChartDifficulty, StarChartNodeProgress, StarChartSnapshot,
};
