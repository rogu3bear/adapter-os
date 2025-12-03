//! AdapterOS Deterministic Configuration System
//!
//! This crate provides a deterministic configuration system with strict precedence rules:
//! CLI arguments > Environment variables (.env file supported) > Manifest file (TOML)
//!
//! Once frozen at startup, configuration becomes immutable and all environment
//! variable access is banned to ensure deterministic behavior.
//!
//! # Environment Configuration
//!
//! Create a `.env` file in your project root:
//!
//! ```env
//! AOS_MODEL_PATH=./var/model-cache/models/qwen2.5-7b-instruct-bf16
//! AOS_MODEL_BACKEND=auto
//! AOS_SERVER_PORT=8080
//! ```
//!
//! # EffectiveConfig (Recommended)
//!
//! Use `init_effective_config()` for unified configuration with type-safe sections:
//!
//! ```rust,ignore
//! use adapteros_config::{init_effective_config, effective_config};
//!
//! // Initialize at startup
//! init_effective_config(Some("configs/cp.toml"), vec![])?;
//!
//! // Access anywhere
//! let cfg = effective_config()?;
//! println!("Port: {}", cfg.server.port);
//! ```

pub mod effective;
pub mod global;
pub mod guards;
pub mod loader;
pub mod model;
pub mod precedence;
pub mod runtime;
pub mod schema;
pub mod session;
pub mod types;

pub use effective::{
    effective_config, init_effective_config, is_effective_initialized, try_effective_config,
    AlertingSection, ConfigValueSource, DatabaseSection, EffectiveConfig, LoggingSection,
    MetricsSection, ModelSection, PathsSection, RateLimitsSection, SecuritySection, ServerSection,
};
pub use global::{
    config, config_or_default, init_runtime_config, is_initialized, try_config, ConfigError,
};
pub use guards::{ConfigGuards, FeatureFlags};
pub use loader::ConfigLoader;
pub use model::{
    get_model_path_optional, get_model_path_with_fallback, get_tokenizer_path,
    get_tokenizer_path_optional, is_model_path_configured, is_tokenizer_available, load_dotenv,
    resolve_tokenizer_path, BackendPreference, ModelConfig,
};
pub use precedence::DeterministicConfig;
pub use runtime::{ConfigSource, ParsedValue, RuntimeConfig, StorageBackend};
pub use schema::{
    default_schema, parse_bool, validate_value, ConfigSchema, ConfigType, ConfigVariable,
    DeprecationInfo, ValidationError,
};
pub use session::{
    ConfigDriftReport, ConfigDriftSeverity, ConfigFieldDrift, ConfigSnapshot, ConfigSnapshotEntry,
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
