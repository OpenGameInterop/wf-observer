//! Account currency balances.

use crate::AccountId;

/// Native account balances.
///
/// Signed 32-bit integers preserve the game's representation, including zero and
/// negative balances. JSON numbers represent this range exactly on every binding target.
#[boltffi::data]
#[derive(Debug, ..Copy, ..Eq, ..Serde)]
pub struct CurrencyBalances {
    pub credits: i32,
    pub endo: i32,
    /// Platinum eligible for player trading.
    pub tradable_platinum: i32,
    /// Free or otherwise non-tradable Platinum; excludes the tradable balance.
    pub non_tradable_platinum: i32,
}

/// One account's complete currency replacement.
#[derive(Debug, Clone, ..Eq, ..Serde)]
pub struct CurrencySnapshot {
    pub account_id: AccountId,
    pub balances: CurrencyBalances,
}
