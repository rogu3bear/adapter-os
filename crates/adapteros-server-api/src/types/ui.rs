//! UI-only API types.

use crate::types::{TimingBreakdown, TokenDecision};
use serde::{Deserialize, Serialize};

/// UI-only inference trace detail response with extended receipt fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiInferenceTraceDetailResponse {
    pub trace_id: String,
    pub request_id: Option<String>,
    pub created_at: String,
    pub latency_ms: u64,
    pub adapters_used: Vec<String>,
    pub stack_id: Option<String>,
    pub model_id: Option<String>,
    pub policy_id: Option<String>,
    #[serde(default)]
    pub token_decisions: Vec<TokenDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_decisions_next_cursor: Option<u32>,
    #[serde(default)]
    pub token_decisions_has_more: bool,
    pub timing_breakdown: TimingBreakdown,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<UiTraceReceiptSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_id: Option<String>,
}

/// UI-only receipt summary with extended provenance fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTraceReceiptSummary {
    pub receipt_digest: String,
    pub run_head_hash: String,
    pub output_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_digest_b3: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_lineage_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_attestation_b3: Option<String>,
    /// BLAKE3 hashes of training datasets for adapters used in this inference.
    /// Enables verification of which training data influenced the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_training_digests: Option<Vec<String>>,
    pub logical_prompt_tokens: u32,
    pub logical_output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_token_index: Option<u32>,
    /// Receipt verification status:
    /// - `Some(true)`: server recomputation matched the stored receipt digest
    /// - `Some(false)`: mismatch detected
    /// - `None`: not yet recomputed/verified
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ane_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix_cache_hit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix_kv_bytes: Option<u64>,
}
