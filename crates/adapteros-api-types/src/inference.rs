//! Inference types

use adapteros_core::{backend::BackendKind, FusionInterval};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use adapteros_types::coreml::CoreMLMode;
use adapteros_types::routing::B3Hash;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

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
    pub fused_weight_hash: B3Hash,
}

/// Verifiable run receipt (hash chain over per-token decisions)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RunReceipt {
    pub trace_id: String,
    pub run_head_hash: B3Hash,
    pub output_digest: B3Hash,
    pub receipt_digest: B3Hash,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attestation: Option<String>,
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
