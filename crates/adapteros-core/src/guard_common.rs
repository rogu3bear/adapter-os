//! Common types for RAII resource guards
//!
//! This module provides shared types used by the various RAII guards:
//! - `AdapterUseGuard` in the worker crate
//! - `SeedScopeGuard` in this crate
//! - `TrainingJobGuard` in the orchestrator crate

use serde::{Deserialize, Serialize};
use std::env;
use std::str::FromStr;

/// Log level for guard cleanup operations.
///
/// Controls the verbosity of logging when a guard performs cleanup
/// on an error path (i.e., when the guarded operation did not complete normally).
///
/// # Configuration
///
/// Can be configured via:
/// - Environment variables: `AOS_GUARD_ADAPTER_LOG`, `AOS_GUARD_SEED_LOG`, `AOS_GUARD_TRAINING_LOG`
/// - TOML config: `[guards]` section
///
/// # Defaults
///
/// - `AdapterUseGuard`: `Warn` (resource leaks are serious)
/// - `SeedScopeGuard`: `Debug` (quieter, for determinism debugging)
/// - `TrainingJobGuard`: `Warn` (zombie jobs need visibility)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuardLogLevel {
    /// Log cleanup at warn level (high visibility, default for most guards)
    #[default]
    Warn,
    /// Log cleanup at debug level (quieter, for determinism debugging)
    Debug,
    /// Disable cleanup logging entirely
    Off,
}

impl GuardLogLevel {
    /// Parse from environment variable value.
    ///
    /// Accepts: "warn", "debug", "off" (case-insensitive)
    pub fn from_env_value(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "warn" => Some(Self::Warn),
            "debug" => Some(Self::Debug),
            "off" => Some(Self::Off),
            _ => None,
        }
    }

    /// Get the log level from an environment variable with a default fallback.
    ///
    /// # Arguments
    /// * `env_var` - The environment variable name (e.g., `AOS_GUARD_ADAPTER_LOG`)
    /// * `default` - The default value if the env var is not set or invalid
    pub fn from_env_or(env_var: &str, default: Self) -> Self {
        env::var(env_var)
            .ok()
            .and_then(|v| Self::from_env_value(&v))
            .unwrap_or(default)
    }
}

impl FromStr for GuardLogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_env_value(s).ok_or_else(|| {
            format!(
                "invalid guard log level '{}', expected one of: warn, debug, off",
                s
            )
        })
    }
}

impl std::fmt::Display for GuardLogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Warn => write!(f, "warn"),
            Self::Debug => write!(f, "debug"),
            Self::Off => write!(f, "off"),
        }
    }
}

/// Configuration for RAII resource guards.
///
/// Controls logging behavior for guard cleanup operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardConfig {
    /// Log level for AdapterUseGuard cleanup warnings.
    /// Default: `Warn` (adapter leaks are serious issues)
    #[serde(default = "default_adapter_guard_level")]
    pub adapter_guard_level: GuardLogLevel,

    /// Log level for SeedScopeGuard cleanup.
    /// Default: `Debug` (seed issues are mainly for determinism debugging)
    #[serde(default = "default_seed_guard_level")]
    pub seed_guard_level: GuardLogLevel,

    /// Log level for TrainingJobGuard cleanup.
    /// Default: `Warn` (zombie jobs need visibility)
    #[serde(default = "default_training_guard_level")]
    pub training_guard_level: GuardLogLevel,
}

fn default_adapter_guard_level() -> GuardLogLevel {
    GuardLogLevel::from_env_or("AOS_GUARD_ADAPTER_LOG", GuardLogLevel::Warn)
}

fn default_seed_guard_level() -> GuardLogLevel {
    GuardLogLevel::from_env_or("AOS_GUARD_SEED_LOG", GuardLogLevel::Debug)
}

fn default_training_guard_level() -> GuardLogLevel {
    GuardLogLevel::from_env_or("AOS_GUARD_TRAINING_LOG", GuardLogLevel::Warn)
}

impl Default for GuardConfig {
    fn default() -> Self {
        Self {
            adapter_guard_level: default_adapter_guard_level(),
            seed_guard_level: default_seed_guard_level(),
            training_guard_level: default_training_guard_level(),
        }
    }
}

impl GuardConfig {
    /// Create a new GuardConfig from environment variables.
    ///
    /// Environment variables:
    /// - `AOS_GUARD_ADAPTER_LOG`: warn|debug|off (default: warn)
    /// - `AOS_GUARD_SEED_LOG`: warn|debug|off (default: debug)
    /// - `AOS_GUARD_TRAINING_LOG`: warn|debug|off (default: warn)
    pub fn from_env() -> Self {
        Self::default()
    }

    /// Create a silent config (all guards log at Off level).
    /// Useful for tests where guard logging would be noisy.
    pub fn silent() -> Self {
        Self {
            adapter_guard_level: GuardLogLevel::Off,
            seed_guard_level: GuardLogLevel::Off,
            training_guard_level: GuardLogLevel::Off,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_log_level_from_str() {
        assert_eq!(
            "warn".parse::<GuardLogLevel>().unwrap(),
            GuardLogLevel::Warn
        );
        assert_eq!(
            "debug".parse::<GuardLogLevel>().unwrap(),
            GuardLogLevel::Debug
        );
        assert_eq!("off".parse::<GuardLogLevel>().unwrap(), GuardLogLevel::Off);
        assert_eq!(
            "WARN".parse::<GuardLogLevel>().unwrap(),
            GuardLogLevel::Warn
        );
        assert!("invalid".parse::<GuardLogLevel>().is_err());
    }

    #[test]
    fn test_guard_log_level_display() {
        assert_eq!(GuardLogLevel::Warn.to_string(), "warn");
        assert_eq!(GuardLogLevel::Debug.to_string(), "debug");
        assert_eq!(GuardLogLevel::Off.to_string(), "off");
    }

    #[test]
    fn test_guard_config_default() {
        let config = GuardConfig::default();
        // Note: defaults depend on env vars, so we just check they're valid
        assert!(matches!(
            config.adapter_guard_level,
            GuardLogLevel::Warn | GuardLogLevel::Debug | GuardLogLevel::Off
        ));
    }

    #[test]
    fn test_guard_config_silent() {
        let config = GuardConfig::silent();
        assert_eq!(config.adapter_guard_level, GuardLogLevel::Off);
        assert_eq!(config.seed_guard_level, GuardLogLevel::Off);
        assert_eq!(config.training_guard_level, GuardLogLevel::Off);
    }

    #[test]
    fn test_guard_log_level_serde() {
        let level = GuardLogLevel::Debug;
        let serialized = serde_json::to_string(&level).unwrap();
        assert_eq!(serialized, "\"debug\"");

        let deserialized: GuardLogLevel = serde_json::from_str("\"warn\"").unwrap();
        assert_eq!(deserialized, GuardLogLevel::Warn);
    }
}
