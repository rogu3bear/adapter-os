//! Unified model configuration module
//!
//! Provides a single source of truth for model configuration, including
//! architecture parameters, paths, and backend preferences.
//!
//! # Environment Configuration
//!
//! Configuration can be set via environment variables or a `.env` file:
//!
//! ```env
//! # .env file in project root
//! AOS_MODEL_PATH=./var/models/Qwen2.5-7B-Instruct-4bit
//! AOS_MODEL_BACKEND=mlx
//! ```

use crate::path_resolver::{resolve_model_path, DEV_MODEL_PATH};
use adapteros_core::backend::BackendKind;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Once;

/// Global flag to ensure .env is only loaded once
static DOTENV_INIT: Once = Once::new();
#[cfg(test)]
static DOTENV_SKIP_INIT: Once = Once::new();

/// Load .env file from current directory or parent directories
///
/// This function is idempotent - it only loads the .env file once,
/// even if called multiple times.
pub fn load_dotenv() {
    #[cfg(test)]
    DOTENV_SKIP_INIT.call_once(|| {
        std::env::set_var("AOS_SKIP_DOTENV", "1");
    });

    // Allow tests to opt out of loading workspace .env (prevents host-specific leakage)
    if std::env::var("AOS_SKIP_DOTENV").is_ok() {
        return;
    }

    DOTENV_INIT.call_once(|| {
        // Try to load .env from current directory first
        if dotenvy::dotenv().is_ok() {
            tracing::debug!("Loaded .env file");
        } else {
            // Try to find .env in parent directories (up to project root)
            if let Ok(cwd) = std::env::current_dir() {
                for ancestor in cwd.ancestors().take(5) {
                    let env_path = ancestor.join(".env");
                    if env_path.exists() && dotenvy::from_path(&env_path).is_ok() {
                        tracing::debug!("Loaded .env file from: {}", env_path.display());
                        break;
                    }
                }
            }
        }
    });
}

/// Canonical backend preference used in config (alias of BackendKind).
pub type BackendPreference = BackendKind;

/// Unified model configuration
///
/// This struct provides a single source of truth for model configuration,
/// consolidating path, architecture, and backend settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Path to the model directory or file (single source of truth)
    pub path: PathBuf,

    /// Model architecture identifier (e.g., "qwen2.5", "llama", "mistral")
    pub architecture: String,

    /// Vocabulary size
    pub vocab_size: usize,

    /// Hidden layer dimension
    pub hidden_size: usize,

    /// Number of transformer layers
    pub num_layers: usize,

    /// Number of attention heads
    pub num_attention_heads: usize,

    /// Number of key-value heads (for GQA - Grouped Query Attention)
    pub num_key_value_heads: usize,

    /// FFN intermediate size
    pub intermediate_size: usize,

    /// Maximum sequence length
    pub max_seq_len: usize,

    /// RoPE (Rotary Position Embedding) theta parameter
    pub rope_theta: f32,

    /// Preferred backend for execution
    pub backend: BackendPreference,
}

impl Default for ModelConfig {
    /// Default configuration for Qwen2.5-Coder-32B model (MLX primary, dev-only)
    fn default() -> Self {
        if !cfg!(debug_assertions) {
            panic!("ModelConfig::default() is dev-only. Set AOS_MODEL_PATH or load from config.json in release builds.");
        }

        Self::dev_fixture()
    }
}

impl ModelConfig {
    /// Dev-only fixture used for debug builds and tests.
    pub fn dev_fixture() -> Self {
        Self {
            path: PathBuf::from(DEV_MODEL_PATH),
            architecture: "qwen2.5".to_string(),
            vocab_size: 151936,
            hidden_size: 5120,
            num_layers: 40,
            num_attention_heads: 40,
            num_key_value_heads: 8,
            intermediate_size: 13824,
            max_seq_len: 32768,
            rope_theta: 1_000_000.0,
            backend: BackendPreference::Auto,
        }
    }

    /// Create a new ModelConfig with the specified path
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            ..Self::dev_fixture()
        }
    }

    /// Load configuration from environment variables (including .env file)
    ///
    /// Automatically loads `.env` file from current directory or project root
    /// before reading environment variables.
    ///
    /// Reads:
    /// - `AOS_MODEL_PATH` - path to model directory/file
    /// - `AOS_MODEL_BACKEND` - preferred backend (auto, coreml, metal, mlx, cpu)
    ///
    /// Returns default config if environment variables are not set.
    pub fn from_env() -> Result<Self> {
        // Load .env file (silently ignore if not found)
        load_dotenv();

        let resolved = resolve_model_path(None, None)?;
        let mut config = Self::dev_fixture();
        config.path = resolved.path;

        // Read backend preference from environment
        if let Ok(backend_str) = std::env::var("AOS_MODEL_BACKEND") {
            config.backend = backend_str.parse()?;
        }

        Ok(config)
    }

    /// Load configuration from a model's config.json file
    ///
    /// Parses the HuggingFace-style config.json format and extracts
    /// relevant architecture parameters.
    pub fn from_config_json(path: &Path) -> Result<Self> {
        let config_path = if path.is_dir() {
            path.join("config.json")
        } else {
            path.to_path_buf()
        };

        let contents = std::fs::read_to_string(&config_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read config.json at '{}': {}",
                config_path.display(),
                e
            ))
        })?;

        let json: serde_json::Value = serde_json::from_str(&contents).map_err(|e| {
            AosError::Config(format!(
                "Failed to parse config.json at '{}': {}",
                config_path.display(),
                e
            ))
        })?;

        // Determine model directory path
        let model_dir = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        };

        // Extract architecture from model_type or architectures field
        let architecture = json
            .get("model_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                json.get("architectures")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_lowercase().replace("forcausallm", ""))
            })
            .unwrap_or_else(|| "unknown".to_string());

        // Track fields that used defaults
        let mut defaulted_fields = Vec::new();

        // Extract model parameters with fallbacks to defaults
        let vocab_size = match json.get("vocab_size").and_then(|v| v.as_u64()) {
            Some(v) => v as usize,
            None => {
                defaulted_fields.push("vocab_size");
                152064
            }
        };

        let hidden_size = match json.get("hidden_size").and_then(|v| v.as_u64()) {
            Some(v) => v as usize,
            None => {
                defaulted_fields.push("hidden_size");
                3584
            }
        };

        let num_layers = match json.get("num_hidden_layers").and_then(|v| v.as_u64()) {
            Some(v) => v as usize,
            None => {
                defaulted_fields.push("num_layers");
                28
            }
        };

        let num_attention_heads = match json.get("num_attention_heads").and_then(|v| v.as_u64()) {
            Some(v) => v as usize,
            None => {
                defaulted_fields.push("num_attention_heads");
                28
            }
        };

        let num_key_value_heads = match json.get("num_key_value_heads").and_then(|v| v.as_u64()) {
            Some(v) => v as usize,
            None => {
                defaulted_fields.push("num_key_value_heads");
                4
            }
        };

        let intermediate_size = match json.get("intermediate_size").and_then(|v| v.as_u64()) {
            Some(v) => v as usize,
            None => {
                defaulted_fields.push("intermediate_size");
                18944
            }
        };

        let max_seq_len = match json.get("max_position_embeddings").and_then(|v| v.as_u64()) {
            Some(v) => v as usize,
            None => {
                defaulted_fields.push("max_seq_len");
                32768
            }
        };

        let rope_theta = match json.get("rope_theta").and_then(|v| v.as_f64()) {
            Some(v) => v as f32,
            None => {
                defaulted_fields.push("rope_theta");
                1_000_000.0
            }
        };

        // Log consolidated warning if any fields used defaults
        if !defaulted_fields.is_empty() {
            tracing::warn!(
                config_path = %config_path.display(),
                fields = %defaulted_fields.join(", "),
                "Model config.json missing fields, using defaults"
            );
        }

        Ok(Self {
            path: model_dir,
            architecture,
            vocab_size,
            hidden_size,
            num_layers,
            num_attention_heads,
            num_key_value_heads,
            intermediate_size,
            max_seq_len,
            rope_theta,
            backend: BackendPreference::Auto,
        })
    }

    /// Validate configuration consistency
    ///
    /// Checks:
    /// - Path exists (if not using placeholder)
    /// - Architecture is non-empty
    /// - Dimension parameters are positive
    /// - KV heads divides attention heads evenly (for GQA)
    /// - Head dimension is consistent
    pub fn validate(&self) -> Result<()> {
        // Validate path exists (skip for dev placeholder path in debug)
        let dev_placeholder: PathBuf = DEV_MODEL_PATH.into();
        if !self.path.exists() {
            if cfg!(debug_assertions) && self.path == dev_placeholder {
                tracing::warn!(
                    path = %self.path.display(),
                    "Dev fixture model path missing; allowed in debug builds"
                );
            } else {
                return Err(AosError::Config(format!(
                    "Model path does not exist: '{}'",
                    self.path.display()
                )));
            }
        }

        // Validate architecture
        if self.architecture.is_empty() {
            return Err(AosError::Config(
                "Model architecture cannot be empty".to_string(),
            ));
        }

        // Validate positive dimensions
        if self.vocab_size == 0 {
            return Err(AosError::Config(
                "vocab_size must be greater than 0".to_string(),
            ));
        }

        if self.hidden_size == 0 {
            return Err(AosError::Config(
                "hidden_size must be greater than 0".to_string(),
            ));
        }

        if self.num_layers == 0 {
            return Err(AosError::Config(
                "num_layers must be greater than 0".to_string(),
            ));
        }

        if self.num_attention_heads == 0 {
            return Err(AosError::Config(
                "num_attention_heads must be greater than 0".to_string(),
            ));
        }

        if self.num_key_value_heads == 0 {
            return Err(AosError::Config(
                "num_key_value_heads must be greater than 0".to_string(),
            ));
        }

        if self.intermediate_size == 0 {
            return Err(AosError::Config(
                "intermediate_size must be greater than 0".to_string(),
            ));
        }

        if self.max_seq_len == 0 {
            return Err(AosError::Config(
                "max_seq_len must be greater than 0".to_string(),
            ));
        }

        // Validate GQA: attention heads must be divisible by KV heads
        if !self
            .num_attention_heads
            .is_multiple_of(self.num_key_value_heads)
        {
            return Err(AosError::Config(format!(
                "num_attention_heads ({}) must be divisible by num_key_value_heads ({})",
                self.num_attention_heads, self.num_key_value_heads
            )));
        }

        // Validate head dimension: hidden_size must be divisible by attention heads
        if !self.hidden_size.is_multiple_of(self.num_attention_heads) {
            return Err(AosError::Config(format!(
                "hidden_size ({}) must be divisible by num_attention_heads ({})",
                self.hidden_size, self.num_attention_heads
            )));
        }

        // Validate rope_theta is positive
        if self.rope_theta <= 0.0 {
            return Err(AosError::Config("rope_theta must be positive".to_string()));
        }

        Ok(())
    }

    /// Compute the head dimension
    pub fn head_dim(&self) -> usize {
        self.hidden_size / self.num_attention_heads
    }

    /// Compute the number of attention head groups (for GQA)
    pub fn num_head_groups(&self) -> usize {
        self.num_attention_heads / self.num_key_value_heads
    }

    /// Get the config.json path within the model directory
    pub fn config_json_path(&self) -> PathBuf {
        self.path.join("config.json")
    }

    /// Check if the model path points to a valid model directory
    pub fn is_valid_model_dir(&self) -> bool {
        self.path.is_dir() && self.config_json_path().exists()
    }
}

// ============================================================================
// Legacy Environment Variable Support
// ============================================================================

/// Legacy environment variable names that map to AOS_MODEL_PATH
const LEGACY_MODEL_PATH_VARS: &[&str] = &["AOS_MLX_FFI_MODEL", "MLX_PATH"];

/// Get model path with legacy fallback support
///
/// This function provides a clean migration path from legacy environment variables
/// to the unified `AOS_MODEL_PATH` configuration. It checks variables in order:
///
/// 1. `AOS_MODEL_PATH` (primary, preferred)
/// 2. `AOS_MLX_FFI_MODEL` (legacy, deprecated)
/// 3. `MLX_PATH` (legacy, deprecated)
///
/// If a legacy variable is used, a warning is logged to encourage migration.
///
/// # Example
///
/// ```rust,ignore
/// use adapteros_config::model::get_model_path_with_fallback;
///
/// let path = get_model_path_with_fallback()?;
/// println!("Using model at: {}", path.display());
/// ```
pub fn get_model_path_with_fallback() -> Result<PathBuf> {
    load_dotenv();

    // Try primary env var first
    if let Ok(path) = std::env::var("AOS_MODEL_PATH") {
        if !path.is_empty() {
            let path = PathBuf::from(path);
            crate::path_resolver::reject_tmp_persistent_path(&path, "model-path")?;
            return Ok(path);
        }
    }

    // Try legacy env vars with deprecation warning
    for legacy_var in LEGACY_MODEL_PATH_VARS {
        if let Ok(path) = std::env::var(legacy_var) {
            if !path.is_empty() {
                tracing::warn!(
                    legacy_var = %legacy_var,
                    "Using deprecated environment variable. Please migrate to AOS_MODEL_PATH"
                );
                let path = PathBuf::from(path);
                crate::path_resolver::reject_tmp_persistent_path(&path, "model-path")?;
                return Ok(path);
            }
        }
    }

    Err(AosError::Config(
        "Model path not configured. Set AOS_MODEL_PATH in .env or environment. \
        Run 'aosctl config migrate' to migrate from legacy variables."
            .to_string(),
    ))
}

/// Get model path with fallback, returning None instead of error if not set
///
/// Useful when model path is optional or has a default fallback.
pub fn get_model_path_optional() -> Option<PathBuf> {
    get_model_path_with_fallback().ok()
}

/// Check if model path is configured (via any supported variable)
pub fn is_model_path_configured() -> bool {
    load_dotenv();

    if std::env::var("AOS_MODEL_PATH")
        .map(|s| !s.is_empty())
        .unwrap_or(false)
    {
        return true;
    }

    for legacy_var in LEGACY_MODEL_PATH_VARS {
        if std::env::var(legacy_var)
            .map(|s| !s.is_empty())
            .unwrap_or(false)
        {
            return true;
        }
    }

    false
}

// ============================================================================
// Tokenizer Discovery
// ============================================================================

/// Get tokenizer path with dynamic discovery
///
/// This function discovers the tokenizer path using the following precedence:
/// 1. `AOS_TOKENIZER_PATH` environment variable (explicit override)
/// 2. `tokenizer.json` within the model directory from `AOS_MODEL_PATH`
/// 3. Error with remediation steps (no magic fallback)
///
/// # Example
///
/// ```rust,ignore
/// use adapteros_config::get_tokenizer_path;
///
/// let path = get_tokenizer_path()?;
/// println!("Using tokenizer at: {}", path.display());
/// ```
pub fn get_tokenizer_path() -> Result<PathBuf> {
    load_dotenv();

    // 1. Check explicit tokenizer path (AOS_TOKENIZER_PATH)
    if let Ok(path) = std::env::var("AOS_TOKENIZER_PATH") {
        if !path.is_empty() {
            let path = PathBuf::from(&path);
            // Security: reject /tmp paths for persistent tokenizer storage
            crate::path_resolver::reject_tmp_persistent_path(&path, "tokenizer-path")?;
            if path.exists() {
                tracing::debug!(path = %path.display(), "Using tokenizer from AOS_TOKENIZER_PATH");
                return Ok(path);
            }
            // Explicit path set but doesn't exist - this is an error, not a fallback
            return Err(AosError::Config(format!(
                "AOS_TOKENIZER_PATH is set to '{}' but the file does not exist",
                path.display()
            )));
        }
    }

    // 2. Discover from model path (AOS_MODEL_PATH/tokenizer.json)
    if let Ok(model_path) = get_model_path_with_fallback() {
        let tokenizer_path = model_path.join("tokenizer.json");
        crate::path_resolver::reject_tmp_persistent_path(&tokenizer_path, "tokenizer-path")?;
        if tokenizer_path.exists() {
            tracing::debug!(path = %tokenizer_path.display(), "Discovered tokenizer in model directory");
            return Ok(tokenizer_path);
        }
    }

    // No magic fallback - provide clear error with remediation steps
    let model_path_hint = std::env::var("AOS_MODEL_PATH")
        .map(|p| format!(" (AOS_MODEL_PATH='{}')", p))
        .unwrap_or_default();

    Err(AosError::Config(format!(
        "Tokenizer not found. To fix:\n\
         1. Set AOS_TOKENIZER_PATH to the path of your tokenizer.json file, or\n\
         2. Ensure tokenizer.json exists in your model directory{}\n\
         \n\
         Example: export AOS_TOKENIZER_PATH=./var/models/Qwen2.5-7B-Instruct-4bit/tokenizer.json",
        model_path_hint
    )))
}

/// Resolve tokenizer path from CLI override or automatic discovery
///
/// This is the **canonical function** for resolving tokenizer paths across the codebase.
/// All CLI commands, workers, and xtask utilities should use this function to ensure
/// consistent behavior.
///
/// # Resolution Order
///
/// 1. **CLI override**: If `cli_override` is `Some(path)`, validates the path exists
/// 2. **AOS_TOKENIZER_PATH**: Environment variable for explicit configuration
/// 3. **Model directory**: Looks for `tokenizer.json` in `AOS_MODEL_PATH`
/// 4. **Error**: Returns actionable error message with remediation steps
///
/// # Example
///
/// ```rust,ignore
/// // In CLI commands with TokenizerArg:
/// let path = resolve_tokenizer_path(self.tokenizer_arg.tokenizer.as_ref())?;
///
/// // In non-CLI code with Option<PathBuf>:
/// let path = resolve_tokenizer_path(config.tokenizer_path.as_ref())?;
/// ```
///
/// # Errors
///
/// Returns `AosError::Config` if:
/// - CLI override path doesn't exist
/// - `AOS_TOKENIZER_PATH` is set but file doesn't exist
/// - No tokenizer found via discovery
pub fn resolve_tokenizer_path(cli_override: Option<&PathBuf>) -> Result<PathBuf> {
    match cli_override {
        Some(path) => {
            crate::path_resolver::reject_tmp_persistent_path(path, "tokenizer-path")?;
            if path.exists() {
                Ok(path.clone())
            } else {
                Err(AosError::Config(format!(
                    "Tokenizer file not found at specified path: {}",
                    path.display()
                )))
            }
        }
        None => get_tokenizer_path(),
    }
}

/// Get tokenizer path, returning None instead of error if not found
///
/// Useful when tokenizer is optional or has a CLI override.
pub fn get_tokenizer_path_optional() -> Option<PathBuf> {
    get_tokenizer_path().ok()
}

/// Check if tokenizer exists at standard locations
pub fn is_tokenizer_available() -> bool {
    get_tokenizer_path().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestEnvGuard;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("tempdir")
    }

    #[test]
    fn test_default_config() {
        let config = ModelConfig::default();
        let fixture = ModelConfig::dev_fixture();
        assert_eq!(config.architecture, fixture.architecture);
        assert_eq!(config.vocab_size, fixture.vocab_size);
        assert_eq!(config.hidden_size, fixture.hidden_size);
        assert_eq!(config.num_layers, fixture.num_layers);
        assert_eq!(config.num_attention_heads, fixture.num_attention_heads);
        assert_eq!(config.num_key_value_heads, fixture.num_key_value_heads);
        assert_eq!(config.intermediate_size, fixture.intermediate_size);
        assert_eq!(config.max_seq_len, fixture.max_seq_len);
        assert_eq!(config.rope_theta, fixture.rope_theta);
        assert_eq!(config.backend, fixture.backend);
    }

    #[test]
    fn test_default_config_validation() {
        let config = ModelConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_head_dim() {
        let config = ModelConfig::default();
        assert_eq!(config.head_dim(), 128); // 5120 / 40 = 128
    }

    #[test]
    fn test_num_head_groups() {
        let config = ModelConfig::default();
        let expected = config.num_attention_heads / config.num_key_value_heads;
        assert_eq!(config.num_head_groups(), expected); // uses dev fixture defaults
    }

    #[test]
    fn test_backend_preference_display() {
        assert_eq!(BackendPreference::Auto.to_string(), "auto");
        assert_eq!(BackendPreference::CoreML.to_string(), "coreml");
        assert_eq!(BackendPreference::Metal.to_string(), "metal");
        assert_eq!(BackendPreference::Mlx.to_string(), "mlx");
        assert_eq!(BackendPreference::CPU.to_string(), "cpu");
    }

    #[test]
    fn test_backend_preference_from_str() {
        assert_eq!(
            "auto".parse::<BackendPreference>().unwrap(),
            BackendPreference::Auto
        );
        assert_eq!(
            "coreml".parse::<BackendPreference>().unwrap(),
            BackendPreference::CoreML
        );
        assert_eq!(
            "metal".parse::<BackendPreference>().unwrap(),
            BackendPreference::Metal
        );
        assert_eq!(
            "mlx".parse::<BackendPreference>().unwrap(),
            BackendPreference::Mlx
        );
        assert_eq!(
            "AUTO".parse::<BackendPreference>().unwrap(),
            BackendPreference::Auto
        );
        assert_eq!(
            "cpu".parse::<BackendPreference>().unwrap(),
            BackendPreference::CPU
        );
        assert!("invalid".parse::<BackendPreference>().is_err());
    }

    #[test]
    fn test_validation_zero_vocab_size() {
        let mut config = ModelConfig::default();
        config.vocab_size = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_zero_hidden_size() {
        let mut config = ModelConfig::default();
        config.hidden_size = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_gqa() {
        let mut config = ModelConfig::default();
        config.num_attention_heads = 28;
        config.num_key_value_heads = 5; // 28 is not divisible by 5
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_head_dim() {
        let mut config = ModelConfig::default();
        config.hidden_size = 100;
        config.num_attention_heads = 7; // 100 is not divisible by 7
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_empty_architecture() {
        let mut config = ModelConfig::default();
        config.architecture = String::new();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_negative_rope_theta() {
        let mut config = ModelConfig::default();
        config.rope_theta = -1.0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_from_env_no_vars() {
        let _env = TestEnvGuard::new();
        // Clear any existing env vars
        std::env::remove_var("AOS_MODEL_PATH");
        std::env::remove_var("AOS_MODEL_BACKEND");

        let config = ModelConfig::from_env().unwrap();
        assert_eq!(config.path, PathBuf::from(DEV_MODEL_PATH));
        assert_eq!(config.backend, BackendPreference::Auto);
    }

    #[test]
    fn test_from_env_with_vars() {
        let _env = TestEnvGuard::new();
        // Skip workspace .env to avoid leakage into expectations
        std::env::set_var("AOS_SKIP_DOTENV", "1");

        let temp = new_test_tempdir();
        let model_dir = temp.path().join("test-model");
        std::fs::create_dir_all(&model_dir).unwrap();

        std::env::set_var("AOS_MODEL_PATH", &model_dir);
        std::env::set_var("AOS_MODEL_BACKEND", "mlx");

        let config = ModelConfig::from_env().unwrap();
        assert_eq!(config.path, model_dir);
        assert_eq!(config.backend, BackendPreference::Mlx);

        // Clean up
        std::env::remove_var("AOS_MODEL_PATH");
        std::env::remove_var("AOS_MODEL_BACKEND");
        std::env::remove_var("AOS_SKIP_DOTENV");
    }

    #[test]
    fn test_config_json_path() {
        let config = ModelConfig::new(PathBuf::from("/models/test"));
        assert_eq!(
            config.config_json_path(),
            PathBuf::from("/models/test/config.json")
        );
    }

    #[test]
    fn test_serde_roundtrip() {
        let config = ModelConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ModelConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.path, deserialized.path);
        assert_eq!(config.architecture, deserialized.architecture);
        assert_eq!(config.vocab_size, deserialized.vocab_size);
        assert_eq!(config.hidden_size, deserialized.hidden_size);
        assert_eq!(config.num_layers, deserialized.num_layers);
        assert_eq!(config.num_attention_heads, deserialized.num_attention_heads);
        assert_eq!(config.num_key_value_heads, deserialized.num_key_value_heads);
        assert_eq!(config.intermediate_size, deserialized.intermediate_size);
        assert_eq!(config.max_seq_len, deserialized.max_seq_len);
        assert_eq!(config.rope_theta, deserialized.rope_theta);
        assert_eq!(config.backend, deserialized.backend);
    }

    // ========================================================================
    // Tokenizer Discovery Tests
    // ========================================================================

    mod tokenizer_tests {
        use super::TestEnvGuard;
        use super::*;
        use std::fs;
        use tempfile::TempDir;

        /// Helper to create a temporary directory with a tokenizer.json file
        fn create_temp_tokenizer() -> (TempDir, PathBuf) {
            let temp_dir = new_test_tempdir();
            let tokenizer_path = temp_dir.path().join("tokenizer.json");
            fs::write(&tokenizer_path, r#"{"version": "1.0"}"#).unwrap();
            (temp_dir, tokenizer_path)
        }

        /// Helper to create a temp model directory with tokenizer.json
        fn create_temp_model_dir() -> (TempDir, PathBuf) {
            let temp_dir = new_test_tempdir();
            let model_dir = temp_dir.path().join("model");
            fs::create_dir(&model_dir).unwrap();
            let tokenizer_path = model_dir.join("tokenizer.json");
            fs::write(&tokenizer_path, r#"{"version": "1.0"}"#).unwrap();
            (temp_dir, model_dir)
        }

        #[test]
        fn test_resolve_tokenizer_path_with_valid_override() {
            let _env = TestEnvGuard::new();
            let (_temp_dir, tokenizer_path) = create_temp_tokenizer();

            let result = resolve_tokenizer_path(Some(&tokenizer_path));
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), tokenizer_path);
        }

        #[test]
        fn test_resolve_tokenizer_path_with_invalid_override() {
            let _env = TestEnvGuard::new();
            let nonexistent = PathBuf::from("/nonexistent/path/tokenizer.json");

            let result = resolve_tokenizer_path(Some(&nonexistent));
            assert!(result.is_err());

            let err = result.unwrap_err().to_string();
            assert!(err.contains("not found at specified path"));
            assert!(err.contains("/nonexistent/path/tokenizer.json"));
        }

        #[test]
        fn test_resolve_tokenizer_path_none_without_env() {
            let _env = TestEnvGuard::new();
            // Clear relevant env vars for this test
            std::env::remove_var("AOS_TOKENIZER_PATH");
            std::env::remove_var("AOS_MODEL_PATH");
            std::env::remove_var("AOS_MLX_FFI_MODEL");

            let result = resolve_tokenizer_path(None);

            // Should fail with helpful error since no discovery source is available
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("Tokenizer not found"));
            assert!(err.contains("AOS_TOKENIZER_PATH"));
        }

        #[test]
        fn test_get_tokenizer_path_with_explicit_env() {
            let _env = TestEnvGuard::new();
            let (_temp_dir, tokenizer_path) = create_temp_tokenizer();

            // Set explicit tokenizer path
            std::env::set_var("AOS_TOKENIZER_PATH", tokenizer_path.to_str().unwrap());

            let result = get_tokenizer_path();
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), tokenizer_path);

            // Cleanup
            std::env::remove_var("AOS_TOKENIZER_PATH");
        }

        #[test]
        fn test_get_tokenizer_path_explicit_env_nonexistent_errors() {
            let _env = TestEnvGuard::new();
            // Set explicit path to nonexistent file - should ERROR, not fallback
            std::env::set_var("AOS_TOKENIZER_PATH", "/nonexistent/tokenizer.json");

            let result = get_tokenizer_path();
            assert!(result.is_err());

            let err = result.unwrap_err().to_string();
            assert!(err.contains("AOS_TOKENIZER_PATH is set"));
            assert!(err.contains("does not exist"));

            // Cleanup
            std::env::remove_var("AOS_TOKENIZER_PATH");
        }

        #[test]
        fn test_get_tokenizer_path_discovers_from_model_path() {
            let _env = TestEnvGuard::new();
            let (_temp_dir, model_dir) = create_temp_model_dir();

            // Clear explicit tokenizer path, set model path
            std::env::remove_var("AOS_TOKENIZER_PATH");
            std::env::set_var("AOS_MODEL_PATH", model_dir.to_str().unwrap());

            let result = get_tokenizer_path();
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), model_dir.join("tokenizer.json"));

            // Cleanup
            std::env::remove_var("AOS_MODEL_PATH");
        }

        #[test]
        fn test_get_tokenizer_path_error_message_includes_model_path() {
            let _env = TestEnvGuard::new();
            // Set model path but no tokenizer in it
            let temp_dir = new_test_tempdir();
            let empty_model_dir = temp_dir.path().join("empty-model");
            fs::create_dir(&empty_model_dir).unwrap();

            std::env::remove_var("AOS_TOKENIZER_PATH");
            std::env::set_var("AOS_MODEL_PATH", empty_model_dir.to_str().unwrap());

            let result = get_tokenizer_path();
            assert!(result.is_err());

            let err = result.unwrap_err().to_string();
            // Should mention the model path in the error
            assert!(err.contains("AOS_MODEL_PATH"));

            // Cleanup
            std::env::remove_var("AOS_MODEL_PATH");
        }

        #[test]
        fn test_get_tokenizer_path_optional_returns_some() {
            let _env = TestEnvGuard::new();
            let (_temp_dir, tokenizer_path) = create_temp_tokenizer();
            std::env::set_var("AOS_TOKENIZER_PATH", tokenizer_path.to_str().unwrap());

            let result = get_tokenizer_path_optional();
            assert!(result.is_some());
            assert_eq!(result.unwrap(), tokenizer_path);

            std::env::remove_var("AOS_TOKENIZER_PATH");
        }

        #[test]
        fn test_get_tokenizer_path_optional_returns_none() {
            let _env = TestEnvGuard::new();
            std::env::remove_var("AOS_TOKENIZER_PATH");
            std::env::remove_var("AOS_MODEL_PATH");
            std::env::remove_var("AOS_MLX_FFI_MODEL");

            let result = get_tokenizer_path_optional();
            assert!(result.is_none());
        }

        #[test]
        fn test_is_tokenizer_available_true() {
            let _env = TestEnvGuard::new();
            let (_temp_dir, tokenizer_path) = create_temp_tokenizer();
            std::env::set_var("AOS_TOKENIZER_PATH", tokenizer_path.to_str().unwrap());

            assert!(is_tokenizer_available());

            std::env::remove_var("AOS_TOKENIZER_PATH");
        }

        #[test]
        fn test_is_tokenizer_available_false() {
            let _env = TestEnvGuard::new();
            std::env::remove_var("AOS_TOKENIZER_PATH");
            std::env::remove_var("AOS_MODEL_PATH");
            std::env::remove_var("AOS_MLX_FFI_MODEL");

            assert!(!is_tokenizer_available());
        }
    }
}
