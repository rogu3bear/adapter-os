//! Embedding benchmark API types
//!
//! Types for embedding benchmark reports - retrieval quality metrics
//! and determinism verification results.

use serde::{Deserialize, Serialize};

/// Embedding benchmark report
///
/// Represents a single benchmark run with retrieval quality metrics
/// and determinism verification results.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EmbeddingBenchmarkReport {
    /// Unique report identifier
    pub report_id: String,
    /// ISO 8601 timestamp when benchmark was run
    pub timestamp: String,
    /// Human-readable model name
    pub model_name: String,
    /// BLAKE3 hash of the model (truncated for display)
    pub model_hash: String,
    /// Whether the model is a fine-tuned variant
    pub is_finetuned: bool,
    /// Corpus version identifier (e.g., "v1.2.0")
    pub corpus_version: String,
    /// Number of chunks in the corpus
    pub num_chunks: usize,
    /// Recall@10 metric (0.0-1.0)
    pub recall_at_10: f64,
    /// nDCG@10 metric (0.0-1.0)
    pub ndcg_at_10: f64,
    /// MRR@10 metric (0.0-1.0)
    pub mrr_at_10: f64,
    /// Whether determinism verification passed
    pub determinism_pass: bool,
    /// Number of determinism verification runs
    pub determinism_runs: usize,
}

/// Response for listing embedding benchmarks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EmbeddingBenchmarksResponse {
    #[serde(default = "crate::schema_version")]
    pub schema_version: String,
    pub benchmarks: Vec<EmbeddingBenchmarkReport>,
    pub total: usize,
}

/// Query parameters for listing embedding benchmarks
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams))]
pub struct EmbeddingBenchmarksQuery {
    /// Filter by model name (partial match)
    pub model_name: Option<String>,
    /// Maximum number of results (default: 50)
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
}
