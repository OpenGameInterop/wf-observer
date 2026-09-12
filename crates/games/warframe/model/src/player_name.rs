use derive_more::{Display, Error};
use serde::{Deserialize, Deserializer, Serialize, de};
use std::str::FromStr;

/// A nonempty player name of at most 128 UTF-8 bytes, with no NUL bytes.
#[derive(Debug, Clone, Hash, Serialize, Display, ..Ord)]
#[serde(transparent)]
#[display("{_0}")]
pub struct PlayerName(String);

impl PlayerName {
    /// Validates a name without changing its spelling or case.
    ///
    /// # Errors
    ///
    /// Rejects empty names, names over 128 bytes, and NUL bytes.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidPlayerName> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 || value.contains('\0') {
            Err(InvalidPlayerName)
        } else {
            Ok(Self(value))
        }
    }

    /// Borrows the name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<PlayerName> for String {
    fn from(value: PlayerName) -> Self {
        value.0
    }
}

impl FromStr for PlayerName {
    type Err = InvalidPlayerName;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for PlayerName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// Rejected player name; diagnostics do not retain the supplied value.
#[derive(Debug, Display, Error, ..Copy, ..Eq)]
#[display("Warframe player name must be nonempty, at most 128 bytes, and contain no NUL bytes")]
pub struct InvalidPlayerName;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_as_strings_without_normalization() -> Result<(), Box<dyn std::error::Error>>
    {
        for text in ["MiXeD.Name-42".into(), "x".repeat(128), "é".repeat(64)] {
            let name: PlayerName = text.parse()?;
            assert_eq!(name.as_str(), text);
            let value = serde_json::to_value(&name)?;
            assert_eq!(value, text);
            assert_eq!(serde_json::from_value::<PlayerName>(value)?, name);
            assert_eq!(String::from(name), text);
        }
        Ok(())
    }

    #[test]
    fn construction_and_deserialization_reject_invalid_names() {
        for text in [
            String::new(),
            "x".repeat(129),
            "é".repeat(65),
            "Player\0".into(),
        ] {
            assert_eq!(PlayerName::new(text.clone()), Err(InvalidPlayerName));
            assert!(serde_json::from_value::<PlayerName>(serde_json::json!(text)).is_err());
        }
        for value in [serde_json::Value::Null, serde_json::json!(123)] {
            assert!(serde_json::from_value::<PlayerName>(value).is_err());
        }
    }
}
