//! AdapterOS Deterministic Configuration System
//!
//! This crate provides a deterministic configuration system with strict precedence rules:
//! CLI arguments > Environment variables > Manifest file
//!
//! Once frozen at startup, configuration becomes immutable and all environment
//! variable access is banned to ensure deterministic behavior.

pub mod guards;
pub mod loader;
pub mod precedence;
pub mod types;

pub use guards::{ConfigGuards, FeatureFlags};
pub use loader::ConfigLoader;
pub use precedence::DeterministicConfig;
pub use types::*;

use adapteros_core::{AosError, Result};
use std::sync::OnceLock;

/// Global configuration instance, frozen after first access
static CONFIG: OnceLock<DeterministicConfig> = OnceLock::new();

/// Initialize the global configuration from CLI args, environment, and manifest
pub fn initialize_config(
    cli_args: Vec<String>,
    manifest_path: Option<String>,
) -> Result<&'static DeterministicConfig> {
    let loader = ConfigLoader::new();
    let config = loader.load(cli_args, manifest_path)?;

    CONFIG
        .set(config)
        .map_err(|_| AosError::Config("Configuration already initialized".to_string()))?;

    Ok(CONFIG.get().unwrap())
}

/// Get the frozen global configuration
pub fn get_config() -> Result<&'static DeterministicConfig> {
    CONFIG
        .get()
        .ok_or_else(|| AosError::Config("Configuration not initialized".to_string()))
}

/// Check if configuration is frozen
pub fn is_frozen() -> bool {
    CONFIG.get().is_some()
}
