//! Inference types

use adapteros_core::{backend::BackendKind, B3Hash, FusionInterval};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use adapteros_types::coreml::CoreMLMode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

// =============================================================================
// Stop Controller Types (PRD: Hard Deterministic Stop Controller)
// =============================================================================

/// Q15 denominator for stop controller thresholds (matches router pattern)
pub const STOP_Q15_DENOM: f32 = 32767.0;

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

/// Exhaustive stop reason codes for inference termination.
///
/// Every inference run MUST emit exactly one of these codes to explain
/// why generation stopped. This enables deterministic behavior auditing
/// and cost attribution.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StopReasonCode {
    /// Generation stopped due to max_tokens limit being reached
    Length,
    /// Hard budget cap exceeded (output_max_tokens in StopPolicySpec)
    BudgetMax,
    /// EOS token probability exceeded completion_threshold_q15
    CompletionConfident,
    /// N-gram repetition detected within sliding window
    RepetitionGuard,
}

impl std::fmt::Display for StopReasonCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Length => write!(f, "LENGTH"),
            Self::BudgetMax => write!(f, "BUDGET_MAX"),
            Self::CompletionConfident => write!(f, "COMPLETION_CONFIDENT"),
            Self::RepetitionGuard => write!(f, "REPETITION_GUARD"),
        }
    }
}

impl std::str::FromStr for StopReasonCode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "LENGTH" => Ok(Self::Length),
            "BUDGET_MAX" => Ok(Self::BudgetMax),
            "COMPLETION_CONFIDENT" => Ok(Self::CompletionConfident),
            "REPETITION_GUARD" => Ok(Self::RepetitionGuard),
            _ => Err(format!("Unknown stop reason code: {}", s)),
        }
    }
}

/// Stop policy specification for deterministic stopping behavior.
///
/// Configures thresholds and parameters for the stop controller.
/// The policy is hashed (BLAKE3) and committed to the receipt for audit.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    pub fn completion_threshold_f32(&self) -> f32 {
        self.completion_threshold_q15 as f32 / STOP_Q15_DENOM
    }
}

/// Inference request
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct InferRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Per-request override for router determinism (e.g., "deterministic", "adaptive")
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
    /// Adapter stack identifier to use for inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    /// Optional domain hint to bias package/adapters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Fusion interval policy (per_request|per_segment|per_token)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub fusion_interval: Option<FusionInterval>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Explicit backend preference (auto|coreml|mlx|metal|cpu)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<BackendKind>,
    /// CoreML mode for backend selection (coreml_strict|coreml_preferred|backend_auto)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_mode: Option<CoreMLMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_evidence: Option<bool>,
    /// Explicit adapter list to use for inference (legacy field)
    ///
    /// Historically used as a "stack" placeholder, this now represents a concrete
    /// adapter list when provided. Prefer `stack_id` for named stacks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_stack: Option<Vec<String>>,
    /// Specific adapters to use (alternative to adapter_stack)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<String>>,
    /// Effective adapter set computed by the control plane (debug/audit only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_adapter_ids: Option<Vec<String>>,
    /// Chat session ID for trace linkage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Tenant ID (usually extracted from JWT claims, but can be explicit)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    /// Enable RAG context retrieval for this inference request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rag_enabled: Option<bool>,
    /// Collection ID for scoped RAG retrieval (requires rag_enabled = true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    /// Dataset version ID for deterministic dataset pinning
    ///
    /// When provided, the inference is scoped to this exact dataset version,
    /// enabling deterministic replay and explicit dataset pinning in receipts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_version_id: Option<String>,
    /// Stop policy specification for deterministic stop behavior
    ///
    /// If not provided, a default policy is constructed from max_tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_policy: Option<StopPolicySpec>,
}

/// Inference response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_receipt: Option<RunReceipt>,
    /// Deterministic receipt for audit/replay metadata.
    ///
    /// Includes seeds, resolved parameters, and execution selection. This is
    /// intended to be deterministic given the same prompt+system+params and
    /// runtime state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deterministic_receipt: Option<DeterministicReceipt>,
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    #[schema(value_type = String)]
    pub prompt_system_params_digest_b3: B3Hash,
}

/// Sampling parameters applied for inference execution (receipt).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct CharRange {
    /// Start character offset
    pub start: u64,
    /// End character offset
    pub end: u64,
}

/// Bounding box for visual citations (e.g., PDF coordinates)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
}

/// Fusion interval boundary with fused tensor hash evidence
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct FusionIntervalTrace {
    pub interval_id: String,
    pub start_token: usize,
    pub end_token: usize,
    #[schema(value_type = String)]
    pub fused_weight_hash: B3Hash,
}

/// Verifiable run receipt (hash chain over per-token decisions)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RunReceipt {
    pub trace_id: String,
    #[schema(value_type = String)]
    pub run_head_hash: B3Hash,
    #[schema(value_type = String)]
    pub output_digest: B3Hash,
    #[schema(value_type = String)]
    pub receipt_digest: B3Hash,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attestation: Option<String>,
    /// Total logical prompt tokens before cache reuse
    pub logical_prompt_tokens: u32,
    /// Tokens satisfied by prefix cache reuse
    pub prefix_cached_token_count: u32,
    /// Billed input tokens (logical - cached, floored at 0)
    pub billed_input_tokens: u32,
    /// Tokens produced logically (excludes eos)
    pub logical_output_tokens: u32,
    /// Billed output tokens (v1 = logical output tokens)
    pub billed_output_tokens: u32,
    // =========================================================================
    // Stop Controller Fields (PRD: Hard Deterministic Stop Controller)
    // =========================================================================
    /// Stop reason code explaining why generation terminated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_code: Option<StopReasonCode>,
    /// Token index at which the stop decision was made
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_token_index: Option<u32>,
    /// BLAKE3 digest of the StopPolicySpec used for this inference
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub stop_policy_digest_b3: Option<B3Hash>,
    // =========================================================================
    // KV Quota/Residency fields for evidence (PRD: KvResidencyAndQuotas v1)
    // =========================================================================
    /// Tenant's allocated KV cache quota in bytes
    #[serde(default)]
    pub tenant_kv_quota_bytes: u64,
    /// Actual KV cache bytes used for this inference
    #[serde(default)]
    pub tenant_kv_bytes_used: u64,
    /// Number of KV cache evictions that occurred during inference
    #[serde(default)]
    pub kv_evictions: u32,
    /// ID of the residency policy governing KV cache management
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kv_residency_policy_id: Option<String>,
    /// Whether KV quota enforcement was active for this inference
    #[serde(default)]
    pub kv_quota_enforced: bool,
    // =========================================================================
    // Prefix KV Cache fields (PRD: PrefixKvCache v1)
    // =========================================================================
    /// Cryptographic key for the prefix KV cache entry used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub prefix_kv_key_b3: Option<B3Hash>,
    /// Whether the prefix KV cache was hit (true) or missed (false)
    #[serde(default)]
    pub prefix_cache_hit: bool,
    /// Bytes of cached KV tensors for the prefix
    #[serde(default)]
    pub prefix_kv_bytes: u64,
    // =========================================================================
    // Model Cache Identity v2 (PRD-06: ModelCacheIdentity v2 Canonicalization)
    // =========================================================================
    /// BLAKE3 digest of ModelCacheIdentityV2 canonical bytes.
    /// Binds the receipt to the exact kernel/quant/fusion/tokenizer/tenant/worker combination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub model_cache_identity_v2_digest_b3: Option<B3Hash>,
}

/// Candidate adapter entry for router trace
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RouterCandidate {
    pub adapter_idx: u16,
    pub raw_score: f32,
    pub gate_q15: i16,
}

/// Decision hash material for audit (mirrors router DecisionHash)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RouterDecisionHash {
    pub input_hash: String,
    pub output_hash: String,
    pub combined_hash: String,
    pub tau: f32,
    pub eps: f32,
    pub k: usize,
}

/// Chained router decision entry (per token)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub policy_mask_digest: Option<B3Hash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_overrides_applied: Option<PolicyOverrideFlags>,
}

/// Router decision at a specific position (canonical schema)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub policy_mask_digest: Option<B3Hash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_overrides_applied: Option<PolicyOverrideFlags>,
}

/// Flags describing which policy overrides affected routing.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PolicyOverrideFlags {
    pub allow_list: bool,
    pub deny_list: bool,
    pub trust_state: bool,
}

/// KV cache usage statistics for receipt generation
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
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
