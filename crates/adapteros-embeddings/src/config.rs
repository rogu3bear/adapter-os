//! Embedding model configuration
//!
//! Defines configuration types for embedding models including:
//! - Model paths and parameters
//! - Pooling strategies
//! - Normalization settings

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Pooling strategy for aggregating token embeddings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PoolingStrategy {
    /// Mean pooling (average of all token embeddings)
    #[default]
    Mean,
    /// CLS token pooling (first token embedding)
    Cls,
    /// Last token pooling
    LastToken,
}

/// Configuration for embedding model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Model name/identifier
    pub model_name: String,
    /// Path to model weights
    pub model_path: PathBuf,
    /// Path to tokenizer
    pub tokenizer_path: PathBuf,
    /// Embedding dimension
    pub embedding_dim: usize,
    /// Maximum sequence length
    pub max_seq_length: usize,
    /// Batch size for inference
    pub batch_size: usize,
    /// Normalize embeddings to unit length
    pub normalize: bool,
    /// Pooling strategy for token aggregation
    #[serde(default)]
    pub pooling_strategy: PoolingStrategy,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model_name: "nomic-embed-text-v1.5".to_string(),
            model_path: PathBuf::from("./models/nomic-embed"),
            tokenizer_path: PathBuf::from("./models/nomic-embed/tokenizer.json"),
            embedding_dim: 768,
            max_seq_length: 8192,
            batch_size: 32,
            normalize: true,
            pooling_strategy: PoolingStrategy::default(),
        }
    }
}

impl EmbeddingConfig {
    /// Load from TOML file
    pub fn from_file(path: &std::path::Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Create a builder for constructing config
    pub fn builder() -> EmbeddingConfigBuilder {
        EmbeddingConfigBuilder::default()
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if self.model_name.is_empty() {
            return Err(ConfigValidationError::EmptyModelName);
        }
        if self.embedding_dim == 0 {
            return Err(ConfigValidationError::InvalidEmbeddingDim(0));
        }
        if self.max_seq_length == 0 {
            return Err(ConfigValidationError::InvalidMaxSeqLength(0));
        }
        if self.batch_size == 0 {
            return Err(ConfigValidationError::InvalidBatchSize(0));
        }
        Ok(())
    }
}

/// Builder for EmbeddingConfig
#[derive(Debug, Clone, Default)]
pub struct EmbeddingConfigBuilder {
    model_name: Option<String>,
    model_path: Option<PathBuf>,
    tokenizer_path: Option<PathBuf>,
    embedding_dim: Option<usize>,
    max_seq_length: Option<usize>,
    batch_size: Option<usize>,
    normalize: Option<bool>,
    pooling_strategy: Option<PoolingStrategy>,
}

impl EmbeddingConfigBuilder {
    /// Set model name
    pub fn model_name(mut self, name: impl Into<String>) -> Self {
        self.model_name = Some(name.into());
        self
    }

    /// Set model path
    pub fn model_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.model_path = Some(path.into());
        self
    }

    /// Set tokenizer path
    pub fn tokenizer_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.tokenizer_path = Some(path.into());
        self
    }

    /// Set embedding dimension
    pub fn embedding_dim(mut self, dim: usize) -> Self {
        self.embedding_dim = Some(dim);
        self
    }

    /// Set maximum sequence length
    pub fn max_seq_length(mut self, length: usize) -> Self {
        self.max_seq_length = Some(length);
        self
    }

    /// Set batch size
    pub fn batch_size(mut self, size: usize) -> Self {
        self.batch_size = Some(size);
        self
    }

    /// Set normalization flag
    pub fn normalize(mut self, normalize: bool) -> Self {
        self.normalize = Some(normalize);
        self
    }

    /// Set pooling strategy
    pub fn pooling_strategy(mut self, strategy: PoolingStrategy) -> Self {
        self.pooling_strategy = Some(strategy);
        self
    }

    /// Build the config with defaults for missing fields
    pub fn build(self) -> EmbeddingConfig {
        let default = EmbeddingConfig::default();
        EmbeddingConfig {
            model_name: self.model_name.unwrap_or(default.model_name),
            model_path: self.model_path.unwrap_or(default.model_path),
            tokenizer_path: self.tokenizer_path.unwrap_or(default.tokenizer_path),
            embedding_dim: self.embedding_dim.unwrap_or(default.embedding_dim),
            max_seq_length: self.max_seq_length.unwrap_or(default.max_seq_length),
            batch_size: self.batch_size.unwrap_or(default.batch_size),
            normalize: self.normalize.unwrap_or(default.normalize),
            pooling_strategy: self.pooling_strategy.unwrap_or(default.pooling_strategy),
        }
    }
}

/// Error type for configuration validation
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigValidationError {
    #[error("model name cannot be empty")]
    EmptyModelName,
    #[error("invalid embedding dimension: {0}")]
    InvalidEmbeddingDim(usize),
    #[error("invalid max sequence length: {0}")]
    InvalidMaxSeqLength(usize),
    #[error("invalid batch size: {0}")]
    InvalidBatchSize(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.embedding_dim, 768);
        assert_eq!(config.model_name, "nomic-embed-text-v1.5");
        assert!(config.normalize);
        assert_eq!(config.pooling_strategy, PoolingStrategy::Mean);
    }

    #[test]
    fn test_config_serialization() {
        let config = EmbeddingConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: EmbeddingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model_name, config.model_name);
        assert_eq!(parsed.embedding_dim, config.embedding_dim);
    }

    #[test]
    fn test_config_builder() {
        let config = EmbeddingConfig::builder()
            .model_name("custom-model")
            .embedding_dim(384)
            .normalize(false)
            .pooling_strategy(PoolingStrategy::Cls)
            .build();

        assert_eq!(config.model_name, "custom-model");
        assert_eq!(config.embedding_dim, 384);
        assert!(!config.normalize);
        assert_eq!(config.pooling_strategy, PoolingStrategy::Cls);
    }

    #[test]
    fn test_config_validation() {
        let mut config = EmbeddingConfig::default();
        assert!(config.validate().is_ok());

        config.model_name = String::new();
        assert_eq!(
            config.validate(),
            Err(ConfigValidationError::EmptyModelName)
        );

        config.model_name = "test".to_string();
        config.embedding_dim = 0;
        assert_eq!(
            config.validate(),
            Err(ConfigValidationError::InvalidEmbeddingDim(0))
        );
    }

    #[test]
    fn test_pooling_strategy_serialization() {
        let mean = PoolingStrategy::Mean;
        let json = serde_json::to_string(&mean).unwrap();
        assert_eq!(json, "\"mean\"");

        let cls = PoolingStrategy::Cls;
        let json = serde_json::to_string(&cls).unwrap();
        assert_eq!(json, "\"cls\"");
    }

    #[test]
    fn test_config_toml_serialization() {
        let config = EmbeddingConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: EmbeddingConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.model_name, config.model_name);
    }
}
