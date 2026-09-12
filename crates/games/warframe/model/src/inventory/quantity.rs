//! Decimal-string encoding for inventory quantities.

use serde::{Deserialize, Deserializer, Serializer, de, ser};

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde with adapter signature"
)]
pub(super) fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    if *value == 0 {
        return Err(ser::Error::custom("inventory quantity must be positive"));
    }
    serializer.serialize_str(&value.to_string())
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = String::deserialize(deserializer)?;
    if value.is_empty()
        || value.len() > 20
        || value.starts_with('0')
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(de::Error::custom(
            "expected a positive canonical decimal u64 string",
        ));
    }
    value.parse().map_err(de::Error::custom)
}
