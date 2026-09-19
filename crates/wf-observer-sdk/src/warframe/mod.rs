//! Typed Warframe helpers over the generic protocol.
mod chat;
mod currencies;
mod inventory;
mod mastery;
mod player;
mod relic_rewards;
mod screens;

pub use chat::{ChatTopic, decode_chat};
pub use currencies::{CurrenciesTopic, decode_currencies};
pub use inventory::{InventoryTopic, decode_inventory};
pub use mastery::{MasteryTopic, decode_mastery};
pub use player::{PlayerTopic, decode_player};
pub use relic_rewards::{RelicRewardsTopic, decode_relic_rewards};
pub use screens::{ScreensTopic, decode_screens};
pub use warframe_model::{
    AccountId, ChatChannel, ChatDirection, ChatEvent, ChatMessage, ChatTime, ChatUpdate,
    CurrencyBalances, CurrencySnapshot, InvalidAccountId, InvalidPlayerName, InventoryFamily,
    InventoryFamilySnapshot, InventoryItemCount, InventorySnapshot, ItemKey, PlayerName,
    PlayerSnapshot,
};
pub use warframe_model::{
    InvalidMastery, MasteryItemProgress, MasteryPointBreakdown, MasterySnapshot,
};
pub use warframe_model::{
    RelicRewardChoice, RelicRewardPicker, RelicRewardsSnapshot, Screen, ScreensSnapshot,
};

mod intrinsics;
mod star_chart;
pub use intrinsics::{IntrinsicsTopic, decode_intrinsics};
pub use star_chart::{StarChartTopic, decode_star_chart};
pub use warframe_model::{
    DrifterIntrinsics, IntrinsicsSnapshot, InvalidIntrinsics, InvalidStarChart, RailjackIntrinsics,
    StarChartDifficulty, StarChartNodeProgress, StarChartSnapshot,
};
