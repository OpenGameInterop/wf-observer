//! Finds the logged-in account and the objects holding its data.
//!
//! Starting at the executable's manager root, follow a control pointer to the
//! manager, then its profile vector to the selected profile, then the profile's
//! data control to profile data. A "control" here is the game's ownership object:
//! its first pointer leads to the actual manager, profile or data object. Reading
//! it does not give us ownership or prevent the game from replacing that object.
//!
//! `validate_login_layout` checks game instructions against the compiled facts
//! during session preparation. `resolve_login` follows the live pointers and
//! checks object vtables, login state and account identity on each acquisition.
//! It requires two consecutive resolutions to agree. The session also compares
//! resolutions before and after reading account data, rejecting a sample if its
//! owner changed. Neither comparison freezes game memory or detects every change
//! that happens and reverses between reads.
//!
//! `Absent` means an observed no-data state, such as a missing manager, logged-out
//! manager, empty profile vector or missing profile data. Errors mean we could
//! not establish ownership: reads failed, data changed, or selection/layout was
//! contradictory or ambiguous. Neither outcome supplies an empty domain value.

mod account;
mod facts;
mod identity;
mod resolve;

pub(crate) use facts::LOGIN;
pub(crate) use identity::{LoginIdentity, ObjectIdentity};
pub(crate) use resolve::{LoginResolution, ResolveError, resolve_login, validate_login_layout};
