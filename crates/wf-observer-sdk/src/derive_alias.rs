//! Common derive combinations used by the client library.

#![allow(
    unused_macros,
    reason = "derive aliases are available before every alias has a call site"
)]

// Do not add more aliases. These cover the common derive combinations; keep
// uncommon derives explicit at their call sites.
derive_aliases::define! {
    Eq = ::core::cmp::PartialEq, ::core::cmp::Eq;
    Ord = ..Eq, ::core::cmp::PartialOrd, ::core::cmp::Ord;
    Copy = ::core::marker::Copy, ::core::clone::Clone;
    Serde = ::serde::Serialize, ::serde::Deserialize;
}
