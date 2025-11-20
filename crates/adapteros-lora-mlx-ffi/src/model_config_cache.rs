//! Model configuration caching for MLX backend
//!
//! Provides efficient caching of model configuration parameters with thread-safe
//! access patterns. Avoids repeated file reads and allows dynamic model support
//! by caching vocabulary size, hidden dimensions, layer count, and other critical
//! parameters.
//!
//! # Features
//! - Lazy loading from model files
//! - Thread-safe access with Arc<RwLock>
//! - Cache validation and refresh
//! - Serializable configuration snapshots

use adapteros_core::{AosError, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info};

/// Complete model configuration cache
///
/// Stores all model parameters that are frequently accessed during inference.
/// Implements thread-safe interior mutability for read-heavy workloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigCache {
    /// Vocabulary size (tokens)
    pub vocab_size: usize,
    /// Hidden dimension size
    pub hidden_size: usize,
    /// Number of transformer layers
    pub num_hidden_layers: usize,
    /// Number of attention heads
    pub num_attention_heads: usize,
    /// Number of key-value heads (for grouped query attention)
    pub num_key_value_heads: usize,
    /// Intermediate FFN dimension
    pub intermediate_size: usize,
    /// Maximum sequence length
    pub max_position_embeddings: usize,
    /// RoPE theta parameter
    pub rope_theta: f32,
    /// Head dimension (hidden_size / num_attention_heads)
    pub head_dim: usize,
    /// Attention heads per key-value head (for grouped query attention)
    pub num_heads_per_kv_head: usize,
}

impl ModelConfigCache {
    /// Create a new configuration cache from known values
    ///
    /// # Arguments
    /// * `vocab_size` - Vocabulary size
    /// * `hidden_size` - Hidden dimension
    /// * `num_hidden_layers` - Number of layers
    /// * `num_attention_heads` - Number of attention heads
    /// * `num_key_value_heads` - Number of KV cache heads
    /// * `intermediate_size` - FFN intermediate size
    /// * `max_position_embeddings` - Max sequence length
    /// * `rope_theta` - RoPE theta parameter
    pub fn new(
        vocab_size: usize,
        hidden_size: usize,
        num_hidden_layers: usize,
        num_attention_heads: usize,
        num_key_value_heads: usize,
        intermediate_size: usize,
        max_position_embeddings: usize,
        rope_theta: f32,
    ) -> Result<Self> {
        if hidden_size == 0 || num_attention_heads == 0 {
            return Err(AosError::Validation(
                "hidden_size and num_attention_heads must be non-zero".to_string(),
            ));
        }

        let head_dim = hidden_size / num_attention_heads;
        if head_dim * num_attention_heads != hidden_size {
            return Err(AosError::Validation(
                format!(
                    "hidden_size ({}) must be divisible by num_attention_heads ({})",
                    hidden_size, num_attention_heads
                ),
            ));
        }

        if num_key_value_heads == 0 || num_attention_heads % num_key_value_heads != 0 {
            return Err(AosError::Validation(
                format!(
                    "num_attention_heads ({}) must be divisible by num_key_value_heads ({})",
                    num_attention_heads, num_key_value_heads
                ),
            ));
        }

        let num_heads_per_kv_head = num_attention_heads / num_key_value_heads;

        Ok(Self {
            vocab_size,
            hidden_size,
            num_hidden_layers,
            num_attention_heads,
            num_key_value_heads,
            intermediate_size,
            max_position_embeddings,
            rope_theta,
            head_dim,
            num_heads_per_kv_head,
        })
    }

    /// Load configuration from config.json file
    ///
    /// # Arguments
    /// * `config_path` - Path to config.json file
    pub fn from_file<P: AsRef<Path>>(config_path: P) -> Result<Self> {
        let config_path = config_path.as_ref();
        let config_str = std::fs::read_to_string(config_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read config from {}: {}",
                config_path.display(),
                e
            ))
        })?;

        Self::from_json(&config_str)
    }

    /// Parse configuration from JSON string
    ///
    /// # Arguments
    /// * `json_str` - JSON configuration string
    pub fn from_json(json_str: &str) -> Result<Self> {
        let config_json: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| {
                AosError::Parse(format!("Failed to parse config JSON: {}", e))
            })?;

        let vocab_size = config_json
            .get("vocab_size")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| AosError::Validation("Missing or invalid vocab_size".to_string()))?
            as usize;

        let hidden_size = config_json
            .get("hidden_size")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| AosError::Validation("Missing or invalid hidden_size".to_string()))?
            as usize;

        let num_hidden_layers = config_json
            .get("num_hidden_layers")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| {
                AosError::Validation("Missing or invalid num_hidden_layers".to_string())
            })?
            as usize;

        let num_attention_heads = config_json
            .get("num_attention_heads")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| {
                AosError::Validation("Missing or invalid num_attention_heads".to_string())
            })?
            as usize;

        let num_key_value_heads = config_json
            .get("num_key_value_heads")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(num_attention_heads);

        let intermediate_size = config_json
            .get("intermediate_size")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| {
                AosError::Validation("Missing or invalid intermediate_size".to_string())
            })?
            as usize;

        let max_position_embeddings = config_json
            .get("max_position_embeddings")
            .and_then(|v| v.as_u64())
            .unwrap_or(2048)
            as usize;

        let rope_theta = config_json
            .get("rope_theta")
            .and_then(|v| v.as_f64())
            .unwrap_or(10000.0)
            as f32;

        Self::new(
            vocab_size,
            hidden_size,
            num_hidden_layers,
            num_attention_heads,
            num_key_value_heads,
            intermediate_size,
            max_position_embeddings,
            rope_theta,
        )
    }

    /// Estimate memory requirement for a LoRA adapter
    ///
    /// # Arguments
    /// * `rank` - LoRA rank
    /// * `num_target_modules` - Number of target modules
    ///
    /// # Returns
    /// Estimated bytes needed for adapter weights
    pub fn estimate_adapter_memory(&self, rank: usize, num_target_modules: usize) -> usize {
        // For each target module:
        // - Shared down projection: rank × hidden_size × sizeof(f32)
        // - Per-module up projection: hidden_size × rank × sizeof(f32)
        // Total per module: 2 × rank × hidden_size × sizeof(f32)
        let bytes_per_module = 2 * rank * self.hidden_size * std::mem::size_of::<f32>();
        bytes_per_module * num_target_modules
    }

    /// Validate configuration consistency
    ///
    /// # Returns
    /// Error if configuration is invalid
    pub fn validate(&self) -> Result<()> {
        if self.vocab_size == 0 {
            return Err(AosError::Validation("vocab_size must be > 0".to_string()));
        }
        if self.hidden_size == 0 {
            return Err(AosError::Validation("hidden_size must be > 0".to_string()));
        }
        if self.num_hidden_layers == 0 {
            return Err(AosError::Validation(
                "num_hidden_layers must be > 0".to_string(),
            ));
        }
        if self.num_attention_heads == 0 {
            return Err(AosError::Validation(
                "num_attention_heads must be > 0".to_string(),
            ));
        }
        if self.intermediate_size == 0 {
            return Err(AosError::Validation(
                "intermediate_size must be > 0".to_string(),
            ));
        }
        if self.rope_theta <= 0.0 {
            return Err(AosError::Validation("rope_theta must be > 0".to_string()));
        }

        // Check dimensional consistency
        if self.hidden_size % self.num_attention_heads != 0 {
            return Err(AosError::Validation(format!(
                "hidden_size ({}) must be divisible by num_attention_heads ({})",
                self.hidden_size, self.num_attention_heads
            )));
        }

        if self.num_attention_heads % self.num_key_value_heads != 0 {
            return Err(AosError::Validation(format!(
                "num_attention_heads ({}) must be divisible by num_key_value_heads ({})",
                self.num_attention_heads, self.num_key_value_heads
            )));
        }

        Ok(())
    }

    /// Convert to JSON representation
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| {
            AosError::Serialization(e)
        })
    }
}

/// Thread-safe model configuration cache wrapper
///
/// Provides lazy loading and cached access to model configurations.
/// Uses interior mutability for efficient read operations.
pub struct ModelConfigCacheManager {
    cache: Arc<RwLock<Option<ModelConfigCache>>>,
    config_path: PathBuf,
}

impl ModelConfigCacheManager {
    /// Create a new cache manager pointing to a config file
    ///
    /// # Arguments
    /// * `config_path` - Path to config.json
    pub fn new<P: AsRef<Path>>(config_path: P) -> Self {
        Self {
            cache: Arc::new(RwLock::new(None)),
            config_path: config_path.as_ref().to_path_buf(),
        }
    }

    /// Get or load the cached configuration
    ///
    /// If the cache is empty, loads from file. If already cached, returns
    /// the cached value without file I/O.
    pub fn get(&self) -> Result<ModelConfigCache> {
        // Fast path: read lock if cached
        {
            let cache = self.cache.read();
            if let Some(config) = cache.as_ref() {
                debug!(
                    config_path = %self.config_path.display(),
                    "Returning cached model configuration"
                );
                return Ok(config.clone());
            }
        }

        // Slow path: load from file with write lock
        let config = ModelConfigCache::from_file(&self.config_path)?;
        config.validate()?;

        info!(
            config_path = %self.config_path.display(),
            vocab_size = config.vocab_size,
            hidden_size = config.hidden_size,
            num_layers = config.num_hidden_layers,
            "Loaded model configuration into cache"
        );

        // Update cache
        {
            let mut cache = self.cache.write();
            *cache = Some(config.clone());
        }

        Ok(config)
    }

    /// Get the cached configuration if available, without loading
    pub fn get_cached(&self) -> Option<ModelConfigCache> {
        self.cache.read().clone()
    }

    /// Force reload configuration from file
    pub fn reload(&self) -> Result<ModelConfigCache> {
        let config = ModelConfigCache::from_file(&self.config_path)?;
        config.validate()?;

        info!(
            config_path = %self.config_path.display(),
            vocab_size = config.vocab_size,
            "Reloaded model configuration from file"
        );

        {
            let mut cache = self.cache.write();
            *cache = Some(config.clone());
        }

        Ok(config)
    }

    /// Clear the cache
    pub fn clear(&self) {
        let mut cache = self.cache.write();
        *cache = None;
        debug!("Cleared model configuration cache");
    }

    /// Check if configuration is cached
    pub fn is_cached(&self) -> bool {
        self.cache.read().is_some()
    }

    /// Get cache statistics for monitoring
    pub fn cache_status(&self) -> CacheStatus {
        let is_cached = self.cache.read().is_some();
        CacheStatus {
            is_cached,
            config_path: self.config_path.clone(),
        }
    }
}

impl Clone for ModelConfigCacheManager {
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
            config_path: self.config_path.clone(),
        }
    }
}

/// Cache status information
#[derive(Debug, Clone)]
pub struct CacheStatus {
    pub is_cached: bool,
    pub config_path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = ModelConfigCache::new(32000, 4096, 32, 32, 8, 11008, 2048, 10000.0);
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!(config.vocab_size, 32000);
        assert_eq!(config.hidden_size, 4096);
        assert_eq!(config.head_dim, 128);
        assert_eq!(config.num_heads_per_kv_head, 4);
    }

    #[test]
    fn test_config_validation_dimension_check() {
        // hidden_size not divisible by num_attention_heads
        let config = ModelConfigCache::new(32000, 4096, 32, 33, 8, 11008, 2048, 10000.0);
        assert!(config.is_err());
    }

    #[test]
    fn test_config_validation_kv_heads_check() {
        // num_attention_heads not divisible by num_key_value_heads
        let config = ModelConfigCache::new(32000, 4096, 32, 32, 7, 11008, 2048, 10000.0);
        assert!(config.is_err());
    }

    #[test]
    fn test_config_from_json() {
        let json = r#"{
            "vocab_size": 32000,
            "hidden_size": 4096,
            "num_hidden_layers": 32,
            "num_attention_heads": 32,
            "num_key_value_heads": 8,
            "intermediate_size": 11008,
            "max_position_embeddings": 2048,
            "rope_theta": 10000.0
        }"#;

        let config = ModelConfigCache::from_json(json);
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!(config.vocab_size, 32000);
        assert_eq!(config.hidden_size, 4096);
        assert_eq!(config.num_attention_heads, 32);
    }

    #[test]
    fn test_config_from_json_with_defaults() {
        let json = r#"{
            "vocab_size": 32000,
            "hidden_size": 4096,
            "num_hidden_layers": 32,
            "num_attention_heads": 32,
            "num_key_value_heads": 8,
            "intermediate_size": 11008
        }"#;

        let config = ModelConfigCache::from_json(json);
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!(config.max_position_embeddings, 2048);
        assert_eq!(config.rope_theta, 10000.0);
    }

    #[test]
    fn test_adapter_memory_estimation() {
        let config = ModelConfigCache::new(32000, 4096, 32, 32, 8, 11008, 2048, 10000.0)
            .unwrap();

        // LoRA rank=16 with 4 target modules
        let bytes = config.estimate_adapter_memory(16, 4);
        // 4 modules × 2 × 16 rank × 4096 hidden × 4 bytes/f32
        let expected = 4 * 2 * 16 * 4096 * 4;
        assert_eq!(bytes, expected);
    }

    #[test]
    fn test_cache_manager_lazy_loading() {
        // Create temporary config file for testing
        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("test_config.json");

        let config_json = r#"{
            "vocab_size": 32000,
            "hidden_size": 4096,
            "num_hidden_layers": 32,
            "num_attention_heads": 32,
            "num_key_value_heads": 8,
            "intermediate_size": 11008,
            "max_position_embeddings": 2048,
            "rope_theta": 10000.0
        }"#;

        std::fs::write(&config_path, config_json).expect("Failed to write test config");

        let manager = ModelConfigCacheManager::new(&config_path);

        // Initially not cached
        assert!(!manager.is_cached());

        // First access loads from file
        let config1 = manager.get().expect("Failed to load config");
        assert!(manager.is_cached());
        assert_eq!(config1.vocab_size, 32000);

        // Second access uses cache
        let config2 = manager.get().expect("Failed to get cached config");
        assert_eq!(config2.vocab_size, config1.vocab_size);

        // Cleanup
        std::fs::remove_file(&config_path).expect("Failed to clean up test file");
    }

    #[test]
    fn test_cache_manager_reload() {
        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("test_config_reload.json");

        let initial_json = r#"{
            "vocab_size": 32000,
            "hidden_size": 4096,
            "num_hidden_layers": 32,
            "num_attention_heads": 32,
            "num_key_value_heads": 8,
            "intermediate_size": 11008,
            "max_position_embeddings": 2048,
            "rope_theta": 10000.0
        }"#;

        std::fs::write(&config_path, initial_json).expect("Failed to write test config");

        let manager = ModelConfigCacheManager::new(&config_path);
        let config1 = manager.get().expect("Failed to load config");
        assert_eq!(config1.vocab_size, 32000);

        // Update file
        let updated_json = r#"{
            "vocab_size": 50000,
            "hidden_size": 4096,
            "num_hidden_layers": 32,
            "num_attention_heads": 32,
            "num_key_value_heads": 8,
            "intermediate_size": 11008,
            "max_position_embeddings": 2048,
            "rope_theta": 10000.0
        }"#;

        std::fs::write(&config_path, updated_json).expect("Failed to update test config");

        // Old cache still returns old value
        let config2 = manager.get_cached().unwrap();
        assert_eq!(config2.vocab_size, 32000);

        // Reload forces fresh load
        let config3 = manager.reload().expect("Failed to reload config");
        assert_eq!(config3.vocab_size, 50000);

        // Cleanup
        std::fs::remove_file(&config_path).expect("Failed to clean up test file");
    }
}
