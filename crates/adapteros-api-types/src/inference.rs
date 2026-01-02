//! Inference types

#[cfg(feature = "server")]
use adapteros_core::{backend::BackendKind, B3Hash};
#[cfg(feature = "server")]
use adapteros_types::{
    fusion::FusionInterval,
    inference::{InferRequest as RootInferRequest, RunReceipt as RootRunReceipt},
};
use serde::{Deserialize, Serialize};

use crate::{schema_version, RunEnvelope};

#[cfg(feature = "server")]
pub use adapteros_types::inference::{StopReasonCode, STOP_Q15_DENOM};

// =============================================================================
// Stop Controller Types (PRD: Hard Deterministic Stop Controller)
// =============================================================================

/// Default completion threshold in Q15 (~0.75 probability)
fn default_completion_threshold_q15() -> i16 {
    24576
}

/// Default n-gram size for repetition detection
fn default_repetition_ngram() -> u8 {
    3
}

/// Default window size for repetition detection
fn default_repetition_window() -> u16 {
    32
}

/// Stop policy specification for deterministic stopping behavior.
///
/// Configures thresholds and parameters for the stop controller.
/// The policy is hashed (BLAKE3) and committed to the receipt for audit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct StopPolicySpec {
    /// Hard cap on output tokens (overrides request max_tokens if lower)
    pub output_max_tokens: u32,

    /// Optional explicit EOS token ID (uses model default if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eos_token_id: Option<u32>,

    /// Q15 threshold for EOS probability triggering COMPLETION_CONFIDENT.
    /// Range: 0-32767 where 32767 = 1.0 probability.
    /// Default: 24576 (~0.75)
    #[serde(default = "default_completion_threshold_q15")]
    pub completion_threshold_q15: i16,

    /// N-gram size for repetition detection (minimum 3).
    /// Default: 3
    #[serde(default = "default_repetition_ngram")]
    pub repetition_ngram: u8,

    /// Sliding window size for repetition detection in tokens.
    /// Default: 32
    #[serde(default = "default_repetition_window")]
    pub repetition_window: u16,
}

impl Default for StopPolicySpec {
    fn default() -> Self {
        Self {
            output_max_tokens: 2048,
            eos_token_id: None,
            completion_threshold_q15: default_completion_threshold_q15(),
            repetition_ngram: default_repetition_ngram(),
            repetition_window: default_repetition_window(),
        }
    }
}

impl StopPolicySpec {
    /// Create a new StopPolicySpec with the given max tokens and defaults for other fields
    pub fn new(output_max_tokens: u32) -> Self {
        Self {
            output_max_tokens,
            ..Default::default()
        }
    }

    /// Compute BLAKE3 digest of this policy specification for audit commitment
    #[cfg(feature = "server")]
    pub fn digest(&self) -> B3Hash {
        let bytes = self.canonical_bytes();
        B3Hash::hash(&bytes)
    }

    /// Canonical byte representation for hashing (deterministic serialization)
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(32);
        bytes.extend_from_slice(&self.output_max_tokens.to_le_bytes());
        bytes.extend_from_slice(&self.eos_token_id.unwrap_or(0).to_le_bytes());
        bytes.extend_from_slice(&self.completion_threshold_q15.to_le_bytes());
        bytes.push(self.repetition_ngram);
        bytes.extend_from_slice(&self.repetition_window.to_le_bytes());
        bytes
    }

    /// Get completion threshold as f32 probability (0.0 to 1.0)
    #[cfg(feature = "server")]
    pub fn completion_threshold_f32(&self) -> f32 {
        self.completion_threshold_q15 as f32 / STOP_Q15_DENOM
    }
}

/// Inference request
#[cfg(feature = "server")]
pub type InferRequest = RootInferRequest<BackendKind, FusionInterval, StopPolicySpec>;

/// Inference response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct InferResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// Unique response identifier
    pub id: String,
    pub text: String,
    pub tokens: Vec<u32>,
    /// Number of tokens generated
    pub tokens_generated: usize,
    pub finish_reason: String,
    /// Latency in milliseconds (also available in trace)
    pub latency_ms: u64,
    /// Adapters used for this inference (also available in trace)
    pub adapters_used: Vec<String>,
    /// Verifiable run receipt for audit/replay
    #[cfg(feature = "server")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_receipt: Option<RunReceipt>,
    /// Deterministic receipt for audit/replay metadata.
    ///
    /// Includes seeds, resolved parameters, and execution selection. This is
    /// intended to be deterministic given the same prompt+system+params and
    /// runtime state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deterministic_receipt: Option<DeterministicReceipt>,
    /// Canonical run envelope describing the execution context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_envelope: Option<RunEnvelope>,
    /// Source citations for the response
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<Citation>,
    pub trace: InferenceTrace,
    /// Model used for inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Number of prompt tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<usize>,
    /// Error message if inference failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Pinned adapters that were unavailable for this inference
    ///
    /// These are adapters that were in the session's pinned set but were not
    /// available in the candidate adapter set. Returned for UI warning display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_pinned_adapters: Option<Vec<String>>,
    /// Routing fallback mode when pinned adapters are unavailable
    ///
    /// - `None`: All pinned adapters were available (or no pins configured)
    /// - `Some("partial")`: Some pinned adapters unavailable, using available pins + stack
    /// - `Some("stack_only")`: All pinned adapters unavailable, routing uses stack only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_routing_fallback: Option<String>,
    /// Backend used to execute the request (e.g., coreml, metal, mlx)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_used: Option<String>,
    /// Requested CoreML compute preference (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_compute_preference: Option<String>,
    /// CoreML compute units actually used (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_compute_units: Option<String>,
    /// Whether CoreML leveraged GPU for this inference (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_gpu_used: Option<bool>,
    /// Backend selected after fallback (if different from requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_backend: Option<String>,
    /// Whether backend fallback occurred during execution
    #[serde(default)]
    pub fallback_triggered: bool,
    /// Determinism mode that was applied after resolution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode_applied: Option<String>,
    /// Replay guarantee level for this inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_guarantee: Option<ReplayGuarantee>,

    // Stop Controller Fields (PRD: Hard Deterministic Stop Controller)
    /// Stop reason code explaining why generation terminated
    #[cfg(feature = "server")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_code: Option<StopReasonCode>,
    /// Token index at which the stop decision was made
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_token_index: Option<u32>,
    /// BLAKE3 digest of the StopPolicySpec used (hex encoded)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_policy_digest_b3: Option<String>,
}

/// Deterministic inference receipt (metadata only).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct DeterministicReceipt {
    /// Router seed (audit only; routing is deterministic by algorithm).
    pub router_seed: String,
    /// Sampling parameters applied for token generation.
    pub sampling_params: ReceiptSamplingParams,
    /// Stack identifier used to scope adapter routing (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    /// Adapter identifiers selected/used during inference.
    pub adapters_used: Vec<String>,
    /// Model identifier used for this inference (base model).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Backend used to execute the inference (e.g., coreml, metal, mlx).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_used: Option<String>,
    /// BLAKE3 digest of (prompt + system + params), hex encoded.
    #[cfg(feature = "server")]
    #[cfg_attr(feature = "server", schema(value_type = String))]
    pub prompt_system_params_digest_b3: B3Hash,
}

/// Sampling parameters applied for inference execution (receipt).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ReceiptSamplingParams {
    /// Maximum tokens to generate.
    pub max_tokens: usize,
    /// Sampling temperature.
    pub temperature: f32,
    /// Top-K sampling (None to disable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    /// Top-P nucleus sampling (None to disable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Random seed used for sampling (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

/// Character range for precise text location
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct CharRange {
    /// Start character offset
    pub start: u64,
    /// End character offset
    pub end: u64,
}

/// Bounding box for visual citations (e.g., PDF coordinates)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct BoundingBox {
    /// X coordinate
    pub x: f64,
    /// Y coordinate
    pub y: f64,
    /// Width
    pub width: f64,
    /// Height
    pub height: f64,
}

/// Citation metadata for a response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct Citation {
    /// Adapter that supplied the knowledge
    pub adapter_id: String,
    /// Path of the source file
    pub file_path: String,
    /// Chunk identifier within the source
    pub chunk_id: String,
    /// Byte offset where the chunk starts
    pub offset_start: u64,
    /// Byte offset where the chunk ends
    pub offset_end: u64,
    /// Short preview of the chunk text
    pub preview: String,
    /// Deterministic BLAKE3 hash of citation content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation_id: Option<String>,
    /// Page number within the document
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_number: Option<u32>,
    /// Character-level range for precise location
    #[serde(skip_serializing_if = "Option::is_none")]
    pub char_range: Option<CharRange>,
    /// Bounding box for visual citations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BoundingBox>,
    /// Relevance score for ranking citations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relevance_score: Option<f64>,
    /// Rank of this citation in the result set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
}

/// Replay guarantee level for an inference
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ReplayGuarantee {
    /// Exact replay possible (strict mode, seeded, no fallback/backend drift, no truncation)
    Exact,
    /// Replay approximate (seed missing or fallback/backend drift/truncation)
    Approximate,
    /// No replay guarantee (relaxed mode)
    None,
}

/// Inference trace for observability
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct InferenceTrace {
    pub adapters_used: Vec<String>,
    pub router_decisions: Vec<RouterDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_decision_chain: Option<Vec<RouterDecisionChainEntry>>,
    /// Fusion intervals and fused tensor hashes for determinism evidence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fusion_intervals: Option<Vec<FusionIntervalTrace>>,
    pub latency_ms: u64,
    /// Model type for the trace
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_type: Option<RouterModelType>,
}

/// Fusion interval boundary with fused tensor hash evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct FusionIntervalTrace {
    pub interval_id: String,
    pub start_token: usize,
    pub end_token: usize,
    #[cfg(feature = "server")]
    #[cfg_attr(feature = "server", schema(value_type = String))]
    pub fused_weight_hash: B3Hash,
}

/// Verifiable run receipt (hash chain over per-token decisions)
#[cfg(feature = "server")]
pub type RunReceipt = RootRunReceipt<B3Hash>;

/// Candidate adapter entry for router trace
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RouterCandidate {
    pub adapter_idx: u16,
    pub raw_score: f32,
    pub gate_q15: i16,
}

/// Routing model type for trace display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RouterModelType {
    Dense,
}

impl RouterModelType {
    pub fn dense() -> Self {
        RouterModelType::Dense
    }
}

/// Decision hash material for audit (mirrors router DecisionHash)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RouterDecisionHash {
    pub input_hash: String,
    pub output_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_hash: Option<String>,
    pub combined_hash: String,
    pub tau: f32,
    pub eps: f32,
    pub k: usize,
}

/// Chained router decision entry (per token)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RouterDecisionChainEntry {
    pub step: usize,
    pub input_token_id: Option<u32>,
    pub adapter_indices: Vec<u16>,
    pub adapter_ids: Vec<String>,
    pub gates_q15: Vec<i16>,
    pub entropy: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_hash: Option<RouterDecisionHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_hash: Option<String>,
    pub entry_hash: String,
    #[cfg(feature = "server")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "policy_mask_digest"
    )]
    #[cfg_attr(feature = "server", schema(value_type = String))]
    pub policy_mask_digest_b3: Option<B3Hash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_overrides_applied: Option<PolicyOverrideFlags>,
}

/// Router decision at a specific position (canonical schema)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RouterDecision {
    pub step: usize,
    pub input_token_id: Option<u32>,
    pub candidate_adapters: Vec<RouterCandidate>,
    pub entropy: f32,
    pub tau: f32,
    pub entropy_floor: f32,
    pub stack_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_mask: Option<Vec<bool>>,
    #[cfg(feature = "server")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "policy_mask_digest"
    )]
    #[cfg_attr(feature = "server", schema(value_type = String))]
    pub policy_mask_digest_b3: Option<B3Hash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_overrides_applied: Option<PolicyOverrideFlags>,
    /// Model type for this decision
    #[serde(default = "RouterModelType::dense")]
    pub model_type: RouterModelType,
}

/// Flags describing which policy overrides affected routing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct PolicyOverrideFlags {
    pub allow_list: bool,
    pub deny_list: bool,
    pub trust_state: bool,
}

/// KV cache usage statistics for receipt generation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct KvUsageStats {
    /// Tenant's allocated KV cache quota in bytes
    pub tenant_kv_quota_bytes: u64,
    /// Actual KV cache bytes used for this inference
    pub tenant_kv_bytes_used: u64,
    /// Number of KV cache evictions during inference
    pub kv_evictions: u32,
    /// ID of the residency policy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kv_residency_policy_id: Option<String>,
    /// Whether quota enforcement was active
    pub kv_quota_enforced: bool,
}
