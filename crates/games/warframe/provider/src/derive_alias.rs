//! Common derive combinations; keep uncommon derives explicit at their call sites.

derive_aliases::define! {
    Eq = ::core::cmp::PartialEq, ::core::cmp::Eq;
    Ord = ..Eq, ::core::cmp::PartialOrd, ::core::cmp::Ord;
    Copy = ::core::marker::Copy, ::core::clone::Clone;
}
