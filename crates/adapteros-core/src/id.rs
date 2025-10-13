//! Checkpoint ID (CPID) type

use crate::error::{AosError, Result};
use crate::hash::B3Hash;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Checkpoint ID - shortened display identifier
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CPID([u8; 16]);

impl CPID {
    /// Create a new CPID from bytes
    pub fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Derive CPID from a hash (take first 16 bytes)
    pub fn from_hash(hash: &B3Hash) -> Self {
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&hash.as_bytes()[..16]);
        Self(bytes)
    }

    /// Generate random CPID (for testing)
    #[cfg(test)]
    pub fn random() -> Self {
        use rand::Rng;
        let mut bytes = [0u8; 16];
        rand::thread_rng().fill(&mut bytes);
        Self(bytes)
    }

    /// Get the raw bytes
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse from hex string
    pub fn from_hex(s: &str) -> Result<Self> {
        let bytes =
            hex::decode(s).map_err(|e| AosError::InvalidCPID(format!("Invalid hex: {}", e)))?;
        if bytes.len() != 16 {
            return Err(AosError::InvalidCPID(format!(
                "Expected 16 bytes, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 16];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }
}

impl fmt::Debug for CPID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CPID({})", hex::encode(&self.0[..4]))
    }
}

impl fmt::Display for CPID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl Serialize for CPID {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for CPID {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        CPID::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "schemars-support")]
impl schemars::JsonSchema for CPID {
    fn schema_name() -> String {
        "CPID".to_string()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        let mut schema = gen.subschema_for::<String>();
        if let schemars::schema::Schema::Object(obj) = &mut schema {
            obj.string = Some(Box::new(schemars::schema::StringValidation {
                pattern: Some("[0-9a-f]{32}".to_string()),
                ..Default::default()
            }));
        }
        schema
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpid_from_hash() {
        let hash = B3Hash::hash(b"test");
        let cpid = CPID::from_hash(&hash);
        assert_eq!(cpid.as_bytes().len(), 16);
    }

    #[test]
    fn test_hex_roundtrip() {
        let cpid = CPID::from_hash(&B3Hash::hash(b"test"));
        let hex = cpid.to_hex();
        let cpid2 = CPID::from_hex(&hex).expect("Test CPID should deserialize from hex");
        assert_eq!(cpid, cpid2);
    }
}
