//! Typed SSE event payloads for inference streaming.
//!
//! These structs match the JSON payloads emitted by the server's `stream_started`,
//! `stream_finished`, and `error` SSE event types.  Using typed deserialization
//! replaces ~120 lines of manual `serde_json::Value` field extraction on the
//! client side.

use serde::{Deserialize, Serialize};

/// Payload for the `stream_started` SSE event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamStartedEvent {
    /// Unique stream identifier assigned by the server.
    #[serde(default)]
    pub stream_id: String,
    /// Correlation request ID.
    #[serde(default)]
    pub request_id: String,
    /// Optional idempotency key echoed back from the request.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Server-side timestamp in milliseconds since epoch.
    #[serde(default)]
    pub timestamp_ms: Option<u64>,
}

/// Payload for the `stream_finished` SSE event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamFinishedEvent {
    /// Stream identifier matching the one from `stream_started`.
    #[serde(default)]
    pub stream_id: String,
    /// Correlation request ID.
    #[serde(default)]
    pub request_id: String,
    /// Total tokens generated in this stream.
    #[serde(default)]
    pub total_tokens: usize,
    /// Wall-clock duration of the stream in milliseconds.
    #[serde(default)]
    pub duration_ms: u64,
    /// Reason the stream finished (e.g. "stop", "length").
    #[serde(default)]
    pub finish_reason: Option<String>,
    /// Server-side timestamp in milliseconds since epoch.
    #[serde(default)]
    pub timestamp_ms: Option<u64>,
}

/// Payload for the `error` SSE event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamErrorEvent {
    /// Machine-readable error code.
    #[serde(default)]
    pub code: String,
    /// Human-readable error message.
    #[serde(default)]
    pub message: String,
    /// Whether the client should retry this request.
    #[serde(default)]
    pub retryable: bool,
    /// Correlation ID for server-side log lookup.
    #[serde(default)]
    pub correlation_id: String,
}
