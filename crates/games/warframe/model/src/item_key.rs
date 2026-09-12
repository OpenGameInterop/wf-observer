use derive_more::{Display, Error};
use serde::{Deserialize, Deserializer, Serialize, de};
use std::{borrow::Borrow, str::FromStr};

/// Canonical, case-sensitive game-authored identity beneath `/Lotus/`, not a display name.
#[derive(Debug, Clone, Hash, Serialize, Display, ..Ord)]
#[serde(transparent)]
#[display("{_0}")]
pub struct ItemKey(String);

impl ItemKey {
    /// Creates a key without restricting the open-ended game path catalog.
    ///
    /// # Errors
    ///
    /// Rejects paths outside `/Lotus/`, empty segments, and NUL bytes.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidItemKey> {
        let value = value.into();
        if value.strip_prefix("/Lotus/").is_some_and(|relative| {
            !relative.is_empty()
                && !relative.contains('\0')
                && relative.split('/').all(|segment| !segment.is_empty())
        }) {
            Ok(Self(value))
        } else {
            Err(InvalidItemKey(value))
        }
    }

    /// Borrows the canonical path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<ItemKey> for String {
    fn from(value: ItemKey) -> Self {
        value.0
    }
}

impl Borrow<str> for ItemKey {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl FromStr for ItemKey {
    type Err = InvalidItemKey;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for ItemKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// Rejected canonical item path.
#[derive(Debug, Clone, Display, Error, ..Eq)]
#[display("invalid Warframe item key: {_0}")]
pub struct InvalidItemKey(#[error(not(source))] String);
