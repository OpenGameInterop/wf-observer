use derive_more::{Display, Error};
use serde::{Deserialize, Deserializer, Serialize, de};
use std::{fmt, str::FromStr};

/// Opaque Warframe account identifier: 24 lowercase hexadecimal characters.
#[derive(Clone, Hash, Serialize, ..Ord)]
#[serde(transparent)]
pub struct AccountId(String);

impl AccountId {
    /// Validates a game account identifier without normalizing it.
    ///
    /// # Errors
    ///
    /// Rejects values other than 24 lowercase hexadecimal characters.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidAccountId> {
        let value = value.into();
        if value.len() == 24
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            Ok(Self(value))
        } else {
            Err(InvalidAccountId)
        }
    }

    /// Borrows the account identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<AccountId> for String {
    fn from(value: AccountId) -> Self {
        value.0
    }
}

impl FromStr for AccountId {
    type Err = InvalidAccountId;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for AccountId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl fmt::Debug for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted account>")
    }
}

/// Rejected account identifier; diagnostics do not retain the supplied value.
#[derive(Debug, Display, Error, ..Copy, ..Eq)]
#[display("Warframe account identifier must contain 24 lowercase hexadecimal characters")]
pub struct InvalidAccountId;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_ids_round_trip_as_strings_with_redacted_debug()
    -> Result<(), Box<dyn std::error::Error>> {
        let text = "0123456789abcdef01234567";
        let id: AccountId = text.parse()?;
        let json = serde_json::to_value(&id)?;
        assert_eq!(json, text);
        assert_eq!(serde_json::from_value::<AccountId>(json)?, id);
        assert_eq!(id.as_str(), text);
        assert_eq!(format!("{id:?}"), "<redacted account>");
        assert_eq!(String::from(id), text);
        Ok(())
    }

    #[test]
    fn construction_and_deserialization_reject_invalid_ids() {
        for text in [
            "",
            "0123456789abcdef0123456",
            "0123456789abcdef012345678",
            "0123456789ABCDEF01234567",
            "0123456789abcdef0123456g",
            "0123456789abcdef0123456\0",
            "éééééééééééé",
        ] {
            assert_eq!(AccountId::new(text), Err(InvalidAccountId));
            assert!(serde_json::from_value::<AccountId>(serde_json::json!(text)).is_err());
        }
        for value in [serde_json::Value::Null, serde_json::json!(123)] {
            assert!(serde_json::from_value::<AccountId>(value).is_err());
        }
    }
}
