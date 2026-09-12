//! Common derive combinations; keep uncommon derives explicit at their call sites.

derive_aliases::define! {
    Eq = ::core::cmp::PartialEq, ::core::cmp::Eq;
}
