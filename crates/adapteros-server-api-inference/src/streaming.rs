//! Streaming inference types for SSE responses
//!
//! This module provides Server-Sent Events (SSE) types for streaming inference.
//! Compatible with OpenAI's streaming API format for chat completions.
//!
//! # SSE Event Format
//!
//! ```text
//! event: aos.run_envelope
//! data: {"run_id":"...","schema_version":"..."}
//!
//! event: stream_started
//! data: {"type":"stream_started","stream_id":"...","idempotency_key":"..."}
//!
//! data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{"role":"assistant"}}]}
//! data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{"content":"Hello"}}]}
//! data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"finish_reason":"stop"}]}
//! data: [DONE]
//!
//! event: stream_finished
//! data: {"type":"stream_finished","total_tokens":42,"duration_ms":1234}
//! ```
//!
//! # Reconnection
//!
//! Reconnection replay is not supported; clients should retry the full request.

use adapteros_api_types::inference::{Citation, ContextRequest, StopPolicySpec, StopReasonCode};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use adapteros_types::coreml::CoreMLMode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Streaming inference request
///
/// Accepts the same fields as the standard `/v1/infer` endpoint but returns
/// a stream of Server-Sent Events (SSE) with tokens as they are generated.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StreamingInferRequest {
    /// The input prompt for inference
    pub prompt: String,

    /// Model identifier (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// CoreML mode for backend selection (coreml_strict|coreml_preferred|backend_auto)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_mode: Option<CoreMLMode>,

    /// Per-request override for router determinism (deterministic/adaptive)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,

    /// Adapter stack identifier to use for inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,

    /// Optional domain hint to bias adapter/package selection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,

    /// Maximum number of tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Sampling temperature (0.0 - 2.0)
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Top-p nucleus sampling parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    /// Top-k sampling parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,

    /// Stop sequences to terminate generation
    #[serde(default)]
    pub stop: Vec<String>,

    /// Adapter stack to use for inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_stack: Option<Vec<String>>,

    /// Specific adapters to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<String>>,

    /// Random seed for reproducibility.
    ///
    /// Required when the effective determinism mode is strict and tenant policy
    /// sets `determinism.require_seed=true`. Missing or invalid seeds are rejected
    /// as determinism violations (no partial result).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,

    /// Per-adapter strength overrides (session scoped)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_strength_overrides: Option<HashMap<String, f32>>,

    /// Require evidence in response
    #[serde(default)]
    pub require_evidence: bool,

    /// Enable reasoning-aware routing and mid-flight swaps
    #[serde(default)]
    pub reasoning_mode: bool,

    /// Collection ID for scoping RAG retrieval to specific document collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,

    /// Session ID for linking inference to chat sessions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Effective adapter IDs (control-plane computed; ignored from clients)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_adapter_ids: Option<Vec<String>>,

    /// Stop policy specification (PRD: Hard Deterministic Stop Controller)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_policy: Option<StopPolicySpec>,

    /// Context request for UI context injection (PRD-002 Phase 2)
    /// When flags are true, the server fetches and injects the corresponding
    /// context data into the prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextRequest>,
}

fn default_max_tokens() -> usize {
    512
}

fn default_temperature() -> f32 {
    0.7
}

/// OpenAI-compatible streaming chunk response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StreamingChunk {
    /// Unique identifier for the completion
    pub id: String,

    /// Object type (always "chat.completion.chunk")
    pub object: String,

    /// Unix timestamp of creation
    pub created: u64,

    /// Model used for generation
    pub model: String,

    /// System fingerprint for determinism tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,

    /// Array of choices
    pub choices: Vec<StreamingChoice>,
}

/// Individual choice in a streaming response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StreamingChoice {
    /// Index of this choice
    pub index: usize,

    /// Delta containing new content
    pub delta: Delta,

    /// Finish reason (null until complete)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,

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

/// Delta containing new content
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Delta {
    /// Role (only in first chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,

    /// Content delta (new tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Internal streaming event types
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Stream lifecycle started - sent as first event with stream metadata
    StreamStarted {
        stream_id: String,
        idempotency_key: Option<String>,
    },
    /// First chunk with role
    Start,
    /// Token generated
    Token(String),
    /// Generation complete
    Done { finish_reason: String },
    /// Stream lifecycle finished - sent as final event with summary
    StreamFinished {
        stream_id: String,
        total_tokens: usize,
        duration_ms: u64,
    },
    /// Heartbeat to keep SSE connection alive
    Heartbeat,
    /// Error occurred
    Error {
        code: String,
        message: String,
        retryable: bool,
    },
}

/// Error payload for stream errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamErrorPayload {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Whether the error is retryable
    pub retryable: bool,
    /// Correlation ID for debugging
    pub correlation_id: String,
}

/// Adapter state information for visualization
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterStateInfo {
    /// Adapter identifier
    pub adapter_id: String,
    /// Usage rate (uses per minute)
    pub uses_per_minute: u32,
    /// Currently active (in use for this inference)
    pub is_active: bool,
}

/// Inference event types for progress streaming
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event")]
pub enum InferenceEvent {
    /// Model is being loaded
    Loading {
        phase: LoadPhase,
        progress: u8,
        eta_seconds: Option<u64>,
    },
    /// Model is ready
    Ready { warmup_latency_ms: u64 },
    /// Inference token
    Token { text: String, token_id: Option<u32> },
    /// Inference complete
    Done {
        total_tokens: usize,
        latency_ms: u64,
        /// Pinned adapters that were unavailable
        #[serde(skip_serializing_if = "Option::is_none")]
        unavailable_pinned_adapters: Option<Vec<String>>,
        /// Routing fallback mode
        #[serde(skip_serializing_if = "Option::is_none")]
        pinned_routing_fallback: Option<String>,
        /// Citations attached to the response
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<Vec<Citation>>,
        /// Stop reason code
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_reason_code: Option<StopReasonCode>,
        /// Token index at which the stop decision was made
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_reason_token_index: Option<u32>,
        /// BLAKE3 digest of the StopPolicySpec used (hex encoded)
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_policy_digest_b3: Option<String>,
        /// Pending RAG evidence IDs that need to be bound to a message_id
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pending_evidence_ids: Vec<String>,
    },
    /// Error occurred
    Error { message: String, recoverable: bool },
    /// Adapter state update for visualization
    AdapterStateUpdate { adapters: Vec<AdapterStateInfo> },
}

/// Load phases for progress tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoadPhase {
    Downloading,
    LoadingWeights,
    Warmup,
}

/// Streaming inference configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Enable streaming response
    pub stream: bool,
    /// Chunk size (tokens per chunk)
    pub chunk_size: Option<u32>,
    /// Include token probabilities
    pub include_logprobs: bool,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            stream: true,
            chunk_size: None,
            include_logprobs: false,
        }
    }
}

/// Streaming error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamError {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
}

/// Get current Unix timestamp
pub fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_request_defaults() {
        let json = r#"{"prompt": "Hello"}"#;
        let req: StreamingInferRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt, "Hello");
        assert_eq!(req.max_tokens, 512);
        assert!((req.temperature - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_streaming_chunk_serialization() {
        let chunk = StreamingChunk {
            id: "chatcmpl-test".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1234567890,
            model: "test-model".to_string(),
            system_fingerprint: None,
            choices: vec![StreamingChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: Some("Hello".to_string()),
                },
                finish_reason: None,
                stop_reason_code: None,
                stop_reason_token_index: None,
                stop_policy_digest_b3: None,
            }],
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("chat.completion.chunk"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_done_chunk_format() {
        let chunk = StreamingChunk {
            id: "chatcmpl-test".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1234567890,
            model: "test-model".to_string(),
            system_fingerprint: None,
            choices: vec![StreamingChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: None,
                },
                finish_reason: Some("stop".to_string()),
                stop_reason_code: None,
                stop_reason_token_index: None,
                stop_policy_digest_b3: None,
            }],
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("stop"));
    }

    #[test]
    fn test_inference_event_serialization() {
        let event = InferenceEvent::Loading {
            phase: LoadPhase::LoadingWeights,
            progress: 50,
            eta_seconds: Some(30),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Loading"));
        assert!(json.contains("LoadingWeights"));
        assert!(json.contains("50"));
    }

    #[test]
    fn test_inference_event_done() {
        let event = InferenceEvent::Done {
            total_tokens: 100,
            latency_ms: 5000,
            unavailable_pinned_adapters: None,
            pinned_routing_fallback: None,
            citations: None,
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
            pending_evidence_ids: Vec::new(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Done"));
        assert!(json.contains("100"));
        assert!(json.contains("5000"));
    }
}
