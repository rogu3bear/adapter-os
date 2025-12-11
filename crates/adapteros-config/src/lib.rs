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
//! AOS_MODEL_PATH=./var/models/Qwen2.5-7B-Instruct-4bit
//! AOS_MODEL_BACKEND=metal
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

pub mod coreml;
pub mod effective;
pub mod global;
pub mod guards;
pub mod loader;
pub mod model;
pub mod path_resolver;
pub mod placement;
pub mod precedence;
pub mod runtime;
pub mod schema;
pub mod session;
pub mod types;

pub use coreml::CoreMLComputePreference;
pub use effective::{
    effective_config, init_effective_config, is_effective_initialized, try_effective_config,
    AlertingSection, AuthSection, ConfigValueSource, DatabaseSection, EffectiveConfig,
    InferenceSection, LoggingSection, MetricsSection, ModelSection, PathsSection,
    RateLimitsSection, SecuritySection, SelfHostingMode, SelfHostingSection, ServerSection,
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
pub use path_resolver::{
    prepare_socket_path, resolve_adapters_root, resolve_base_model_location, resolve_database_url,
    resolve_embedding_model_path, resolve_embedding_model_path_with_override, resolve_index_root,
    resolve_manifest_cache_dir, resolve_manifest_path, resolve_model_path, resolve_status_path,
    resolve_telemetry_dir, resolve_worker_socket_for_cp, resolve_worker_socket_for_worker,
    BaseModelLocation, PathSource, ResolvedPath, DEFAULT_ADAPTERS_ROOT, DEFAULT_BASE_MODEL_ID,
    DEFAULT_CP_WORKER_SOCKET, DEFAULT_DB_PATH, DEFAULT_EMBEDDING_MODEL_PATH, DEFAULT_INDEX_ROOT,
    DEFAULT_MANIFEST_CACHE_DIR, DEFAULT_MODEL_CACHE_ROOT, DEFAULT_STATUS_PATH,
    DEFAULT_TELEMETRY_DIR, DEFAULT_WORKER_SOCKET_DEV, DEFAULT_WORKER_SOCKET_PROD_ROOT,
    DEV_MANIFEST_PATH, DEV_MODEL_PATH,
};
pub use placement::{PlacementConfig, PlacementMode, PlacementWeights};
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
    ConfigGuards::initialize()?;

    let loader = ConfigLoader::new();
    let config = loader.load(cli_args, manifest_path)?;

    CONFIG
        .set(config)
        .map_err(|_| AosError::Config("Configuration already initialized".to_string()))?;

    // Lock environment access after initialization
    ConfigGuards::freeze()?;

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
