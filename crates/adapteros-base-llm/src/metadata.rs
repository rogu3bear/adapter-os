//! Base LLM metadata definitions
//!
//! Defines metadata structures for foundation models following
//! the patterns established in the existing codebase.

use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};

/// Base LLM metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseLLMMetadata {
    /// Model identifier (e.g., "Qwen2.5-7B-Instruct")
    pub model_id: String,

    /// Model hash for verification
    pub model_hash: String,

    /// Model architecture
    pub arch: ModelArchitecture,

    /// Vocabulary size
    pub vocab_size: usize,

    /// Hidden dimension size
    pub hidden_dim: usize,

    /// Number of transformer layers
    pub n_layers: usize,

    /// Number of attention heads
    pub n_heads: usize,
}

/// Model architecture types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelArchitecture {
    /// Qwen2 architecture
    Qwen2,
    /// Llama architecture
    Llama,
    /// GPT architecture
    GPT,
    /// Custom architecture
    Custom(String),
}

impl BaseLLMMetadata {
    /// Create new metadata
    pub fn new(
        model_id: String,
        arch: ModelArchitecture,
        vocab_size: usize,
        hidden_dim: usize,
        n_layers: usize,
        n_heads: usize,
    ) -> Self {
        // Compute model hash from metadata
        let metadata_json = serde_json::to_string(&serde_json::json!({
            "model_id": model_id,
            "arch": arch,
            "vocab_size": vocab_size,
            "hidden_dim": hidden_dim,
            "n_layers": n_layers,
            "n_heads": n_heads,
        }))
        .unwrap();

        let model_hash = B3Hash::hash(metadata_json.as_bytes()).to_string();

        Self {
            model_id,
            model_hash,
            arch,
            vocab_size,
            hidden_dim,
            n_layers,
            n_heads,
        }
    }

    /// Get model size in parameters (approximate)
    pub fn parameter_count(&self) -> usize {
        // Approximate parameter count for transformer models
        // Embedding: vocab_size * hidden_dim
        // Layers: n_layers * (4 * hidden_dim^2 + 2 * hidden_dim * vocab_size)
        // Note: This is a simplified calculation and may not match exact model counts
        let embedding_params = self.vocab_size * self.hidden_dim;
        let layer_params = self.n_layers
            * (4 * self.hidden_dim * self.hidden_dim + 2 * self.hidden_dim * self.vocab_size);

        embedding_params + layer_params
    }

    /// Get model size in GB (approximate, FP16)
    pub fn size_gb(&self) -> f32 {
        let params = self.parameter_count();
        (params * 2) as f32 / 1_000_000_000.0 // 2 bytes per FP16 parameter
    }

    /// Verify metadata integrity
    pub fn verify(&self) -> bool {
        // Recompute hash and verify
        let metadata_json = serde_json::to_string(&serde_json::json!({
            "model_id": self.model_id,
            "arch": self.arch,
            "vocab_size": self.vocab_size,
            "hidden_dim": self.hidden_dim,
            "n_layers": self.n_layers,
            "n_heads": self.n_heads,
        }))
        .unwrap();

        let computed_hash = B3Hash::hash(metadata_json.as_bytes()).to_string();
        computed_hash == self.model_hash
    }
}

impl Default for BaseLLMMetadata {
    fn default() -> Self {
        Self::new(
            "Qwen2.5-7B-Instruct".to_string(),
            ModelArchitecture::Qwen2,
            152064,
            3584,
            28,
            28,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_creation() {
        let metadata = BaseLLMMetadata::new(
            "test-model".to_string(),
            ModelArchitecture::Qwen2,
            1000,
            512,
            4,
            8,
        );

        assert_eq!(metadata.model_id, "test-model");
        assert_eq!(metadata.vocab_size, 1000);
        assert_eq!(metadata.hidden_dim, 512);
        assert_eq!(metadata.n_layers, 4);
        assert_eq!(metadata.n_heads, 8);
        assert!(!metadata.model_hash.is_empty());
    }

    #[test]
    fn test_metadata_verification() {
        let metadata = BaseLLMMetadata::default();
        assert!(metadata.verify());
    }

    #[test]
    fn test_parameter_count() {
        let metadata = BaseLLMMetadata::default();
        let param_count = metadata.parameter_count();
        assert!(param_count > 0);

        // Based on our calculation, this should be approximately 32.5B parameters
        assert!(param_count > 30_000_000_000);
        assert!(param_count < 35_000_000_000);
    }

    #[test]
    fn test_size_gb() {
        let metadata = BaseLLMMetadata::default();
        let size_gb = metadata.size_gb();
        assert!(size_gb > 0.0);

        // Based on our calculation, this should be approximately 65GB in FP16
        assert!(size_gb > 60.0);
        assert!(size_gb < 70.0);
    }
}
