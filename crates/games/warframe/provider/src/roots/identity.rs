use std::{mem::size_of, num::NonZeroU64};

use warframe_model::AccountId;

/// Validated internal object identity that is never exposed to consumers.
#[derive(Debug, Hash, ..Copy, ..Ord)]
pub(crate) struct ObjectIdentity(NonZeroU64);

impl ObjectIdentity {
    /// Creates a non-null target object identity.
    pub(crate) const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub(crate) const fn from_address(value: u64) -> Option<Self> {
        if value.is_multiple_of(size_of::<u64>() as u64) {
            Self::new(value)
        } else {
            None
        }
    }

    /// Returns the internal identity value.
    pub(crate) const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Identities whose replacement makes login-scoped values incomparable.
///
/// Compare both control and target addresses: replacement of either breaks
/// continuity even if the account ID stays the same. Conversely, the account ID
/// detects an account switch that reuses the same objects. Addresses alone are
/// not lifetime tokens; reuse between observations can still go undetected.
#[derive(Clone, Debug, Hash, ..Eq)]
pub(crate) struct LoginIdentity {
    /// Stable account identifier used to detect an in-place account replacement.
    pub(crate) account_id: AccountId,
    /// Process-global profile-manager ownership/control identity.
    pub(crate) manager_control: ObjectIdentity,
    /// Profile-manager object identity.
    pub(crate) manager: ObjectIdentity,
    /// Selected profile ownership/control identity.
    pub(crate) profile_control: ObjectIdentity,
    /// Selected active profile identity.
    pub(crate) profile: ObjectIdentity,
    /// Profile-data ownership/control identity.
    pub(crate) profile_data_control: ObjectIdentity,
    /// Profile-data object identity.
    pub(crate) profile_data: ObjectIdentity,
}
