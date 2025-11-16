//! Embedding generation for document chunks
//!
//! Provides deterministic embedding generation for document chunks.
//! In production, this should integrate with a proper embedding model,
//! but for now we provide a simple feature-based embedding for testing.

use adapteros_core::{AosError, B3Hash, Result};
use std::sync::Arc;
use tokenizers::Tokenizer;
use tracing::debug;

/// Embedding dimension (standard for many models)
pub const EMBEDDING_DIMENSION: usize = 384;

/// Embedding model trait for document chunks
pub trait EmbeddingModel: Send + Sync {
    /// Encode text into an embedding vector
    fn encode_text(&self, text: &str) -> Result<Vec<f32>>;

    /// Get the model hash for determinism tracking
    fn model_hash(&self) -> B3Hash;

    /// Get the embedding dimension
    fn dimension(&self) -> usize;
}

/// Simple feature-based embedding generator
///
/// This is a placeholder implementation that generates embeddings based on
/// token features. In production, replace this with a proper embedding model
/// like sentence-transformers or OpenAI embeddings.
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
            "Generated embedding for text of {} tokens, magnitude={:.6}",
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
    fn test_simple_embedding_deterministic() {
        let tokenizer = tokenizers::Tokenizer::from_pretrained("bert-base-uncased", None)
            .expect("Failed to load tokenizer");
        let model = SimpleEmbeddingModel::new(Arc::new(tokenizer));

        let text = "This is a test document for embedding generation.";

        let emb1 = model.encode_text(text).expect("Failed to encode");
        let emb2 = model.encode_text(text).expect("Failed to encode");

        assert_eq!(emb1.len(), EMBEDDING_DIMENSION);
        assert_eq!(emb1, emb2, "Embeddings should be deterministic");

        // Check normalization
        let magnitude: f32 = emb1.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!(
            (magnitude - 1.0).abs() < 1e-5,
            "Embedding should be normalized"
        );
    }

    #[test]
    fn test_empty_text_embedding() {
        let tokenizer = tokenizers::Tokenizer::from_pretrained("bert-base-uncased", None)
            .expect("Failed to load tokenizer");
        let model = SimpleEmbeddingModel::new(Arc::new(tokenizer));

        let emb = model.encode_text("").expect("Failed to encode empty text");
        assert_eq!(emb.len(), EMBEDDING_DIMENSION);
        assert!(emb.iter().all(|&x| x == 0.0));
    }
}
