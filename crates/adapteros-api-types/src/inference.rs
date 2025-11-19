//! Inference types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

/// Inference request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
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
#[serde(rename_all = "snake_case")]
pub struct InferResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub text: String,
    pub tokens: Vec<u32>,
    pub finish_reason: String,
    pub trace: InferenceTrace,
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
