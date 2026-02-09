//! Response types for inference and patch proposals.
//!
//! This module contains all response-related types including:
//! - InferenceResponse: Main response structure for inference
//! - InferenceErrorDetails: Typed error information
//! - PatchProposalResponse: Response for patch proposal requests
//! - ResponseTrace: Trace information with evidence and router decisions
//! - Various supporting types for telemetry and tracing

use adapteros_api_types::{inference::FusionIntervalTrace, RunReceipt};
use adapteros_core::B3Hash;
use adapteros_policy::RefusalResponse;
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

/// Structured error details for inference failures
///
/// This enum provides typed error information for specific failure modes,
/// allowing clients to programmatically handle different error cases.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum InferenceErrorDetails {
    /// Model cache budget exceeded during eviction
    ///
    /// This error occurs when the model cache cannot free enough memory
    /// to accommodate a new model load, typically because entries are
    /// pinned (base models) or active (in-flight inference).
    #[serde(rename = "cache_budget_exceeded")]
    CacheBudgetExceeded {
        /// Memory needed in megabytes
        needed_mb: u64,
        /// Memory freed during eviction attempt in megabytes
        freed_mb: u64,
        /// Number of pinned entries that blocked eviction
        pinned_count: usize,
        /// Number of active entries that blocked eviction
        active_count: usize,
        /// Maximum cache budget in megabytes
        max_mb: u64,
        /// Optional model key identifier (for diagnostics)
        model_key: Option<String>,
    },
    /// Generic worker error (fallback for unstructured errors)
    #[serde(rename = "worker_error")]
    WorkerError {
        /// Error message
        message: String,
    },
}

/// Token usage computed by the worker tokenizer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub billed_input_tokens: u32,
    pub billed_output_tokens: u32,
}

/// Inference response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub text: Option<String>,
    pub status: String,
    pub trace: ResponseTrace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_receipt: Option<RunReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<RefusalResponse>,
    /// Patch proposal if requested
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_proposal: Option<PatchProposalResponse>,
    /// Stack ID for telemetry correlation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    /// Stack version for telemetry correlation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_version: Option<i64>,
    /// Normalized backend identifier for deterministic responses.
    ///
    /// This field contains a canonical identifier that is consistent across
    /// hardware configurations to enable deterministic replay/comparison:
    /// - `"native"` for MLX variants (mlx, mlxbridge)
    /// - `"accelerated"` for Apple hardware acceleration (coreml, metal)
    /// - `"cpu"` for CPU-only execution
    ///
    /// Use `backend_raw` for observability/telemetry when the actual backend matters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_used: Option<String>,
    /// Raw backend identifier for telemetry/observability.
    ///
    /// Contains the device-specific backend name (e.g., "mlx", "coreml", "metal")
    /// for debugging and telemetry purposes. For deterministic comparison, use
    /// the normalized `backend_used` field instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_raw: Option<String>,
    /// Backend version/build identifier (e.g., crate/FFI version)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_version: Option<String>,
    /// Whether backend fallback occurred during execution
    #[serde(default)]
    pub fallback_triggered: bool,
    /// Requested CoreML compute preference (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_compute_preference: Option<String>,
    /// CoreML compute units actually used (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_compute_units: Option<String>,
    /// Whether CoreML leveraged GPU for this inference (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_gpu_used: Option<bool>,
    /// Hash of the fused CoreML package manifest used (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_package_hash: Option<String>,
    /// Expected hash used for verification, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_expected_package_hash: Option<String>,
    /// Whether the computed hash mismatched the expected value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_hash_mismatch: Option<bool>,
    /// Backend selected after fallback (if different from requested)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_backend: Option<String>,
    /// Determinism mode applied after resolution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub determinism_mode_applied: Option<String>,
    /// Pinned adapters that were unavailable (CHAT-PIN-02)
    ///
    /// These are pinned adapter IDs that were not present in the worker's
    /// loaded adapter set (manifest.adapters). Returned for UI warning display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_pinned_adapters: Option<Vec<String>>,
    /// Routing fallback mode when pinned adapters are unavailable (PRD-6A)
    ///
    /// - `None`: All pinned adapters were available (or no pins configured)
    /// - `Some("partial")`: Some pinned adapters unavailable, using available pins + stack
    /// - `Some("stack_only")`: All pinned adapters unavailable, routing uses stack only
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_routing_fallback: Option<String>,
    /// Evidence-bound pinned degradation (counts + digest, no raw IDs).
    ///
    /// Only populated when pinned adapters are unavailable at execution time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_degradation_evidence: Option<adapteros_core::PinnedDegradationEvidence>,
    /// Placement decisions per token (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement_trace: Option<Vec<PlacementTraceEntry>>,

    // Stop Controller Fields (PRD: Hard Deterministic Stop Controller)
    /// Stop reason code explaining why generation terminated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason_code: Option<adapteros_api_types::inference::StopReasonCode>,
    /// Token index at which the stop decision was made
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason_token_index: Option<u32>,
    /// BLAKE3 digest of the StopPolicySpec used (hex encoded)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_policy_digest_b3: Option<String>,

    /// Structured error details when status indicates failure
    ///
    /// Contains typed error information for specific failure modes.
    /// This field is `None` for successful responses or unstructured errors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_details: Option<InferenceErrorDetails>,
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
