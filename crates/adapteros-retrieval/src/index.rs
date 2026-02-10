//! Index backends for vector search
//!
//! Provides vector index implementations with deterministic tie-breaking
//! for reproducible search results.

use adapteros_core::{AosError, B3Hash, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Search result with deterministic ordering
///
/// Results are ordered by score (descending), with chunk_id as tie-breaker (ascending)
/// to ensure reproducible results across runs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResult {
    /// Unique identifier for the matched chunk
    pub chunk_id: String,
    /// Similarity score (higher is better)
    pub score: f32,
    /// Rank in the result set (0-indexed)
    pub rank: usize,
}

/// Index metadata for receipts and verification
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndexMetadata {
    /// Type of index (e.g., "flat", "hnsw")
    pub index_type: String,
    /// Hash of index parameters for determinism verification
    pub params_hash: B3Hash,
    /// Optional seed used during index construction
    pub build_seed: Option<u64>,
    /// Number of vectors in the index
    pub num_vectors: usize,
    /// Dimensionality of the vectors
    pub dimension: usize,
}

/// Trait for vector index backends
///
/// Index backends provide vector storage and similarity search.
/// Implementations must ensure deterministic results for the same
/// inputs and configuration.
#[async_trait]
pub trait IndexBackend: Send + Sync {
    /// Build the index from a set of embeddings
    ///
    /// # Arguments
    /// * `embeddings` - Tuples of (chunk_id, embedding_vector)
    ///
    /// # Returns
    /// Index metadata including dimension and vector count
    async fn build(&mut self, embeddings: &[(String, Vec<f32>)]) -> Result<IndexMetadata>;

    /// Search the index for similar vectors
    ///
    /// # Arguments
    /// * `query` - Query embedding vector
    /// * `top_k` - Maximum number of results to return
    ///
    /// # Returns
    /// Ranked search results with deterministic ordering
    async fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<SearchResult>>;

    /// Get index metadata
    fn metadata(&self) -> &IndexMetadata;
}

/// Flat index using brute-force exact search
///
/// This index computes cosine similarity against all vectors.
/// While O(n) per query, it provides exact results and serves
/// as a baseline for benchmarking approximate indexes.
pub struct FlatIndex {
    /// Stored embeddings as (chunk_id, vector) pairs
    embeddings: Vec<(String, Vec<f32>)>,
    /// Index metadata
    metadata: IndexMetadata,
}

impl FlatIndex {
    /// Create a new empty flat index
    pub fn new() -> Self {
        Self {
            embeddings: Vec::new(),
            metadata: IndexMetadata {
                index_type: "flat".to_string(),
                params_hash: B3Hash::zero(),
                build_seed: None,
                num_vectors: 0,
                dimension: 0,
            },
        }
    }

    /// Compute cosine similarity between two vectors
    ///
    /// Returns 0.0 for zero-length vectors or dimension mismatch.
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        // Handle zero-norm vectors
        if norm_a < 1e-9 || norm_b < 1e-9 {
            return 0.0;
        }

        dot / (norm_a * norm_b)
    }
}

impl Default for FlatIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IndexBackend for FlatIndex {
    async fn build(&mut self, embeddings: &[(String, Vec<f32>)]) -> Result<IndexMetadata> {
        if embeddings.is_empty() {
            return Err(AosError::Validation(
                "Cannot build index with zero embeddings — corpus may be empty or embedding generation failed".to_string(),
            ));
        }

        // Validate dimensions are consistent
        let dimension = embeddings[0].1.len();
        for (chunk_id, vec) in embeddings.iter() {
            if vec.len() != dimension {
                return Err(AosError::Validation(format!(
                    "Dimension mismatch for chunk '{}': expected {}, got {}",
                    chunk_id,
                    dimension,
                    vec.len()
                )));
            }
        }

        // Store embeddings
        self.embeddings = embeddings.to_vec();

        // Compute params hash for determinism verification
        let params_str = format!("flat:dim={}:n={}", dimension, embeddings.len());
        self.metadata = IndexMetadata {
            index_type: "flat".to_string(),
            params_hash: B3Hash::hash(params_str.as_bytes()),
            build_seed: None,
            num_vectors: embeddings.len(),
            dimension,
        };

        Ok(self.metadata.clone())
    }

    async fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<SearchResult>> {
        if self.embeddings.is_empty() {
            return Ok(Vec::new());
        }

        // Validate query dimension
        if !self.embeddings.is_empty() && query.len() != self.metadata.dimension {
            return Err(AosError::Validation(format!(
                "Query dimension mismatch: expected {}, got {}",
                self.metadata.dimension,
                query.len()
            )));
        }

        // Compute similarities for all vectors
        let mut scored: Vec<(&str, f32)> = self
            .embeddings
            .iter()
            .map(|(chunk_id, vec)| (chunk_id.as_str(), Self::cosine_similarity(query, vec)))
            .collect();

        // Sort with deterministic tie-breaking:
        // - Primary: score DESC (higher scores first)
        // - Secondary: chunk_id ASC (alphabetical for ties)
        scored.sort_by(|a, b| {
            match b.1.partial_cmp(&a.1) {
                Some(Ordering::Equal) | None => a.0.cmp(b.0), // chunk_id ASC for ties
                Some(ord) => ord,                             // score DESC
            }
        });

        // Take top-k and assign ranks
        let results: Vec<SearchResult> = scored
            .into_iter()
            .take(top_k)
            .enumerate()
            .map(|(rank, (chunk_id, score))| SearchResult {
                chunk_id: chunk_id.to_string(),
                score,
                rank,
            })
            .collect();

        Ok(results)
    }

    fn metadata(&self) -> &IndexMetadata {
        &self.metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_flat_index_deterministic() {
        let mut index = FlatIndex::new();
        let embeddings = vec![
            ("chunk1".to_string(), vec![1.0, 0.0, 0.0]),
            ("chunk2".to_string(), vec![0.0, 1.0, 0.0]),
            ("chunk3".to_string(), vec![0.7, 0.7, 0.0]),
        ];
        index.build(&embeddings).await.unwrap();

        let query = vec![1.0, 0.0, 0.0];
        let results1 = index.search(&query, 2).await.unwrap();
        let results2 = index.search(&query, 2).await.unwrap();

        assert_eq!(results1.len(), results2.len());
        for (r1, r2) in results1.iter().zip(&results2) {
            assert_eq!(r1.chunk_id, r2.chunk_id);
        }
        assert_eq!(results1[0].chunk_id, "chunk1");
    }

    #[tokio::test]
    async fn test_flat_index_tiebreaking() {
        let mut index = FlatIndex::new();
        let embeddings = vec![
            ("b_chunk".to_string(), vec![1.0, 0.0]),
            ("a_chunk".to_string(), vec![1.0, 0.0]),
            ("c_chunk".to_string(), vec![1.0, 0.0]),
        ];
        index.build(&embeddings).await.unwrap();

        let query = vec![1.0, 0.0];
        let results = index.search(&query, 3).await.unwrap();

        // Tie-breaking: chunk_id ASC
        assert_eq!(results[0].chunk_id, "a_chunk");
        assert_eq!(results[1].chunk_id, "b_chunk");
        assert_eq!(results[2].chunk_id, "c_chunk");
    }

    #[tokio::test]
    async fn test_flat_index_empty_returns_error() {
        let mut index = FlatIndex::new();
        let embeddings: Vec<(String, Vec<f32>)> = vec![];
        let result = index.build(&embeddings).await;

        assert!(result.is_err(), "building with zero embeddings should fail");
        let err = result.unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("zero embeddings"),
            "error should mention zero embeddings, got: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_flat_index_search_before_build() {
        let index = FlatIndex::new();
        let query = vec![1.0, 0.0, 0.0];
        let results = index.search(&query, 5).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_flat_index_metadata() {
        let mut index = FlatIndex::new();
        let embeddings = vec![
            ("chunk1".to_string(), vec![1.0, 0.0, 0.0, 0.0]),
            ("chunk2".to_string(), vec![0.0, 1.0, 0.0, 0.0]),
        ];
        index.build(&embeddings).await.unwrap();

        let metadata = index.metadata();
        assert_eq!(metadata.index_type, "flat");
        assert_eq!(metadata.num_vectors, 2);
        assert_eq!(metadata.dimension, 4);
        assert!(metadata.build_seed.is_none());
    }

    #[tokio::test]
    async fn test_flat_index_dimension_mismatch_build() {
        let mut index = FlatIndex::new();
        let embeddings = vec![
            ("chunk1".to_string(), vec![1.0, 0.0, 0.0]),
            ("chunk2".to_string(), vec![0.0, 1.0]), // Wrong dimension
        ];
        let result = index.build(&embeddings).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_flat_index_dimension_mismatch_query() {
        let mut index = FlatIndex::new();
        let embeddings = vec![
            ("chunk1".to_string(), vec![1.0, 0.0, 0.0]),
            ("chunk2".to_string(), vec![0.0, 1.0, 0.0]),
        ];
        index.build(&embeddings).await.unwrap();

        let query = vec![1.0, 0.0]; // Wrong dimension
        let result = index.search(&query, 2).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_flat_index_scores() {
        let mut index = FlatIndex::new();
        let embeddings = vec![
            ("exact".to_string(), vec![1.0, 0.0, 0.0]),
            ("partial".to_string(), vec![0.7, 0.7, 0.0]),
            ("orthogonal".to_string(), vec![0.0, 1.0, 0.0]),
        ];
        index.build(&embeddings).await.unwrap();

        let query = vec![1.0, 0.0, 0.0];
        let results = index.search(&query, 3).await.unwrap();

        // Exact match should have score ~1.0
        assert_eq!(results[0].chunk_id, "exact");
        assert!((results[0].score - 1.0).abs() < 1e-6);

        // Partial match should have score ~0.707
        assert_eq!(results[1].chunk_id, "partial");
        assert!((results[1].score - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);

        // Orthogonal should have score ~0.0
        assert_eq!(results[2].chunk_id, "orthogonal");
        assert!(results[2].score.abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_flat_index_ranks() {
        let mut index = FlatIndex::new();
        let embeddings = vec![
            ("chunk1".to_string(), vec![1.0, 0.0]),
            ("chunk2".to_string(), vec![0.8, 0.6]),
            ("chunk3".to_string(), vec![0.0, 1.0]),
        ];
        index.build(&embeddings).await.unwrap();

        let query = vec![1.0, 0.0];
        let results = index.search(&query, 3).await.unwrap();

        assert_eq!(results[0].rank, 0);
        assert_eq!(results[1].rank, 1);
        assert_eq!(results[2].rank, 2);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let sim = FlatIndex::cosine_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = FlatIndex::cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = FlatIndex::cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![0.0, 0.0, 0.0];
        let sim = FlatIndex::cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_dimension_mismatch() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0];
        let sim = FlatIndex::cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_flat_index_default() {
        let index = FlatIndex::default();
        assert!(index.embeddings.is_empty());
        assert_eq!(index.metadata.index_type, "flat");
    }
}
