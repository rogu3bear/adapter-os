//! Shared serde helpers for hex encoding of byte arrays.
//!
//! This module provides reusable serde serialization/deserialization helpers
//! for fixed-size byte arrays using hex encoding. Use with `#[serde(with = "...")]`.
//!
//! # Available Modules
//!
//! - [`hex_bytes`] - For `[u8; 32]` arrays (hashes, keys, seeds)
//! - [`hex_bytes_64`] - For `[u8; 64]` arrays (signatures)
//! - [`option_hex_bytes`] - For `Option<[u8; 32]>` optional arrays
//! - [`hex_bytes_16`] - For `[u8; 16]` arrays (CPIDs, UUIDs)
//!
//! # Example
//!
//! ```rust
//! use serde::{Deserialize, Serialize};
//! use adapteros_core::serde_helpers;
//!
//! #[derive(Serialize, Deserialize)]
//! struct MyStruct {
//!     #[serde(with = "serde_helpers::hex_bytes")]
//!     hash: [u8; 32],
//!
//!     #[serde(with = "serde_helpers::hex_bytes_64")]
//!     signature: [u8; 64],
//!
//!     #[serde(with = "serde_helpers::option_hex_bytes")]
//!     optional_hash: Option<[u8; 32]>,
//! }
//! ```

/// Hex encoding for `[u8; 32]` arrays.
///
/// Use with `#[serde(with = "serde_helpers::hex_bytes")]` for fields like
/// BLAKE3 hashes, Ed25519 public keys, or 256-bit seeds.
pub mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize a 32-byte array as a hex string.
    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    /// Deserialize a hex string into a 32-byte array.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}

/// Hex encoding for `[u8; 64]` arrays.
///
/// Use with `#[serde(with = "serde_helpers::hex_bytes_64")]` for fields like
/// Ed25519 signatures or 512-bit values.
pub mod hex_bytes_64 {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize a 64-byte array as a hex string.
    pub fn serialize<S>(bytes: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    /// Deserialize a hex string into a 64-byte array.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 64], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 64 bytes"))
    }
}

/// Hex encoding for `[u8; 16]` arrays.
///
/// Use with `#[serde(with = "serde_helpers::hex_bytes_16")]` for fields like
/// CPIDs, UUIDs, or 128-bit values.
pub mod hex_bytes_16 {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize a 16-byte array as a hex string.
    pub fn serialize<S>(bytes: &[u8; 16], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    /// Deserialize a hex string into a 16-byte array.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 16], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 16 bytes"))
    }
}

/// Hex encoding for `Option<[u8; 32]>` optional arrays.
///
/// Use with `#[serde(with = "serde_helpers::option_hex_bytes")]` for optional
/// 32-byte fields. Serializes `Some(bytes)` as a hex string and `None` as null.
pub mod option_hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize an optional 32-byte array as a hex string or null.
    pub fn serialize<S>(bytes: &Option<[u8; 32]>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match bytes {
            Some(b) => serializer.serialize_some(&hex::encode(b)),
            None => serializer.serialize_none(),
        }
    }

    /// Deserialize a hex string or null into an optional 32-byte array.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<[u8; 32]>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => {
                let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
                let arr: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| serde::de::Error::custom("expected 32 bytes"))?;
                Ok(Some(arr))
            }
            None => Ok(None),
        }
    }
}

/// Hex encoding for `Option<[u8; 64]>` optional arrays.
///
/// Use with `#[serde(with = "serde_helpers::option_hex_bytes_64")]` for optional
/// 64-byte fields like optional signatures.
pub mod option_hex_bytes_64 {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize an optional 64-byte array as a hex string or null.
    pub fn serialize<S>(bytes: &Option<[u8; 64]>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match bytes {
            Some(b) => serializer.serialize_some(&hex::encode(b)),
            None => serializer.serialize_none(),
        }
    }

    /// Deserialize a hex string or null into an optional 64-byte array.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<[u8; 64]>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => {
                let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
                let arr: [u8; 64] = bytes
                    .try_into()
                    .map_err(|_| serde::de::Error::custom("expected 64 bytes"))?;
                Ok(Some(arr))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestStruct32 {
        #[serde(with = "hex_bytes")]
        hash: [u8; 32],
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestStruct64 {
        #[serde(with = "hex_bytes_64")]
        signature: [u8; 64],
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestStruct16 {
        #[serde(with = "hex_bytes_16")]
        cpid: [u8; 16],
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestStructOpt {
        #[serde(with = "option_hex_bytes")]
        optional: Option<[u8; 32]>,
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestStructOpt64 {
        #[serde(with = "option_hex_bytes_64")]
        optional: Option<[u8; 64]>,
    }

    #[test]
    fn test_hex_bytes_32_roundtrip() {
        let original = TestStruct32 {
            hash: [0xab; 32],
        };
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("abababab")); // hex encoded
        let decoded: TestStruct32 = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_hex_bytes_64_roundtrip() {
        let original = TestStruct64 {
            signature: [0xcd; 64],
        };
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("cdcdcdcd")); // hex encoded
        let decoded: TestStruct64 = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_hex_bytes_16_roundtrip() {
        let original = TestStruct16 {
            cpid: [0xef; 16],
        };
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("efefefef")); // hex encoded
        let decoded: TestStruct16 = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_option_hex_bytes_some() {
        let original = TestStructOpt {
            optional: Some([0x12; 32]),
        };
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("12121212")); // hex encoded
        let decoded: TestStructOpt = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_option_hex_bytes_none() {
        let original = TestStructOpt { optional: None };
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("null"));
        let decoded: TestStructOpt = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_option_hex_bytes_64_some() {
        let original = TestStructOpt64 {
            optional: Some([0x34; 64]),
        };
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("34343434")); // hex encoded
        let decoded: TestStructOpt64 = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_invalid_hex_string() {
        let json = r#"{"hash": "not_valid_hex!"}"#;
        let result: Result<TestStruct32, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_length() {
        // Only 16 bytes of hex (8 bytes)
        let json = r#"{"hash": "abababababababab"}"#;
        let result: Result<TestStruct32, _> = serde_json::from_str(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("32 bytes"));
    }
}
