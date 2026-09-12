//! Reads Credits, Endo and both Platinum balances from login-owned profile data.
//!
//! [`acquisition`] checks each protected scalar's integrity and compares two full
//! sets of balances. The session checks account ownership around acquisition.
//! [`facts`] and [`validation`] are independent of inventory vectors, item types
//! and string-pool lookup. Scalar decoding is shared through [`crate::scalar`].

mod acquisition;
mod facts;
mod validation;

pub(crate) use acquisition::read_currencies;
pub(crate) use validation::validate_currencies_layout;
