//! Model loading from SafeTensors format with LRU caching
//!
//! This module provides functionality to load Qwen models from SafeTensors format,
//! which is the standard format used by MLX and other ML frameworks. Includes
//! LRU-based caching with eviction to optimize memory usage.
//!
//! Citations:
//! - SafeTensors loading: Based on `crates/adapteros-lora-worker/src/embeddings.rs:65-89`【1†adapteros-lora-worker/src/embeddings.rs:65-89】
//! - Model caching: Implements LRU cache with eviction per Memory Management Pattern【2†adapteros-memory/src/model_cache.rs:1-50】
//! - Deterministic eviction: Uses BLAKE3 hash tiebreaking【3†adapteros-memory/src/unified_interface.rs:390-395】

use memmap2::Mmap;
use adapteros_core::{AosError, Result};
use adapteros_memory::{ModelCache, ModelCacheConfig};
use adapteros_secure_fs::{traversal::normalize_path, content::{validate_and_parse_json, validate_model_config_json}};
use safetensors::SafeTensors;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

/// Qwen model structure loaded from SafeTensors
#[derive(Debug, Clone)]
pub struct QwenModel {
    /// Embedding layer weights [vocab_size, hidden_size]
    pub embedding_weight: Vec<f32>,
    /// Language modeling head weights [hidden_size, vocab_size]
    pub lm_head_weight: Vec<f32>,
    /// Transformer layers
    pub layers: Vec<TransformerLayer>,
    /// Model configuration
    pub config: ModelConfig,
}

/// Individual transformer layer
#[derive(Debug, Clone)]
pub struct TransformerLayer {
    /// Self-attention weights
    pub self_attn_weight: Vec<f32>,
    /// MLP weights
    pub mlp_weight: Vec<f32>,
    /// Layer normalization weights
    pub norm_weight: Vec<f32>,
}

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub vocab_size: usize,
    pub hidden_size: usize,
    pub num_layers: usize,
    pub num_attention_heads: usize,
    pub num_key_value_heads: usize,
    pub intermediate_size: usize,
    pub rope_theta: f32,
    pub max_position_embeddings: usize,
}

/// Model loader for SafeTensors format with LRU caching
pub struct ModelLoader {
    model_path: std::path::PathBuf,
    /// LRU cache for loaded models
    cache: Option<Arc<ModelCache<String, QwenModel>>>,
}

impl ModelLoader {
    /// Create a new model loader without caching
    pub fn new<P: AsRef<Path>>(model_path: P) -> Self {
        Self {
            model_path: model_path.as_ref().to_path_buf(),
            cache: None,
        }
    }

    /// Create a new model loader with caching enabled
    pub fn with_cache<P: AsRef<Path>>(model_path: P, cache_config: ModelCacheConfig) -> Self {
        Self {
            model_path: model_path.as_ref().to_path_buf(),
            cache: Some(Arc::new(ModelCache::with_config(cache_config))),
        }
    }

    /// Enable caching on an existing loader
    pub fn enable_cache(&mut self, config: ModelCacheConfig) {
        self.cache = Some(Arc::new(ModelCache::with_config(config)));
    }

    /// Disable caching
    pub fn disable_cache(&mut self) {
        self.cache = None;
    }

    /// Get cache metrics (if caching is enabled)
    pub fn cache_metrics(&self) -> Option<Arc<adapteros_memory::ModelCacheMetrics>> {
        self.cache.as_ref().map(|c| c.metrics())
    }

    /// Load Qwen model from SafeTensors format with caching
    ///
    /// Uses LRU cache to avoid reloading models that are already in memory.
    /// Falls back to disk loading if cache miss occurs.
    pub fn load_qwen_model_cached(&self, tenant_id: Option<&str>) -> Result<Arc<QwenModel>> {
        if let Some(ref cache) = self.cache {
            // Create cache key from model path
            let cache_key = self.model_path.to_string_lossy().to_string();

            // Check cache first
            if let Some(cached_entry) = cache.get(&cache_key) {
                tracing::debug!(cache_key = %cache_key, "Model cache hit");
                return Ok(Arc::clone(&cached_entry.model));
            }

            // Cache miss - load from disk
            tracing::debug!(cache_key = %cache_key, "Model cache miss, loading from disk");
            let model = Arc::new(self.load_qwen_model()?);

            // Estimate memory usage (rough approximation)
            let memory_usage = self.estimate_model_memory_usage(&model);
            let quality_score = 0.8; // Default quality score for models

            // Insert into cache
            cache.insert(
                cache_key,
                Arc::clone(&model),
                memory_usage,
                tenant_id.map(|s| s.to_string()),
                quality_score,
            )?;

            Ok(model)
        } else {
            // No caching enabled, load directly
            Ok(Arc::new(self.load_qwen_model()?))
        }
    }

    /// Load Qwen model from SafeTensors format (bypasses cache)
    pub fn load_qwen_model(&self) -> Result<QwenModel> {
        let safetensors_path = self.model_path.join("model.safetensors");

        // Check if model file exists
        if !safetensors_path.exists() {
            return Err(AosError::Worker(format!(
                "Model file not found: {:?}",
                safetensors_path
            )));
        }

        // Load SafeTensors file
        let file = File::open(&safetensors_path)
            .map_err(|e| AosError::Worker(format!("Failed to open model file: {}", e)))?;

        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| AosError::Worker(format!("Failed to mmap model file: {}", e)))?;

        let tensors = SafeTensors::deserialize(&mmap)
            .map_err(|e| AosError::Worker(format!("Failed to parse SafeTensors: {}", e)))?;

        // Load model configuration
        let config = self.load_config()?;

        // Load embedding weights
        let embedding_weight = self.load_tensor(&tensors, "model.embed_tokens.weight")?;

        // Load language modeling head weights
        let lm_head_weight = self.load_tensor(&tensors, "lm_head.weight")?;

        // Load transformer layers
        let mut layers = Vec::new();
        for i in 0..config.num_layers {
            let layer = self.load_transformer_layer(&tensors, i)?;
            layers.push(layer);
        }

        Ok(QwenModel {
            embedding_weight,
            lm_head_weight,
            layers,
            config,
        })
    }

    /// Load model configuration from config.json
    fn load_config(&self) -> Result<ModelConfig> {
        let config_path = self.model_path.join("config.json");

        // Canonicalize path for security validation
        let canonical_config_path = normalize_path(&config_path)
            .map_err(|e| AosError::Worker(format!("Path security validation failed for config.json: {}", e)))?;

        if !canonical_config_path.exists() {
            // Return default config for Qwen2.5-7B
            return Ok(ModelConfig {
                vocab_size: 32000,
                hidden_size: 4096,
                num_layers: 32,
                num_attention_heads: 32,
                num_key_value_heads: 4,
                intermediate_size: 14336,
                rope_theta: 1000000.0,
                max_position_embeddings: 32768,
            });
        }

        let config_content = std::fs::read_to_string(&canonical_config_path)
            .map_err(|e| AosError::Worker(format!("Failed to read config: {}", e)))?;

        // Perform semantic validation for model config
        validate_model_config_json(&config_content)
            .map_err(|e| AosError::Worker(format!("Model config validation failed: {}", e)))?;

        let config: ModelConfig = validate_and_parse_json(&config_content, "config.json")
            .map_err(|e| AosError::Worker(format!("Config validation failed: {}", e)))?;

        Ok(config)
    }

    /// Load individual transformer layer
    fn load_transformer_layer(
        &self,
        tensors: &SafeTensors,
        layer_idx: usize,
    ) -> Result<TransformerLayer> {
        let prefix = format!("model.layers.{}", layer_idx);

        // Load self-attention weights
        let self_attn_weight =
            self.load_tensor(tensors, &format!("{}.self_attn.weight", prefix))?;

        // Load MLP weights
        let mlp_weight = self.load_tensor(tensors, &format!("{}.mlp.weight", prefix))?;

        // Load layer normalization weights
        let norm_weight =
            self.load_tensor(tensors, &format!("{}.input_layernorm.weight", prefix))?;

        Ok(TransformerLayer {
            self_attn_weight,
            mlp_weight,
            norm_weight,
        })
    }

    /// Load tensor from SafeTensors
    fn load_tensor(&self, tensors: &SafeTensors, name: &str) -> Result<Vec<f32>> {
        let tensor = tensors
            .tensor(name)
            .map_err(|e| AosError::Worker(format!("Tensor '{}' not found: {}", name, e)))?;

        // Convert tensor data to Vec<f32>
        let data = tensor.data();
        let float_data: Vec<f32> = data
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        Ok(float_data)
    }

    /// Validate model integrity
    pub fn validate_model(&self, model: &QwenModel) -> Result<()> {
        // Check embedding weight dimensions
        let expected_embedding_size = model.config.vocab_size * model.config.hidden_size;
        if model.embedding_weight.len() != expected_embedding_size {
            return Err(AosError::Worker(format!(
                "Embedding weight size mismatch: expected {}, got {}",
                expected_embedding_size,
                model.embedding_weight.len()
            )));
        }

        // Check LM head weight dimensions
        let expected_lm_head_size = model.config.hidden_size * model.config.vocab_size;
        if model.lm_head_weight.len() != expected_lm_head_size {
            return Err(AosError::Worker(format!(
                "LM head weight size mismatch: expected {}, got {}",
                expected_lm_head_size,
                model.lm_head_weight.len()
            )));
        }

        // Check layer count
        if model.layers.len() != model.config.num_layers {
            return Err(AosError::Worker(format!(
                "Layer count mismatch: expected {}, got {}",
                model.config.num_layers,
                model.layers.len()
            )));
        }

        Ok(())
    }

    /// Get model metadata
    pub fn get_model_info(&self) -> Result<ModelInfo> {
        let config = self.load_config()?;

        Ok(ModelInfo {
            model_path: self.model_path.clone(),
            vocab_size: config.vocab_size,
            hidden_size: config.hidden_size,
            num_layers: config.num_layers,
            total_parameters: self.estimate_parameter_count(&config),
        })
    }

    /// Estimate total parameter count
    fn estimate_parameter_count(&self, config: &ModelConfig) -> usize {
        // Embedding layer
        let embedding_params = config.vocab_size * config.hidden_size;

        // LM head
        let lm_head_params = config.hidden_size * config.vocab_size;

        // Transformer layers (approximate)
        let attention_params = config.num_layers * config.hidden_size * config.hidden_size * 4; // Q, K, V, O
        let mlp_params = config.num_layers * config.hidden_size * config.intermediate_size * 2; // gate, up

        embedding_params + lm_head_params + attention_params + mlp_params
    }

    /// Estimate memory usage of a loaded model in bytes
    ///
    /// Provides a rough estimate based on parameter count and data types.
    /// This is used for cache memory management and eviction decisions.
    fn estimate_model_memory_usage(&self, model: &QwenModel) -> u64 {
        // Each f32 parameter is 4 bytes
        let bytes_per_param = 4u64;

        // Count total parameters
        let total_params = self.estimate_parameter_count(&model.config);

        // Calculate base memory usage (parameters only)
        let base_memory = total_params as u64 * bytes_per_param;

        // Add overhead for model structure and metadata (~10% overhead)
        let overhead_factor = 1.1;

        (base_memory as f64 * overhead_factor) as u64
    }
}

/// Model information
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub model_path: std::path::PathBuf,
    pub vocab_size: usize,
    pub hidden_size: usize,
    pub num_layers: usize,
    pub total_parameters: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_model_loader_creation() {
        let temp_dir = tempdir().expect("Test temp directory creation should succeed");
        let loader = ModelLoader::new(temp_dir.path());

        // Test that we can create a loader
        assert!(loader.model_path.exists());
        assert!(loader.cache.is_none()); // No cache by default
    }

    #[test]
    fn test_model_loader_with_cache() {
        let temp_dir = tempdir().expect("Test temp directory creation should succeed");
        let cache_config = ModelCacheConfig {
            max_memory_bytes: 1024 * 1024, // 1MB
            max_models: 5,
            headroom_threshold: 15.0,
            tenant_aware: false,
            eviction_batch_size: 2,
        };

        let loader = ModelLoader::with_cache(temp_dir.path(), cache_config);

        // Test that cache is enabled
        assert!(loader.cache.is_some());
        assert!(loader.model_path.exists());
    }

    #[test]
    fn test_config_loading_default() {
        let temp_dir = tempdir().expect("Test temp directory creation should succeed");
        let loader = ModelLoader::new(temp_dir.path());

        let config = loader.load_config().expect("Test config loading should succeed");
        assert_eq!(config.vocab_size, 32000);
        assert_eq!(config.hidden_size, 4096);
        assert_eq!(config.num_layers, 32);
    }

    #[test]
    fn test_parameter_count_estimation() {
        let temp_dir = tempdir().expect("Test temp directory creation should succeed");
        let loader = ModelLoader::new(temp_dir.path());

        let info = loader.get_model_info().expect("Test model info retrieval should succeed");

        // Qwen2.5-7B should have approximately 7 billion parameters
        assert!(info.total_parameters > 6_000_000_000);
        assert!(info.total_parameters < 8_000_000_000);
    }
}
