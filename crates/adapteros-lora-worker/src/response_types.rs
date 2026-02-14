//! Response types for inference and patch proposals.
//!
//! This module contains all response-related types including:
//! - PatchProposalResponse: Response for patch proposal requests
//! - ResponseTrace: Trace information with evidence and router decisions
//! - Various supporting types for telemetry and tracing

use adapteros_api_types::inference::FusionIntervalTrace;
use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};

/// Placement decision trace entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementTraceEntry {
    pub step: usize,
    pub lane: String,
    pub score: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<f32>,
    pub utilization: f32,
}

/// Cached CoreML verification snapshot for observability.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CoremlVerificationSnapshot {
    /// Verification mode (off/warn/strict) in effect when the check ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Expected fused CoreML package hash (hex) if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    /// Actual fused CoreML package hash (hex) if computed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    /// Source of the expected hash (db/manifest/env/metadata/none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Terminal verification status label (match/mismatch/missing_expected/missing_actual/skipped).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Convenience flag for mismatch detection.
    #[serde(default)]
    pub mismatch: bool,
}

/// Token usage computed by the worker tokenizer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub billed_input_tokens: u32,
    pub billed_output_tokens: u32,
}

/// Token payload for streaming inference.
#[derive(Debug, Clone)]
pub struct StreamToken {
    pub text: String,
    pub token_id: Option<u32>,
}

/// Streaming inference events emitted by the worker.
#[derive(Debug, Clone)]
pub enum WorkerStreamEvent {
    Token(StreamToken),
    Complete(Box<crate::InferenceResponse>),
    Error(String),
    /// Inference paused for human review
    Paused {
        /// Unique pause ID for resume correlation
        pause_id: String,
        /// Inference request ID
        inference_id: String,
        /// Why the pause was triggered
        trigger_kind: String,
        /// Context for the reviewer
        context: Option<String>,
        /// Generated text so far
        text_so_far: Option<String>,
        /// Token count at pause point
        token_count: usize,
    },
}

/// Patch proposal response with patches and citations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchProposalResponse {
    pub proposal_id: String,
    pub rationale: String,
    pub patches: Vec<FilePatchResponse>,
    pub citations: Vec<CitationResponse>,
    pub confidence: f32,
}

/// File patch in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePatchResponse {
    pub file_path: String,
    pub hunks: Vec<PatchHunkResponse>,
}

/// Patch hunk in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchHunkResponse {
    pub start_line: usize,
    pub end_line: usize,
    pub old_content: String,
    pub new_content: String,
}

/// Citation in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationResponse {
    pub source_type: String,
    pub reference: String,
    pub relevance: f32,
}

/// Response trace with evidence and router decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTrace {
    pub cpid: String,
    pub plan_id: String,
    pub evidence: Vec<EvidenceRef>,
    pub router_summary: RouterSummary,
    pub token_count: usize,
    /// Detailed router decisions per step (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router_decisions: Option<Vec<adapteros_api_types::inference::RouterDecision>>,
    /// Cryptographically chained router decisions (per-token)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router_decision_chain:
        Option<Vec<adapteros_api_types::inference::RouterDecisionChainEntry>>,
    /// Fusion interval boundaries and fused tensor hashes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fusion_intervals: Option<Vec<FusionIntervalTrace>>,
    /// Model type for this trace
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_type: Option<adapteros_api_types::inference::RouterModelType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub doc_id: String,
    pub rev: String,
    pub span_hash: B3Hash,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterSummary {
    pub adapters_used: Vec<String>,
    pub avg_activations: Vec<f32>,
}

/// CoreML runtime telemetry captured for replay/logging
#[derive(Debug, Clone, Default)]
pub struct CoremlRuntimeTelemetry {
    pub compute_preference: Option<String>,
    pub compute_units: Option<String>,
    pub gpu_available: Option<bool>,
    pub ane_available: Option<bool>,
    pub gpu_used: Option<bool>,
    pub ane_used: Option<bool>,
    pub production_mode: Option<bool>,
}

/// Inference event telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceEvent {
    pub duration_ms: u64,
    pub success: bool,
    pub timeout_occurred: bool,
    pub circuit_breaker_open: bool,
    pub memory_usage: u64,
    /// Time spent waiting in queue before inference starts (microseconds)
    #[serde(default)]
    pub queue_time_us: u64,
    /// Time spent in actual token generation (microseconds)
    #[serde(default)]
    pub generation_time_us: u64,
}
