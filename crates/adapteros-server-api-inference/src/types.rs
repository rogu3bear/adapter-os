//! Core inference types
//!
//! This module defines the request and response types for standard inference endpoints.

use adapteros_api_types::inference::{Citation, InferenceTrace, StopPolicySpec, StopReasonCode};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use adapteros_types::coreml::CoreMLMode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Maximum prompt size for replay text (used in validation)
pub const MAX_REPLAY_TEXT_SIZE: usize = 128 * 1024; // 128KB

/// Inference request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InferRequest {
    /// The input prompt for inference
    pub prompt: String,

    /// Optional tenant ID for multi-tenant isolation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,

    /// Model identifier (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// CoreML mode for backend selection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_mode: Option<CoreMLMode>,

    /// Per-request override for router determinism
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,

    /// Adapter stack identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,

    /// Optional domain hint for adapter selection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,

    /// Maximum tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Sampling temperature (0.0 - 2.0)
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Top-p nucleus sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    /// Top-k sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,

    /// Stop sequences
    #[serde(default)]
    pub stop: Vec<String>,

    /// Adapter stack for inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_stack: Option<Vec<String>>,

    /// Specific adapters to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<String>>,

    /// Random seed for reproducibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,

    /// Per-adapter strength overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_strength_overrides: Option<HashMap<String, f32>>,

    /// Require evidence in response
    #[serde(default)]
    pub require_evidence: bool,

    /// Enable reasoning mode
    #[serde(default)]
    pub reasoning_mode: bool,

    /// Collection ID for RAG retrieval
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,

    /// Session ID for chat context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Stop policy specification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_policy: Option<StopPolicySpec>,
}

fn default_max_tokens() -> usize {
    512
}

fn default_temperature() -> f32 {
    0.7
}

/// Inference response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InferResponse {
    /// Schema version for API compatibility
    pub schema_version: String,

    /// Unique request identifier
    pub id: String,

    /// Generated text
    pub text: String,

    /// Token IDs if available
    #[serde(default)]
    pub tokens: Vec<u32>,

    /// Number of tokens generated
    pub tokens_generated: u32,

    /// Reason for completion
    pub finish_reason: String,

    /// Latency in milliseconds
    pub latency_ms: u64,

    /// Adapters used for inference
    #[serde(default)]
    pub adapters_used: Vec<String>,

    /// Run receipt for verification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_receipt: Option<String>,

    /// Deterministic receipt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deterministic_receipt: Option<String>,

    /// Run envelope with execution context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_envelope: Option<adapteros_api_types::RunEnvelope>,

    /// Citations from source documents
    #[serde(default)]
    pub citations: Vec<Citation>,

    /// Inference trace with router decisions
    pub trace: InferenceTrace,

    /// Model used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Prompt token count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,

    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Pinned adapters that were unavailable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_pinned_adapters: Option<Vec<String>>,

    /// Routing fallback mode used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_routing_fallback: Option<String>,

    /// Backend used for inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_used: Option<String>,

    /// CoreML compute preference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_compute_preference: Option<String>,

    /// CoreML compute units used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_compute_units: Option<String>,

    /// Whether CoreML GPU was used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_gpu_used: Option<bool>,

    /// Fallback backend if primary failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_backend: Option<String>,

    /// Whether fallback was triggered
    #[serde(default)]
    pub fallback_triggered: bool,

    /// Determinism mode applied
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode_applied: Option<String>,

    /// Replay guarantee level
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_guarantee: Option<String>,

    /// Stop reason code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_code: Option<StopReasonCode>,

    /// Token index where stop was triggered
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_token_index: Option<u32>,

    /// BLAKE3 digest of stop policy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_policy_digest_b3: Option<String>,
}
