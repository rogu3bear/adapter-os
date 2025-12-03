//! Inference types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

/// Inference request
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct InferRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
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
    /// Adapter stack to use for inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_stack: Option<Vec<String>>,
    /// Specific adapters to use (alternative to adapter_stack)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<String>>,
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
    /// Pinned adapters that were unavailable for this inference (CHAT-PIN-02)
    ///
    /// These are adapters that were in the session's pinned set but were not
    /// available in the candidate adapter set. Returned for UI warning display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_pinned_adapters: Option<Vec<String>>,
    /// Routing fallback mode when pinned adapters are unavailable (PRD-6A)
    ///
    /// - `None`: All pinned adapters were available (or no pins configured)
    /// - `Some("partial")`: Some pinned adapters unavailable, using available pins + stack
    /// - `Some("stack_only")`: All pinned adapters unavailable, routing uses stack only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_routing_fallback: Option<String>,
}

/// Inference trace for observability
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct InferenceTrace {
    pub adapters_used: Vec<String>,
    pub router_decisions: Vec<RouterDecision>,
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
