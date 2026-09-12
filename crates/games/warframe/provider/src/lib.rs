//! Warframe identification and validated read-only observation.

#[macro_use(derive)]
extern crate derive_aliases;

mod derive_alias;

mod item_type;
mod matching;
mod provider;
mod roots;
mod scalar;
mod session;
mod string_pool;
mod target;
mod topics;

pub use provider::WarframeProvider;
