//! Deterministic replay types.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::sampling::SamplingParams;

/// Replay key containing all inputs needed for deterministic reproduction
///
/// This is the "recipe" for recreating an inference operation exactly.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayKey {
    /// BLAKE3 hash of the manifest used
    pub manifest_hash: String,
    /// Router seed for audit purposes (stored but currently unused)
    ///
    /// The router uses a deterministic algorithm (sorted by score, then by
    /// stable_id for tie-breaking). This seed is stored for audit trail purposes
    /// but does NOT currently affect routing decisions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_seed: Option<String>,
    /// Sampling parameters used
    pub sampler_params: SamplingParams,
    /// Backend used (CoreML, MLX, Metal)
    pub backend: String,
    /// Version of the sampling algorithm
    pub sampling_algorithm_version: String,
    /// BLAKE3 hash of sorted RAG document hashes (null if no RAG)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rag_snapshot_hash: Option<String>,
    /// Adapter IDs selected by router
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_ids: Option<Vec<String>>,
    /// Whether the inference ran in base-only mode (no adapters)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_only: Option<bool>,
    /// Dataset version ID for deterministic RAG replay (pins to specific dataset version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_version_id: Option<String>,
}

/// Replay availability status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplayStatus {
    /// Exact replay possible (all conditions match)
    Available,
    /// RAG context changed but documents exist
    Approximate,
    /// Some RAG documents are missing
    Degraded,
    /// Original inference failed (no replayable output)
    FailedInference,
    /// Replay metadata capture failed (record incomplete)
    FailedCapture,
    /// Critical components missing (manifest, backend)
    Unavailable,
}

impl std::fmt::Display for ReplayStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Available => write!(f, "available"),
            Self::Approximate => write!(f, "approximate"),
            Self::Degraded => write!(f, "degraded"),
            Self::FailedInference => write!(f, "failed_inference"),
            Self::FailedCapture => write!(f, "failed_capture"),
            Self::Unavailable => write!(f, "unavailable"),
        }
    }
}

/// Match status after replay execution
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMatchStatus {
    /// Token-for-token identical output
    Exact,
    /// Semantically similar but not identical
    Semantic,
    /// Significantly different output
    Divergent,
    /// Error during replay execution
    Error,
}

impl std::fmt::Display for ReplayMatchStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exact => write!(f, "exact"),
            Self::Semantic => write!(f, "semantic"),
            Self::Divergent => write!(f, "divergent"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Request to execute a deterministic replay
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayRequest {
    /// Inference ID to replay (lookup metadata by ID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_id: Option<String>,
    /// Alternatively, provide full replay key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_key: Option<ReplayKey>,
    /// Override prompt (uses stored prompt if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Allow approximate/degraded replay (default: false)
    #[serde(default)]
    pub allow_approximate: bool,
    /// Skip RAG retrieval (test pure model determinism)
    #[serde(default)]
    pub skip_rag: bool,
}

/// RAG reproducibility details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RagReproducibility {
    /// Score from 0.0 (no overlap) to 1.0 (all docs available)
    pub score: f32,
    /// Number of original documents still available
    pub matching_docs: usize,
    /// Total number of documents in original inference
    pub total_original_docs: usize,
    /// Document IDs that are no longer available
    pub missing_doc_ids: Vec<String>,
}

/// Details about response divergence
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DivergenceDetails {
    /// Character position where divergence was detected (None if exact match)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divergence_position: Option<usize>,
    /// Whether the backend changed from original
    pub backend_changed: bool,
    /// Whether the manifest hash changed
    pub manifest_changed: bool,
    /// Human-readable reasons for approximation
    pub approximation_reasons: Vec<String>,
}

/// Statistics from replay execution
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayStats {
    /// Estimated token count (~4 chars/token heuristic).
    /// Note: This is an approximation since the worker doesn't report actual token counts.
    /// Do not use for chargeback or precise token accounting.
    pub estimated_tokens: usize,
    /// Replay latency in milliseconds
    pub latency_ms: u64,
    /// Original inference latency (if recorded)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_latency_ms: Option<u64>,
}

/// Response from replay execution
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayResponse {
    /// Unique ID for this replay execution
    pub replay_id: String,
    /// Original inference ID that was replayed
    pub original_inference_id: String,
    /// Mode used for replay (exact, approximate, degraded)
    pub replay_mode: String,
    /// Generated response text
    pub response: String,
    /// Whether response was truncated to 64KB limit
    pub response_truncated: bool,
    /// Match status compared to original
    pub match_status: ReplayMatchStatus,
    /// RAG reproducibility details (if RAG was used)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rag_reproducibility: Option<RagReproducibility>,
    /// Divergence details (if not exact match)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divergence: Option<DivergenceDetails>,
    /// Original response for comparison
    pub original_response: String,
    /// Execution statistics
    pub stats: ReplayStats,
}

/// Response from checking replay availability
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayAvailabilityResponse {
    /// Inference ID checked
    pub inference_id: String,
    /// Current replay status
    pub status: ReplayStatus,
    /// Whether exact replay is possible
    pub can_replay_exact: bool,
    /// Whether approximate replay is possible
    pub can_replay_approximate: bool,
    /// Reasons why replay is unavailable (if applicable)
    pub unavailable_reasons: Vec<String>,
    /// Warnings about approximations (if approximate)
    pub approximation_warnings: Vec<String>,
    /// Warning if dataset version has changed since original inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_consistency_warning: Option<String>,
    /// The replay key (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_key: Option<ReplayKey>,
}

/// Single replay execution record for history
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayExecutionRecord {
    /// Replay execution ID
    pub id: String,
    /// Original inference ID
    pub original_inference_id: String,
    /// Mode used (exact, approximate, degraded)
    pub replay_mode: String,
    /// Match status result
    pub match_status: ReplayMatchStatus,
    /// RAG reproducibility score (if RAG used)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rag_reproducibility_score: Option<f32>,
    /// Execution timestamp (RFC3339)
    pub executed_at: String,
    /// User who executed the replay
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_by: Option<String>,
    /// Error message if match_status is Error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Response containing replay execution history
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReplayHistoryResponse {
    /// Original inference ID
    pub inference_id: String,
    /// List of replay executions
    pub executions: Vec<ReplayExecutionRecord>,
    /// Total count of executions
    pub total_count: usize,
}
