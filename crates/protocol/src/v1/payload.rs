//! Valid JSON carried as text: postcard cannot deserialize an untyped JSON value.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Syntactically validated JSON text, without a game-specific schema.
///
/// Construction/deserialization validates JSON syntax, not the topic schema or
/// host size limits. Transport must bound frames before decoding; the host bounds
/// payloads before accepting publication. Large integer IDs/quantities must use
/// decimal strings in topic schemas when they exceed JavaScript's exact range.
#[derive(Debug, Clone, ..Eq, ..Serde)]
#[serde(transparent)]
pub struct JsonPayload(#[serde(deserialize_with = "deserialize_json")] String);

impl JsonPayload {
    /// Encodes a domain value as JSON text.
    ///
    /// # Errors
    ///
    /// Returns an error if JSON serialization fails.
    pub fn from_value(value: &Value) -> Result<Self, serde_json::Error> {
        serde_json::to_string(value).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<JsonPayload> for String {
    fn from(value: JsonPayload) -> Self {
        value.0
    }
}

impl TryFrom<String> for JsonPayload {
    type Error = serde_json::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        serde_json::from_str::<Value>(&value)?;
        Ok(Self(value))
    }
}

fn deserialize_json<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let text = String::deserialize(deserializer)?;
    JsonPayload::try_from(text)
        .map(String::from)
        .map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::JsonPayload;

    #[test]
    fn rejects_invalid_json_on_construction_and_wire_decode() -> Result<(), postcard::Error> {
        let invalid = "{".to_owned();
        assert!(JsonPayload::try_from(invalid.clone()).is_err());
        let encoded = postcard::to_stdvec(&invalid)?;
        assert!(postcard::from_bytes::<JsonPayload>(&encoded).is_err());
        Ok(())
    }
}
