//! Finds the logged-in account and the objects holding its data.
//!
//! Starting at the executable's manager root, follow a control pointer to the
//! manager, then its profile vector to the selected profile, then the profile's
//! data control to profile data. A "control" here is the game's ownership object:
//! its first pointer leads to the actual manager, profile or data object. Reading
//! it does not give us ownership or prevent the game from replacing that object.
//!
//! `validate_account_layout` checks only account identity instructions against
//! the compiled facts. `resolve_account` follows the live pointers and
//! checks object vtables, login state and account identity on each acquisition.
//! It requires two consecutive resolutions to agree. The session also compares
//! resolutions before and after reading account data, rejecting a sample if its
//! owner changed. Profile-data layout and ownership are separately validated
//! only for consumers that need them. Player and relic rewards require account
//! identity without depending on profile data. Neither comparison freezes game memory or detects every change
//! that happens and reverses between reads.
//!
//! `Absent` means an observed no-data state, such as a missing manager, logged-out
//! manager or empty profile vector. Errors mean we could
//! not establish ownership: reads failed, data changed, or selection/layout was
//! contradictory or ambiguous. Neither outcome supplies an empty domain value.

mod account;
mod facts;
mod identity;
mod resolve;

pub(crate) use facts::{ACCOUNT, PROFILE_DATA};
pub(crate) use identity::{AccountIdentity, ObjectIdentity, ProfileDataIdentity};
pub(crate) use resolve::{
    AccountResolution, ResolveError, resolve_account, resolve_profile_data,
    validate_account_layout, validate_profile_data_layout,
};
