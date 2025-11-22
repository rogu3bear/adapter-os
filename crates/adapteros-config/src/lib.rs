//! AdapterOS Deterministic Configuration System
//!
//! This crate provides a deterministic configuration system with strict precedence rules:
//! CLI arguments > Environment variables (.env file supported) > Manifest file
//!
//! Once frozen at startup, configuration becomes immutable and all environment
//! variable access is banned to ensure deterministic behavior.
//!
//! # Environment Configuration
//!
//! Create a `.env` file in your project root:
//!
//! ```env
//! AOS_MODEL_PATH=./models/qwen2.5-7b-mlx
//! AOS_MODEL_BACKEND=auto
//! ```

pub mod guards;
pub mod loader;
pub mod model;
pub mod precedence;
pub mod schema;
pub mod types;

pub use guards::{ConfigGuards, FeatureFlags};
pub use loader::ConfigLoader;
pub use model::{
    get_model_path_optional, get_model_path_with_fallback, is_model_path_configured, load_dotenv,
    BackendPreference, ModelConfig,
};
pub use precedence::DeterministicConfig;
pub use schema::{
    default_schema, parse_bool, validate_value, ConfigSchema, ConfigType, ConfigVariable,
    DeprecationInfo, ValidationError,
};
pub use types::*;

use adapteros_core::{AosError, Result};
use std::sync::OnceLock;

/// Global configuration instance, frozen after first access
static CONFIG: OnceLock<DeterministicConfig> = OnceLock::new();

/// Initialize the global configuration from CLI args, environment, and manifest
///
/// Automatically loads `.env` file before reading environment variables.
pub fn initialize_config(
    cli_args: Vec<String>,
    manifest_path: Option<String>,
) -> Result<&'static DeterministicConfig> {
    // Load .env file first
    load_dotenv();

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
