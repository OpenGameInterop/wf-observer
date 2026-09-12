//! Player identity and display name.

use crate::{AccountId, PlayerName};

/// The account's current display name. Account identity remains stable across renames.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct PlayerSnapshot {
    pub account_id: AccountId,
    pub username: PlayerName,
}
