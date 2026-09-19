//! Purchased Intrinsic ranks and spendable points, independent of mastery credit.
use crate::AccountId;

/// Railjack's purchased ranks and whole points available to spend.
#[boltffi::data]
#[derive(Debug, Default, ..Copy, ..Eq, ..Serde)]
pub struct RailjackIntrinsics {
    pub unspent_points: u32,
    pub piloting: u32,
    pub gunnery: u32,
    pub tactical: u32,
    pub engineering: u32,
    pub command: u32,
}

/// Drifter's purchased ranks and whole points available to spend.
#[boltffi::data]
#[derive(Debug, Default, ..Copy, ..Eq, ..Serde)]
pub struct DrifterIntrinsics {
    pub unspent_points: u32,
    pub combat: u32,
    pub riding: u32,
    pub opportunity: u32,
    pub endurance: u32,
}

/// One account's complete Intrinsics replacement. Zero ranks/balances are valid.
#[derive(Debug, Clone, ..Eq, ..Serde)]
#[serde(try_from = "UncheckedSnapshot")]
pub struct IntrinsicsSnapshot {
    account_id: AccountId,
    railjack: RailjackIntrinsics,
    drifter: DrifterIntrinsics,
}

#[derive(serde::Deserialize)]
struct UncheckedSnapshot {
    account_id: AccountId,
    railjack: RailjackIntrinsics,
    drifter: DrifterIntrinsics,
}

/// Intrinsic ranks must be between zero and ten.
#[derive(Debug, derive_more::Error, displaydoc::Display, ..Copy, ..Eq)]
pub struct InvalidIntrinsics;

impl TryFrom<UncheckedSnapshot> for IntrinsicsSnapshot {
    type Error = InvalidIntrinsics;
    fn try_from(value: UncheckedSnapshot) -> Result<Self, Self::Error> {
        Self::new(value.account_id, value.railjack, value.drifter)
    }
}

impl IntrinsicsSnapshot {
    /// Validates all purchased ranks. Balances are whole spendable points.
    /// # Errors
    /// Rejects ranks above ten.
    pub fn new(
        account_id: AccountId,
        railjack: RailjackIntrinsics,
        drifter: DrifterIntrinsics,
    ) -> Result<Self, InvalidIntrinsics> {
        if [
            railjack.piloting,
            railjack.gunnery,
            railjack.tactical,
            railjack.engineering,
            railjack.command,
            drifter.combat,
            drifter.riding,
            drifter.opportunity,
            drifter.endurance,
        ]
        .into_iter()
        .any(|rank| rank > 10)
        {
            return Err(InvalidIntrinsics);
        }
        Ok(Self {
            account_id,
            railjack,
            drifter,
        })
    }

    #[must_use]
    pub fn account_id(&self) -> &AccountId {
        &self.account_id
    }
    #[must_use]
    pub fn railjack(&self) -> RailjackIntrinsics {
        self.railjack
    }
    #[must_use]
    pub fn drifter(&self) -> DrifterIntrinsics {
        self.drifter
    }
    #[must_use]
    pub fn into_parts(self) -> (AccountId, RailjackIntrinsics, DrifterIntrinsics) {
        (self.account_id, self.railjack, self.drifter)
    }
}
