//! Adapter type classification
//!
//! Provides the `AdapterType` enum for classifying adapters into distinct categories:
//! - **Standard**: Portable adapters (.aos files) that can be freely shared and loaded
//! - **Codebase**: Stream-scoped adapters representing repo state + session context
//! - **Core**: Baseline adapters (like adapteros.aos) used as the base for codebase deltas
//!
//! # Codebase Adapter Rules
//!
//! Codebase adapters have special constraints:
//! - Must declare explicit `base_adapter_id` pointing to a core adapter
//! - Only one can be active per session (stream exclusivity)
//! - Auto-version on threshold or explicit versioning
//! - Require deployment verification (repo clean, manifest hash match)
//!
//! 【2025-01-29†prd-adapters†codebase_adapter_type】

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Classification of adapter types
///
/// Each type has different lifecycle rules, storage patterns, and routing constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AdapterType {
    /// Portable adapters (.aos files) that can be freely shared and loaded.
    /// These are the standard training outputs, downloadable from registries.
    #[default]
    Standard,

    /// Stream-scoped adapters representing repo state + session context.
    /// One per session, auto-versioned, requires base_adapter_id.
    Codebase,

    /// Baseline adapters (like adapteros.aos) used as the base for codebase deltas.
    /// Core adapters are stable reference points, rarely modified directly.
    Core,
}

impl AdapterType {
    /// Check if this adapter type requires a base adapter ID.
    ///
    /// Codebase adapters must declare an explicit `base_adapter_id` pointing
    /// to a core adapter that provides the baseline weights.
    pub fn requires_base_adapter(&self) -> bool {
        matches!(self, AdapterType::Codebase)
    }

    /// Check if this adapter type can be bound to a session.
    ///
    /// Only codebase adapters can be exclusively bound to sessions.
    pub fn can_bind_to_session(&self) -> bool {
        matches!(self, AdapterType::Codebase)
    }

    /// Check if this adapter type supports auto-versioning.
    ///
    /// Codebase adapters auto-version when activation count exceeds threshold.
    pub fn supports_auto_versioning(&self) -> bool {
        matches!(self, AdapterType::Codebase)
    }

    /// Check if this adapter type requires deployment verification.
    ///
    /// Codebase adapters require repo clean state and manifest hash match.
    pub fn requires_deployment_verification(&self) -> bool {
        matches!(self, AdapterType::Codebase)
    }

    /// Convert to database string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            AdapterType::Standard => "standard",
            AdapterType::Codebase => "codebase",
            AdapterType::Core => "core",
        }
    }
}

impl fmt::Display for AdapterType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for AdapterType {
    type Err = AdapterTypeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "standard" => Ok(AdapterType::Standard),
            "codebase" => Ok(AdapterType::Codebase),
            "core" => Ok(AdapterType::Core),
            _ => Err(AdapterTypeParseError(s.to_string())),
        }
    }
}

impl From<&str> for AdapterType {
    fn from(s: &str) -> Self {
        match s.parse() {
            Ok(t) => t,
            Err(_) => {
                tracing::warn!(
                    input = %s,
                    default = "Standard",
                    "Invalid adapter type string, defaulting to Standard"
                );
                AdapterType::Standard
            }
        }
    }
}

impl From<Option<&str>> for AdapterType {
    fn from(s: Option<&str>) -> Self {
        match s {
            Some(val) => AdapterType::from(val),
            None => {
                tracing::debug!(
                    default = "Standard",
                    "No adapter type string provided, defaulting to Standard"
                );
                AdapterType::Standard
            }
        }
    }
}

impl From<Option<String>> for AdapterType {
    fn from(s: Option<String>) -> Self {
        match s {
            Some(ref val) => AdapterType::from(val.as_str()),
            None => {
                tracing::debug!(
                    default = "Standard",
                    "No adapter type string provided, defaulting to Standard"
                );
                AdapterType::Standard
            }
        }
    }
}

/// Error returned when parsing an invalid adapter type string.
#[derive(Debug, Clone)]
pub struct AdapterTypeParseError(pub String);

impl fmt::Display for AdapterTypeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Invalid adapter type '{}'. Expected 'standard', 'codebase', or 'core'",
            self.0
        )
    }
}

impl std::error::Error for AdapterTypeParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_type_default() {
        assert_eq!(AdapterType::default(), AdapterType::Standard);
    }

    #[test]
    fn test_adapter_type_parse() {
        assert_eq!(
            "standard".parse::<AdapterType>().unwrap(),
            AdapterType::Standard
        );
        assert_eq!(
            "codebase".parse::<AdapterType>().unwrap(),
            AdapterType::Codebase
        );
        assert_eq!("core".parse::<AdapterType>().unwrap(), AdapterType::Core);
        assert_eq!(
            "STANDARD".parse::<AdapterType>().unwrap(),
            AdapterType::Standard
        );
        assert!("invalid".parse::<AdapterType>().is_err());
    }

    #[test]
    fn test_adapter_type_from_option() {
        assert_eq!(AdapterType::from(None::<&str>), AdapterType::Standard);
        assert_eq!(AdapterType::from(Some("codebase")), AdapterType::Codebase);
        assert_eq!(AdapterType::from(Some("invalid")), AdapterType::Standard);
    }

    #[test]
    fn test_requires_base_adapter() {
        assert!(!AdapterType::Standard.requires_base_adapter());
        assert!(AdapterType::Codebase.requires_base_adapter());
        assert!(!AdapterType::Core.requires_base_adapter());
    }

    #[test]
    fn test_can_bind_to_session() {
        assert!(!AdapterType::Standard.can_bind_to_session());
        assert!(AdapterType::Codebase.can_bind_to_session());
        assert!(!AdapterType::Core.can_bind_to_session());
    }

    #[test]
    fn test_supports_auto_versioning() {
        assert!(!AdapterType::Standard.supports_auto_versioning());
        assert!(AdapterType::Codebase.supports_auto_versioning());
        assert!(!AdapterType::Core.supports_auto_versioning());
    }

    #[test]
    fn test_serde_roundtrip() {
        let types = [
            AdapterType::Standard,
            AdapterType::Codebase,
            AdapterType::Core,
        ];
        for t in types {
            let json = serde_json::to_string(&t).unwrap();
            let parsed: AdapterType = serde_json::from_str(&json).unwrap();
            assert_eq!(t, parsed);
        }
    }

    #[test]
    fn test_display() {
        assert_eq!(AdapterType::Standard.to_string(), "standard");
        assert_eq!(AdapterType::Codebase.to_string(), "codebase");
        assert_eq!(AdapterType::Core.to_string(), "core");
    }
}
