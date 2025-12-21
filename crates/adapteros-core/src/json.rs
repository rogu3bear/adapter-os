//! Reusable JSON serialization utilities for AdapterOS
//!
//! Provides consistent JSON serialization/deserialization with proper `AosError` mapping.
//!
//! # Examples
//!
//! ```rust
//! use adapteros_core::json::{JsonSerialize, JsonDeserialize};
//! use adapteros_core::{AosError, Result};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize)]
//! struct MyConfig {
//!     name: String,
//!     value: i32,
//! }
//!
//! impl JsonSerialize for MyConfig {
//!     fn error_context() -> &'static str {
//!         "config"
//!     }
//! }
//!
//! impl JsonDeserialize for MyConfig {
//!     fn error_context() -> &'static str {
//!         "config"
//!     }
//! }
//!
//! // Now use trait methods
//! let config = MyConfig { name: "test".to_string(), value: 42 };
//! let json = config.to_json()?; // Returns Result<String>
//! let parsed = MyConfig::from_json(&json)?; // Returns Result<MyConfig>
//! # Ok::<(), AosError>(())
//! ```

use crate::{AosError, Result};
use serde::{de::DeserializeOwned, Serialize};

/// Trait for types that can be serialized to JSON with consistent error handling
pub trait JsonSerialize: Serialize {
    /// Context string for error messages (e.g., "manifest", "config", "SBOM")
    fn error_context() -> &'static str;

    /// Serialize to pretty-printed JSON string
    fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| {
            AosError::Serialization(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize {}: {}", Self::error_context(), e),
            )))
        })
    }

    /// Serialize to compact JSON string (no pretty printing)
    fn to_json_compact(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| {
            AosError::Serialization(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize {}: {}", Self::error_context(), e),
            )))
        })
    }

    /// Serialize to JSON bytes
    fn to_json_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec_pretty(self).map_err(|e| {
            AosError::Serialization(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize {}: {}", Self::error_context(), e),
            )))
        })
    }
}

/// Trait for types that can be deserialized from JSON with consistent error handling
pub trait JsonDeserialize: DeserializeOwned {
    /// Context string for error messages (e.g., "manifest", "config", "SBOM")
    fn error_context() -> &'static str;

    /// Deserialize from JSON string
    fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| {
            AosError::Parse(format!("Failed to parse {}: {}", Self::error_context(), e))
        })
    }

    /// Deserialize from JSON bytes
    fn from_json_bytes(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| {
            AosError::Parse(format!("Failed to parse {}: {}", Self::error_context(), e))
        })
    }

    /// Deserialize from a reader
    fn from_json_reader<R: std::io::Read>(reader: R) -> Result<Self> {
        serde_json::from_reader(reader).map_err(|e| {
            AosError::Parse(format!("Failed to parse {}: {}", Self::error_context(), e))
        })
    }
}

/// Combined trait for types that support both serialization and deserialization
pub trait JsonSerde: JsonSerialize + JsonDeserialize {}

// Auto-implement JsonSerde for types that implement both traits
impl<T: JsonSerialize + JsonDeserialize> JsonSerde for T {}

/// Standalone function for quick JSON serialization with custom error context
pub fn to_json<T: Serialize>(value: &T, context: &str) -> Result<String> {
    serde_json::to_string_pretty(value).map_err(|e| {
        AosError::Serialization(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to serialize {}: {}", context, e),
        )))
    })
}

/// Standalone function for quick JSON deserialization with custom error context
pub fn from_json<T: DeserializeOwned>(json: &str, context: &str) -> Result<T> {
    serde_json::from_str(json)
        .map_err(|e| AosError::Parse(format!("Failed to parse {}: {}", context, e)))
}

/// Standalone function for quick JSON serialization to bytes with custom error context
pub fn to_json_bytes<T: Serialize>(value: &T, context: &str) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(value).map_err(|e| {
        AosError::Serialization(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to serialize {}: {}", context, e),
        )))
    })
}

/// Standalone function for quick JSON deserialization from bytes with custom error context
pub fn from_json_bytes<T: DeserializeOwned>(bytes: &[u8], context: &str) -> Result<T> {
    serde_json::from_slice(bytes)
        .map_err(|e| AosError::Parse(format!("Failed to parse {}: {}", context, e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestStruct {
        name: String,
        value: i32,
    }

    impl JsonSerialize for TestStruct {
        fn error_context() -> &'static str {
            "test struct"
        }
    }

    impl JsonDeserialize for TestStruct {
        fn error_context() -> &'static str {
            "test struct"
        }
    }

    #[test]
    fn test_serialize_deserialize() {
        let original = TestStruct {
            name: "test".to_string(),
            value: 42,
        };

        let json = original.to_json().unwrap();
        let parsed = TestStruct::from_json(&json).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_compact_json() {
        let test = TestStruct {
            name: "test".to_string(),
            value: 42,
        };

        let compact = test.to_json_compact().unwrap();
        assert!(!compact.contains('\n'));
    }

    #[test]
    fn test_bytes_roundtrip() {
        let original = TestStruct {
            name: "test".to_string(),
            value: 42,
        };

        let bytes = original.to_json_bytes().unwrap();
        let parsed = TestStruct::from_json_bytes(&bytes).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_standalone_functions() {
        let original = TestStruct {
            name: "test".to_string(),
            value: 42,
        };

        let json = to_json(&original, "test").unwrap();
        let parsed: TestStruct = from_json(&json, "test").unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_parse_error_context() {
        let result = TestStruct::from_json("invalid json");
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(err_str.contains("test struct"));
    }
}
