//! BLAKE3 hash newtype and utilities

use crate::error::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

/// BLAKE3 hash newtype (32 bytes)
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct B3Hash([u8; 32]);

impl B3Hash {
    /// Create a new B3Hash from bytes
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Create a new B3Hash from bytes (alias for new)
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Hash the given bytes
    pub fn hash(bytes: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(bytes);
        Self(*hasher.finalize().as_bytes())
    }

    /// Hash multiple byte slices
    pub fn hash_multi(slices: &[&[u8]]) -> Self {
        let mut hasher = blake3::Hasher::new();
        for slice in slices {
            hasher.update(slice);
        }
        Self(*hasher.finalize().as_bytes())
    }

    /// Hash a file
    pub fn hash_file<P: AsRef<std::path::Path>>(
        path: P,
    ) -> std::result::Result<Self, std::io::Error> {
        let contents = std::fs::read(path)?;
        Ok(Self::hash(&contents))
    }

    /// Get the raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Get the raw bytes (alias for as_bytes)
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    /// Convert to hex string (first 16 chars for display)
    pub fn to_short_hex(&self) -> String {
        hex::encode(&self.0[..8])
    }

    /// Convert to full hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Check if the hash is all zeros
    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }

    /// Parse from hex string
    pub fn from_hex(s: &str) -> Result<Self> {
        let bytes =
            hex::decode(s).map_err(|e| AosError::InvalidHash(format!("Invalid hex: {}", e)))?;
        if bytes.len() != 32 {
            return Err(AosError::InvalidHash(format!(
                "Expected 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    /// Zero hash placeholder (32 zero bytes)
    pub const fn zero() -> Self {
        Self([0u8; 32])
    }
}

impl Default for B3Hash {
    fn default() -> Self {
        Self::zero()
    }
}

impl fmt::Debug for B3Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "B3Hash({})", self.to_short_hex())
    }
}

impl fmt::Display for B3Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "b3:{}", self.to_short_hex())
    }
}

impl fmt::LowerHex for B3Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl Serialize for B3Hash {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for B3Hash {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        B3Hash::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "schemars-support")]
impl schemars::JsonSchema for B3Hash {
    fn schema_name() -> String {
        "B3Hash".to_string()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        let mut schema = gen.subschema_for::<String>();
        if let schemars::schema::Schema::Object(obj) = &mut schema {
            obj.string = Some(Box::new(schemars::schema::StringValidation {
                pattern: Some("[0-9a-f]{64}".to_string()),
                ..Default::default()
            }));
        }
        schema
    }
}

#[cfg(feature = "utoipa")]
impl utoipa::ToSchema for B3Hash {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("B3Hash")
    }
}

#[cfg(feature = "utoipa")]
impl utoipa::PartialSchema for B3Hash {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::SchemaType::Type(
                utoipa::openapi::schema::Type::String,
            ))
            .description(Some("BLAKE3 hash (64 hex characters)"))
            .pattern(Some("[0-9a-f]{64}"))
            .examples([serde_json::json!(
                "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
            )])
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_deterministic() {
        let data = b"hello world";
        let h1 = B3Hash::hash(data);
        let h2 = B3Hash::hash(data);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_multi() {
        let h1 = B3Hash::hash(b"ab");
        let h2 = B3Hash::hash_multi(&[b"a", b"b"]);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hex_roundtrip() {
        let h1 = B3Hash::hash(b"test");
        let hex = h1.to_hex();
        let h2 = B3Hash::from_hex(&hex).expect("Test hash should deserialize from hex");
        assert_eq!(h1, h2);
    }
}
