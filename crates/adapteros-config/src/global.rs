//! Global runtime configuration access
//!
//! Provides a globally accessible, thread-safe runtime configuration using OnceLock.
//! The configuration is initialized once at startup and remains immutable.

use crate::runtime::RuntimeConfig;
use adapteros_core::{AosError, Result};
use std::sync::OnceLock;
use tracing::{info, warn};

/// Global runtime configuration instance
static RUNTIME_CONFIG: OnceLock<RuntimeConfig> = OnceLock::new();

/// Configuration error for initialization failures
#[derive(Debug)]
pub struct ConfigError {
    /// Error message
    pub message: String,
    /// Validation errors from runtime config
    pub validation_errors: Vec<String>,
    /// Unknown variables
    pub unknown_vars: Vec<String>,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Configuration Error: {}", self.message)?;

        if !self.validation_errors.is_empty() {
            writeln!(f, "\nValidation Errors:")?;
            for err in &self.validation_errors {
                writeln!(f, "  - {}", err)?;
            }
        }

        if !self.unknown_vars.is_empty() {
            writeln!(f, "\nUnknown AOS_* Variables:")?;
            for var in &self.unknown_vars {
                writeln!(f, "  - {}", var)?;
            }
        }

        Ok(())
    }
}

impl std::error::Error for ConfigError {}

/// Initialize the global runtime configuration
///
/// This should be called once at application startup, typically after tracing is initialized.
///
/// # Behavior by Mode
///
/// - **Production mode**: Fails on validation errors, warns on unknown vars
/// - **Development mode**: Warns on errors and unknown vars, continues execution
///
/// # Example
///
/// ```rust,no_run
/// use adapteros_config::global::init_runtime_config;
///
/// // Initialize tracing first...
///
/// if let Err(e) = init_runtime_config() {
///     eprintln!("Configuration Error:\n\n{}", e);
///     std::process::exit(1);
/// }
/// ```
pub fn init_runtime_config() -> std::result::Result<(), ConfigError> {
    let config = RuntimeConfig::from_env().map_err(|e| ConfigError {
        message: e.to_string(),
        validation_errors: vec![],
        unknown_vars: vec![],
    })?;

    // Log configuration hash for reproducibility
    info!(hash = %config.hash(), "Configuration loaded");

    // Check for deprecated variables
    for (name, replacement, notes) in config.deprecated_vars_in_use() {
        if notes.is_empty() {
            warn!(
                variable = %name,
                replacement = %replacement,
                "Deprecated configuration variable in use"
            );
        } else {
            warn!(
                variable = %name,
                replacement = %replacement,
                notes = %notes,
                "Deprecated configuration variable in use"
            );
        }
    }

    // Handle unknown variables
    if config.has_unknown_vars() {
        for var in config.unknown_vars() {
            warn!(variable = %var, "Unknown AOS_* environment variable");
        }
    }

    // Handle validation errors based on runtime mode
    if config.has_errors() {
        let is_production = config.is_production_mode() || config.runtime_mode() == "production";

        if is_production {
            return Err(ConfigError {
                message: "Configuration validation failed in production mode".to_string(),
                validation_errors: config.validation_errors().to_vec(),
                unknown_vars: config.unknown_vars().to_vec(),
            });
        } else {
            for err in config.validation_errors() {
                warn!(error = %err, "Configuration validation error (development mode)");
            }
        }
    }

    RUNTIME_CONFIG.set(config).map_err(|_| ConfigError {
        message: "Runtime configuration already initialized".to_string(),
        validation_errors: vec![],
        unknown_vars: vec![],
    })?;

    Ok(())
}

/// Get the global runtime configuration
///
/// Returns a reference to the initialized runtime configuration.
/// Panics if configuration has not been initialized.
///
/// # Panics
///
/// Panics if `init_runtime_config()` has not been called.
pub fn config() -> &'static RuntimeConfig {
    RUNTIME_CONFIG
        .get()
        .expect("Global runtime configuration access failed: RUNTIME_CONFIG OnceCell is not initialized. Expected state: init_runtime_config() should have been called during application bootstrap. Context: Configuration must be initialized before any code attempts to access it via config(). Use try_config() if initialization status is uncertain.")
}

/// Try to get the global runtime configuration
///
/// Returns `Some(&RuntimeConfig)` if initialized, `None` otherwise.
pub fn try_config() -> Option<&'static RuntimeConfig> {
    RUNTIME_CONFIG.get()
}

/// Check if runtime configuration is initialized
pub fn is_initialized() -> bool {
    RUNTIME_CONFIG.get().is_some()
}

/// Get runtime configuration or initialize with defaults
///
/// This is useful for tests and CLI tools that may not have full configuration.
pub fn config_or_default() -> Result<&'static RuntimeConfig> {
    if !is_initialized() {
        init_runtime_config().map_err(|e| AosError::Config(e.to_string()))?;
    }
    Ok(config())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests may interfere with each other due to OnceLock
    // They should be run with -- --test-threads=1 if needed

    #[test]
    fn test_config_error_display() {
        let err = ConfigError {
            message: "Test error".to_string(),
            validation_errors: vec!["Error 1".to_string(), "Error 2".to_string()],
            unknown_vars: vec!["AOS_UNKNOWN".to_string()],
        };

        let display = format!("{}", err);
        assert!(display.contains("Test error"));
        assert!(display.contains("Error 1"));
        assert!(display.contains("AOS_UNKNOWN"));
    }
}
