//! Valid JSON carried as text: postcard cannot deserialize an untyped JSON value.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::sync::{Arc, OnceLock};

/// Syntactically validated JSON text, without a game-specific schema.
///
/// Construction/deserialization validates JSON syntax, not the topic schema or
/// host size limits. Transport must bound frames before decoding; the host bounds
/// payloads before accepting publication. Large integer IDs/quantities must use
/// decimal strings in topic schemas when they exceed JavaScript's exact range.
#[derive(Debug, Clone)]
pub struct JsonPayload(Arc<Payload>);

#[derive(Debug)]
struct Payload {
    text: String,
    hash: OnceLock<[u8; 32]>,
    patch: OnceLock<([u8; 32], JsonPayload)>,
}

impl JsonPayload {
    /// Encodes a domain value as JSON text.
    ///
    /// # Errors
    ///
    /// Returns an error if JSON serialization fails.
    pub fn from_value(value: &Value) -> Result<Self, serde_json::Error> {
        serde_json::to_string(value).map(Self::new)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0.text
    }

    /// Content identity, computed once and shared with cloned payloads.
    #[must_use]
    pub fn hash(&self) -> [u8; 32] {
        *self
            .0
            .hash
            .get_or_init(|| *blake3::hash(self.as_str().as_bytes()).as_bytes())
    }

    pub(super) fn patch_from(&self, base: &Self) -> Result<Self, serde_json::Error> {
        if let Some((hash, patch)) = self.0.patch.get()
            && *hash == base.hash()
        {
            return Ok(patch.clone());
        }
        let before = serde_json::from_str(base.as_str())?;
        let after = serde_json::from_str(self.as_str())?;
        let patch = Self::new(serde_json::to_string(&json_patch::diff(&before, &after))?);
        // Share one useful patch across clients with the same baseline, without
        // retaining the old snapshot or accumulating per-client patch histories.
        if patch.as_str().len() < self.as_str().len() {
            let _ = self.0.patch.set((base.hash(), patch.clone()));
        }
        Ok(patch)
    }

    fn new(text: String) -> Self {
        Self(Arc::new(Payload {
            text,
            hash: OnceLock::new(),
            patch: OnceLock::new(),
        }))
    }
}

impl PartialEq for JsonPayload {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.as_str() == other.as_str()
    }
}

impl Eq for JsonPayload {}

impl From<JsonPayload> for String {
    fn from(value: JsonPayload) -> Self {
        Arc::try_unwrap(value.0).map_or_else(|payload| payload.text.clone(), |payload| payload.text)
    }
}

impl TryFrom<String> for JsonPayload {
    type Error = serde_json::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        serde_json::from_str::<Value>(&value)?;
        Ok(Self::new(value))
    }
}

impl Serialize for JsonPayload {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for JsonPayload {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
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
