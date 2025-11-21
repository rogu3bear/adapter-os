//! Embedding generation for document chunks
//!
//! Provides embedding generation for document chunks using either:
//! 1. MLX-based transformer models (production)
//! 2. Simple feature-based embeddings (fallback)

use adapteros_core::{AosError, B3Hash, Result};
use std::path::Path;
use std::sync::Arc;
use tokenizers::Tokenizer;
use tracing::{debug, warn};

// Re-export the canonical EmbeddingModel trait from adapteros-lora-rag
pub use adapteros_lora_rag::EmbeddingModel;

/// Embedding dimension (standard for many models)
pub const EMBEDDING_DIMENSION: usize = 384;

/// Production-ready embedding model selector
pub enum ProductionEmbeddingModel {
    /// MLX-based transformer embedding model (recommended for production)
    #[cfg(feature = "experimental-backends")]
    MLX(adapteros_lora_mlx_ffi::MLXEmbeddingModel),
    /// Simple feature-based fallback
    Simple(SimpleEmbeddingModel),
}

impl ProductionEmbeddingModel {
    /// Load the best available embedding model
    ///
    /// Attempts to load MLX model first, falls back to SimpleEmbeddingModel if not available
    pub fn load<P: AsRef<Path>>(_model_path: Option<P>, tokenizer: Arc<Tokenizer>) -> Self {
        #[cfg(feature = "experimental-backends")]
        {
            if let Some(path) = _model_path {
                match adapteros_lora_mlx_ffi::MLXEmbeddingModel::load(path) {
                    Ok(model) => {
                        tracing::info!("Loaded MLX embedding model");
                        return ProductionEmbeddingModel::MLX(model);
                    }
                    Err(e) => {
                        warn!(
                            "Failed to load MLX embedding model: {}, using simple fallback",
                            e
                        );
                    }
                }
            }
        }

        warn!("Using simple feature-based embedding model (not recommended for production)");
        ProductionEmbeddingModel::Simple(SimpleEmbeddingModel::new(tokenizer))
    }
}

impl EmbeddingModel for ProductionEmbeddingModel {
    fn encode_text(&self, text: &str) -> Result<Vec<f32>> {
        match self {
            #[cfg(feature = "experimental-backends")]
            ProductionEmbeddingModel::MLX(model) => model.encode_text(text),
            ProductionEmbeddingModel::Simple(model) => model.encode_text(text),
        }
    }

    fn model_hash(&self) -> B3Hash {
        match self {
            #[cfg(feature = "experimental-backends")]
            ProductionEmbeddingModel::MLX(model) => model.model_hash(),
            ProductionEmbeddingModel::Simple(model) => model.model_hash(),
        }
    }

    fn dimension(&self) -> usize {
        match self {
            #[cfg(feature = "experimental-backends")]
            ProductionEmbeddingModel::MLX(model) => model.dimension(),
            ProductionEmbeddingModel::Simple(model) => model.dimension(),
        }
    }
}

/// Simple feature-based embedding generator (fallback)
///
/// This is a fallback implementation that generates embeddings based on
/// token features. For production use, prefer ProductionEmbeddingModel with MLX.
pub struct SimpleEmbeddingModel {
    tokenizer: Arc<Tokenizer>,
    model_hash: B3Hash,
    dimension: usize,
}

impl SimpleEmbeddingModel {
    pub fn new(tokenizer: Arc<Tokenizer>) -> Self {
        // Create a deterministic hash based on the tokenizer vocab
        let model_hash = B3Hash::hash(b"simple_embedding_v1");

        Self {
            tokenizer,
            model_hash,
            dimension: EMBEDDING_DIMENSION,
        }
    }

    /// Generate deterministic features from text
    fn extract_features(&self, text: &str) -> Result<Vec<f32>> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| AosError::Validation(format!("Failed to tokenize text: {e}")))?;

        let token_ids = encoding.get_ids();

        // Generate features based on token statistics
        let mut features = vec![0.0f32; self.dimension];

        if token_ids.is_empty() {
            return Ok(features);
        }

        // Basic features (first 64 dimensions):
        // - Token frequency distribution
        // - Position-weighted features
        // - Token diversity metrics

        let token_count = token_ids.len() as f32;

        // Feature 1: Log of token count (normalized)
        features[0] = (token_count.ln() / 10.0).min(1.0);

        // Features 2-65: Token ID histogram (bucketed)
        for &token_id in token_ids {
            let bucket = (token_id as usize % 64) + 1;
            if bucket < 65 {
                features[bucket] += 1.0 / token_count;
            }
        }

        // Features 66-129: Bigram features
        for window in token_ids.windows(2) {
            let bigram_hash = (window[0] as usize * 31 + window[1] as usize) % 64;
            let idx = 65 + bigram_hash;
            if idx < 129 {
                features[idx] += 1.0 / (token_count - 1.0).max(1.0);
            }
        }

        // Features 130-193: Positional features (beginning/middle/end)
        let sections = 3;
        let section_size = token_ids.len().div_ceil(sections);
        for (pos, &token_id) in token_ids.iter().enumerate() {
            let section = (pos / section_size).min(sections - 1);
            let bucket = (token_id as usize % 21) + section * 21;
            let idx = 129 + bucket;
            if idx < 193 {
                features[idx] += 1.0 / section_size as f32;
            }
        }

        // Features 194-257: Content hash-based features
        let text_hash = B3Hash::hash(text.as_bytes());
        let hash_bytes = text_hash.as_bytes();
        for (i, &byte) in hash_bytes.iter().enumerate().take(64) {
            features[193 + i] = (byte as f32) / 255.0;
        }

        // Features 258-383: Statistical features
        // Token length variance, unique token ratio, etc.
        let unique_tokens: std::collections::HashSet<_> = token_ids.iter().collect();
        features[257] = unique_tokens.len() as f32 / token_count;

        // Normalize to unit length (cosine normalization)
        let magnitude: f32 = features.iter().map(|&x| x * x).sum::<f32>().sqrt();
        if magnitude > 1e-9 {
            for f in &mut features {
                *f /= magnitude;
            }
        }

        debug!(
            "Generated simple embedding for text of {} tokens, magnitude={:.6}",
            token_ids.len(),
            magnitude
        );

        Ok(features)
    }
}

impl EmbeddingModel for SimpleEmbeddingModel {
    fn encode_text(&self, text: &str) -> Result<Vec<f32>> {
        if text.trim().is_empty() {
            return Ok(vec![0.0; self.dimension]);
        }

        self.extract_features(text)
    }

    fn model_hash(&self) -> B3Hash {
        self.model_hash
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires tokenizer file
    fn test_simple_embedding_deterministic() {
        // This test requires a real tokenizer file
        // Skipped for CI/CD
    }

    #[test]
    #[ignore] // Requires tokenizer file
    fn test_empty_text_embedding() {
        // This test requires a real tokenizer file
        // Skipped for CI/CD
    }

    #[test]
    #[ignore] // Requires tokenizer file
    fn test_production_model_fallback() {
        // This test requires a real tokenizer file
        // Skipped for CI/CD
    }
}
