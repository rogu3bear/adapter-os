//! Inference types

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
    /// Adapter stack identifier to use for inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
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
    pub latency_ms: u64,
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
}
