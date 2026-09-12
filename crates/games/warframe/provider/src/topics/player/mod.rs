//! The active profile's preferred/fallback name, using [`facts::PLAYER`].
//! Account ownership and resets are supplied by the session's shared login checks.

mod acquisition;
mod facts;
mod validation;

pub(crate) use acquisition::read_player;
pub(crate) use validation::validate_player_layout;
