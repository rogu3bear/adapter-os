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
//! AOS_MODEL_PATH=./models/qwen2.5-7b-mlx
//! AOS_MODEL_BACKEND=auto
//! ```

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Once;

/// Global flag to ensure .env is only loaded once
static DOTENV_INIT: Once = Once::new();

/// Load .env file from current directory or parent directories
///
/// This function is idempotent - it only loads the .env file once,
/// even if called multiple times.
pub fn load_dotenv() {
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

/// Backend preference for model execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BackendPreference {
    /// Automatic backend selection based on system capabilities
    #[default]
    Auto,
    /// Prefer CoreML backend (ANE acceleration, production)
    #[serde(rename = "coreml")]
    CoreML,
    /// Prefer Metal backend (legacy, fallback)
    Metal,
    /// Prefer MLX backend (research, training)
    Mlx,
}

impl std::fmt::Display for BackendPreference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendPreference::Auto => write!(f, "auto"),
            BackendPreference::CoreML => write!(f, "coreml"),
            BackendPreference::Metal => write!(f, "metal"),
            BackendPreference::Mlx => write!(f, "mlx"),
        }
    }
}

impl std::str::FromStr for BackendPreference {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(BackendPreference::Auto),
            "coreml" => Ok(BackendPreference::CoreML),
            "metal" => Ok(BackendPreference::Metal),
            "mlx" => Ok(BackendPreference::Mlx),
            _ => Err(AosError::Config(format!(
                "Unknown backend preference: '{}'. Expected one of: auto, coreml, metal, mlx",
                s
            ))),
        }
    }
}

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
    /// Default configuration for Qwen2.5-7B model
    fn default() -> Self {
        Self {
            path: PathBuf::from("./models/qwen2.5-7b"),
            architecture: "qwen2.5".to_string(),
            vocab_size: 152064,
            hidden_size: 3584,
            num_layers: 28,
            num_attention_heads: 28,
            num_key_value_heads: 4,
            intermediate_size: 18944,
            max_seq_len: 32768,
            rope_theta: 1_000_000.0,
            backend: BackendPreference::Auto,
        }
    }
}

impl ModelConfig {
    /// Create a new ModelConfig with the specified path
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            ..Default::default()
        }
    }

    /// Load configuration from environment variables (including .env file)
    ///
    /// Automatically loads `.env` file from current directory or project root
    /// before reading environment variables.
    ///
    /// Reads:
    /// - `AOS_MODEL_PATH` - path to model directory/file
    /// - `AOS_MODEL_BACKEND` - preferred backend (auto, coreml, metal, mlx)
    ///
    /// Returns default config if environment variables are not set.
    pub fn from_env() -> Result<Self> {
        // Load .env file (silently ignore if not found)
        load_dotenv();

        let mut config = Self::default();

        // Read model path from environment
        if let Ok(path) = std::env::var("AOS_MODEL_PATH") {
            config.path = PathBuf::from(path);
        }

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

        // Extract model parameters with fallbacks to defaults
        let vocab_size = json
            .get("vocab_size")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(152064);

        let hidden_size = json
            .get("hidden_size")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(3584);

        let num_layers = json
            .get("num_hidden_layers")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(28);

        let num_attention_heads = json
            .get("num_attention_heads")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(28);

        let num_key_value_heads = json
            .get("num_key_value_heads")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(4);

        let intermediate_size = json
            .get("intermediate_size")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(18944);

        let max_seq_len = json
            .get("max_position_embeddings")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(32768);

        let rope_theta = json
            .get("rope_theta")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(1_000_000.0);

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
        // Validate path exists (skip for default placeholder path)
        let default_placeholder: PathBuf = "./models/qwen2.5-7b".into();
        if self.path != default_placeholder && !self.path.exists() {
            return Err(AosError::Config(format!(
                "Model path does not exist: '{}'",
                self.path.display()
            )));
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
            return Ok(PathBuf::from(path));
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
                return Ok(PathBuf::from(path));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ModelConfig::default();
        assert_eq!(config.architecture, "qwen2.5");
        assert_eq!(config.vocab_size, 152064);
        assert_eq!(config.hidden_size, 3584);
        assert_eq!(config.num_layers, 28);
        assert_eq!(config.num_attention_heads, 28);
        assert_eq!(config.num_key_value_heads, 4);
        assert_eq!(config.intermediate_size, 18944);
        assert_eq!(config.max_seq_len, 32768);
        assert_eq!(config.rope_theta, 1_000_000.0);
        assert_eq!(config.backend, BackendPreference::Auto);
    }

    #[test]
    fn test_default_config_validation() {
        let config = ModelConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_head_dim() {
        let config = ModelConfig::default();
        assert_eq!(config.head_dim(), 128); // 3584 / 28 = 128
    }

    #[test]
    fn test_num_head_groups() {
        let config = ModelConfig::default();
        assert_eq!(config.num_head_groups(), 7); // 28 / 4 = 7
    }

    #[test]
    fn test_backend_preference_display() {
        assert_eq!(BackendPreference::Auto.to_string(), "auto");
        assert_eq!(BackendPreference::CoreML.to_string(), "coreml");
        assert_eq!(BackendPreference::Metal.to_string(), "metal");
        assert_eq!(BackendPreference::Mlx.to_string(), "mlx");
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
        // Clear any existing env vars
        std::env::remove_var("AOS_MODEL_PATH");
        std::env::remove_var("AOS_MODEL_BACKEND");

        let config = ModelConfig::from_env().unwrap();
        assert_eq!(config.path, PathBuf::from("./models/qwen2.5-7b"));
        assert_eq!(config.backend, BackendPreference::Auto);
    }

    #[test]
    fn test_from_env_with_vars() {
        std::env::set_var("AOS_MODEL_PATH", "/tmp/test-model");
        std::env::set_var("AOS_MODEL_BACKEND", "mlx");

        let config = ModelConfig::from_env().unwrap();
        assert_eq!(config.path, PathBuf::from("/tmp/test-model"));
        assert_eq!(config.backend, BackendPreference::Mlx);

        // Clean up
        std::env::remove_var("AOS_MODEL_PATH");
        std::env::remove_var("AOS_MODEL_BACKEND");
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
}
