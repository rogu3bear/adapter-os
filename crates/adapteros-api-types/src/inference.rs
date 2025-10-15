//! Inference types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Inference request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InferRequest {
    pub prompt: String,
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
    pub require_evidence: Option<bool>,
}

/// Inference response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InferResponse {
    pub text: String,
    pub tokens: Vec<u32>,
    pub finish_reason: String,
    pub trace: InferenceTrace,
}

/// Inference trace for observability
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InferenceTrace {
    pub adapters_used: Vec<String>,
    pub router_decisions: Vec<RouterDecision>,
    pub latency_ms: u64,
}

/// Router decision at a specific position
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RouterDecision {
    pub position: usize,
    pub adapter_ids: Vec<u16>,
    pub gates: Vec<u16>,
}
