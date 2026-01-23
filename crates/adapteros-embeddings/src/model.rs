//! Core embedding model types and traits
//!
//! Defines the EmbeddingModel trait for pluggable embedding backends
//! and the Embedding type for vector representation with provenance tracking.

use adapteros_core::{B3Hash, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A single embedding vector with provenance tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    /// The embedding vector
    pub vector: Vec<f32>,
    /// Dimensionality of the embedding
    pub dimension: usize,
    /// Hash of the model that produced this embedding
    pub model_hash: B3Hash,
    /// Optional seed used for deterministic generation
    pub seed: Option<B3Hash>,
}

impl Embedding {
    /// Create a new embedding with provenance tracking
    pub fn new(vector: Vec<f32>, model_hash: B3Hash, seed: Option<B3Hash>) -> Self {
        let dimension = vector.len();
        Self {
            vector,
            dimension,
            model_hash,
            seed,
        }
    }

    /// Compute cosine similarity with another embedding
    pub fn cosine_similarity(&self, other: &Embedding) -> f32 {
        if self.dimension != other.dimension {
            return 0.0;
        }

        let dot: f32 = self
            .vector
            .iter()
            .zip(other.vector.iter())
            .map(|(a, b)| a * b)
            .sum();

        let norm_a: f32 = self.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = other.vector.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a < 1e-9 || norm_b < 1e-9 {
            return 0.0;
        }

        dot / (norm_a * norm_b)
    }
}

/// Trait for embedding model implementations
#[async_trait]
pub trait EmbeddingModel: Send + Sync {
    /// Encode a single text into an embedding
    fn encode(&self, text: &str) -> Result<Embedding>;

    /// Encode a batch of texts into embeddings
    fn encode_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>>;

    /// Get the embedding dimension
    fn dimension(&self) -> usize;

    /// Get the model hash for provenance tracking
    fn model_hash(&self) -> B3Hash;
}

/// Provider enum for selecting embedding backend
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmbeddingProvider {
    /// MLX-based embedding (production, Apple Silicon)
    Mlx,
    /// Mock provider for testing
    Mock,
}

impl Default for EmbeddingProvider {
    fn default() -> Self {
        Self::Mlx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_creation() {
        let vector = vec![0.1, 0.2, 0.3];
        let model_hash = B3Hash::hash(b"test-model");
        let embedding = Embedding::new(vector.clone(), model_hash, None);

        assert_eq!(embedding.dimension, 3);
        assert_eq!(embedding.vector, vector);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let model_hash = B3Hash::hash(b"test");
        let emb = Embedding::new(vec![1.0, 0.0, 0.0], model_hash, None);
        let similarity = emb.cosine_similarity(&emb);
        assert!((similarity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let model_hash = B3Hash::hash(b"test");
        let emb1 = Embedding::new(vec![1.0, 0.0, 0.0], model_hash, None);
        let emb2 = Embedding::new(vec![0.0, 1.0, 0.0], model_hash, None);
        let similarity = emb1.cosine_similarity(&emb2);
        assert!(similarity.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let model_hash = B3Hash::hash(b"test");
        let emb1 = Embedding::new(vec![1.0, 0.0], model_hash, None);
        let emb2 = Embedding::new(vec![-1.0, 0.0], model_hash, None);
        let similarity = emb1.cosine_similarity(&emb2);
        assert!((similarity + 1.0).abs() < 1e-6);
    }
}
