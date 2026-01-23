//! Embedding model trait and types
//!
//! Defines the EmbeddingModel trait for pluggable embedding backends
//! and the Embedding type for vector representation with provenance tracking.

use adapteros_core::{B3Hash, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A computed embedding with provenance tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    /// The embedding vector (normalized)
    pub vector: Vec<f32>,
    /// Hash of the model that produced this embedding
    pub model_hash: B3Hash,
    /// Hash of the input text
    pub input_hash: B3Hash,
}

impl Embedding {
    /// Create a new embedding with provenance tracking
    pub fn new(vector: Vec<f32>, model_hash: B3Hash, input_hash: B3Hash) -> Self {
        Self {
            vector,
            model_hash,
            input_hash,
        }
    }

    /// Compute hash of the embedding vector for determinism checks
    pub fn vector_hash(&self) -> B3Hash {
        let bytes: Vec<u8> = self.vector.iter().flat_map(|f| f.to_le_bytes()).collect();
        B3Hash::hash(&bytes)
    }

    /// Compute cosine similarity with another embedding
    pub fn cosine_similarity(&self, other: &Embedding) -> f32 {
        if self.vector.len() != other.vector.len() {
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

    /// Get the dimensionality of the embedding
    pub fn dimension(&self) -> usize {
        self.vector.len()
    }
}

/// Trait for embedding models with determinism guarantees
#[async_trait]
pub trait EmbeddingModel: Send + Sync {
    /// Embed a single text into an embedding
    async fn embed(&self, text: &str) -> Result<Embedding>;

    /// Embed a batch of texts into embeddings
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>>;

    /// Get the model hash for provenance tracking
    fn model_hash(&self) -> &B3Hash;

    /// Get the tokenizer hash for determinism verification
    fn tokenizer_hash(&self) -> &B3Hash;

    /// Get the embedding dimension
    fn embedding_dimension(&self) -> usize;
}

/// Provider that wraps an embedding model with configuration
pub struct EmbeddingProvider {
    model: Box<dyn EmbeddingModel>,
}

impl EmbeddingProvider {
    /// Create a new embedding provider with the given model
    pub fn new(model: Box<dyn EmbeddingModel>) -> Self {
        Self { model }
    }

    /// Embed a single text
    pub async fn embed(&self, text: &str) -> Result<Embedding> {
        self.model.embed(text).await
    }

    /// Embed a batch of texts
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
        self.model.embed_batch(texts).await
    }

    /// Get the underlying model's hash
    pub fn model_hash(&self) -> &B3Hash {
        self.model.model_hash()
    }

    /// Get the tokenizer hash
    pub fn tokenizer_hash(&self) -> &B3Hash {
        self.model.tokenizer_hash()
    }

    /// Get the embedding dimension
    pub fn embedding_dimension(&self) -> usize {
        self.model.embedding_dimension()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockModel {
        dim: usize,
        hash: B3Hash,
    }

    #[async_trait]
    impl EmbeddingModel for MockModel {
        async fn embed(&self, text: &str) -> Result<Embedding> {
            Ok(Embedding {
                vector: vec![0.0; self.dim],
                model_hash: self.hash.clone(),
                input_hash: B3Hash::hash(text.as_bytes()),
            })
        }

        async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
            let mut results = Vec::with_capacity(texts.len());
            for text in texts {
                results.push(self.embed(text).await?);
            }
            Ok(results)
        }

        fn model_hash(&self) -> &B3Hash {
            &self.hash
        }

        fn tokenizer_hash(&self) -> &B3Hash {
            &self.hash
        }

        fn embedding_dimension(&self) -> usize {
            self.dim
        }
    }

    #[tokio::test]
    async fn test_mock_model_embed() {
        let hash = B3Hash::hash(b"mock");
        let model = MockModel { dim: 384, hash };
        let emb = model.embed("hello").await.unwrap();
        assert_eq!(emb.vector.len(), 384);
    }

    #[tokio::test]
    async fn test_mock_model_batch() {
        let hash = B3Hash::hash(b"mock");
        let model = MockModel { dim: 384, hash };
        let texts = &["hello", "world", "test"];
        let embeddings = model.embed_batch(texts).await.unwrap();
        assert_eq!(embeddings.len(), 3);
        for emb in &embeddings {
            assert_eq!(emb.vector.len(), 384);
        }
    }

    #[tokio::test]
    async fn test_embedding_provider() {
        let hash = B3Hash::hash(b"mock");
        let model = MockModel {
            dim: 384,
            hash: hash.clone(),
        };
        let provider = EmbeddingProvider::new(Box::new(model));

        assert_eq!(provider.embedding_dimension(), 384);
        assert_eq!(provider.model_hash(), &hash);

        let emb = provider.embed("test").await.unwrap();
        assert_eq!(emb.dimension(), 384);
    }

    #[test]
    fn test_cosine_similarity() {
        let hash = B3Hash::hash(b"test");
        let e1 = Embedding {
            vector: vec![1.0, 0.0, 0.0],
            model_hash: hash.clone(),
            input_hash: hash.clone(),
        };
        let e2 = Embedding {
            vector: vec![1.0, 0.0, 0.0],
            model_hash: hash.clone(),
            input_hash: hash.clone(),
        };
        assert!((e1.cosine_similarity(&e2) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let hash = B3Hash::hash(b"test");
        let emb1 = Embedding::new(vec![1.0, 0.0, 0.0], hash.clone(), hash.clone());
        let emb2 = Embedding::new(vec![0.0, 1.0, 0.0], hash.clone(), hash.clone());
        let similarity = emb1.cosine_similarity(&emb2);
        assert!(similarity.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let hash = B3Hash::hash(b"test");
        let emb1 = Embedding::new(vec![1.0, 0.0], hash.clone(), hash.clone());
        let emb2 = Embedding::new(vec![-1.0, 0.0], hash.clone(), hash.clone());
        let similarity = emb1.cosine_similarity(&emb2);
        assert!((similarity + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_vector_hash_deterministic() {
        let hash = B3Hash::hash(b"test");
        let emb = Embedding::new(vec![1.0, 2.0, 3.0], hash.clone(), hash.clone());
        let h1 = emb.vector_hash();
        let h2 = emb.vector_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_vector_hash_differs() {
        let hash = B3Hash::hash(b"test");
        let emb1 = Embedding::new(vec![1.0, 2.0, 3.0], hash.clone(), hash.clone());
        let emb2 = Embedding::new(vec![1.0, 2.0, 4.0], hash.clone(), hash.clone());
        assert_ne!(emb1.vector_hash(), emb2.vector_hash());
    }

    #[test]
    fn test_input_hash_tracked() {
        let model_hash = B3Hash::hash(b"model");
        let input_hash = B3Hash::hash(b"hello world");
        let emb = Embedding::new(vec![0.5, 0.5], model_hash.clone(), input_hash.clone());
        assert_eq!(emb.input_hash, input_hash);
        assert_eq!(emb.model_hash, model_hash);
    }
}
