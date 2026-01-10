//! SSE event types and stream identifiers
//!
//! This module defines the core types used for reliable SSE streaming
//! with monotonic event IDs and replay support.

use serde::{Deserialize, Serialize};

/// Stream type identifiers for per-stream ID sequences
///
/// Each stream type maintains its own independent monotonic counter
/// and ring buffer for event replay.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum SseStreamType {
    /// System metrics (CPU, memory, disk, GPU)
    SystemMetrics,
    /// Telemetry events from all components
    Telemetry,
    /// Adapter lifecycle state transitions
    AdapterState,
    /// Worker status updates
    Workers,
    /// Training progress and signals
    Training,
    /// System alerts and notifications
    Alerts,
    /// Anomaly detection events
    Anomalies,
    /// Dashboard widget metrics
    Dashboard,
    /// Token-by-token inference streaming
    Inference,
    /// Model discovery events
    Discovery,
    /// Workspace activity events
    Activity,
    /// Boot progress events
    BootProgress,
    /// Dataset processing progress
    DatasetProgress,
    /// Git operations progress
    GitProgress,
    /// Inference trace receipts for deterministic proof
    TraceReceipts,
}

impl SseStreamType {
    /// Get the default buffer capacity for this stream type
    ///
    /// High-frequency streams (inference, telemetry) get larger buffers
    /// to support longer reconnection windows.
    pub fn default_capacity(&self) -> usize {
        match self {
            Self::Inference => 2000, // High frequency token streaming
            Self::Telemetry => 1500, // High volume telemetry
            Self::Training => 500,   // Less frequent training events
            Self::Workers => 500,    // Moderate frequency worker updates
            Self::Alerts => 200,     // Alerts should be rare
            Self::Anomalies => 200,  // Anomalies should be rare
            _ => 1000,               // Default for most streams
        }
    }

    /// Get the event type name for SSE events
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::SystemMetrics => "metrics",
            Self::Telemetry => "telemetry",
            Self::AdapterState => "adapters",
            Self::Workers => "workers",
            Self::Training => "training",
            Self::Alerts => "alerts",
            Self::Anomalies => "anomalies",
            Self::Dashboard => "dashboard_metrics",
            Self::Inference => "inference",
            Self::Discovery => "discovery",
            Self::Activity => "activity",
            Self::BootProgress => "boot_progress",
            Self::DatasetProgress => "dataset_progress",
            Self::GitProgress => "git_progress",
            Self::TraceReceipts => "trace_receipts",
        }
    }
}

/// Stored SSE event with monotonic ID
///
/// This structure represents an event that can be stored in the ring buffer
/// and replayed to reconnecting clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseEvent {
    /// Monotonically increasing event ID (u64)
    ///
    /// IDs are unique within a stream type and always increase.
    /// Clients use this ID in the `Last-Event-ID` header to resume.
    pub id: u64,

    /// Event type (e.g., "metrics", "telemetry", "token")
    ///
    /// Maps to the SSE `event:` field.
    pub event_type: String,

    /// JSON-serialized event data
    ///
    /// Maps to the SSE `data:` field.
    pub data: String,

    /// Timestamp of creation (milliseconds since epoch)
    pub timestamp_ms: u64,

    /// Optional retry hint in milliseconds
    ///
    /// Suggests to the client how long to wait before reconnecting.
    /// Maps to the SSE `retry:` field.
    pub retry_ms: Option<u32>,
}

impl SseEvent {
    /// Create a new SSE event
    pub fn new(id: u64, event_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            id,
            event_type: event_type.into(),
            data: data.into(),
            timestamp_ms: current_timestamp_ms(),
            retry_ms: Some(3000), // Default 3 second retry
        }
    }

    /// Set the retry hint
    pub fn with_retry(mut self, retry_ms: u32) -> Self {
        self.retry_ms = Some(retry_ms);
        self
    }

    /// Remove the retry hint
    pub fn without_retry(mut self) -> Self {
        self.retry_ms = None;
        self
    }
}

/// Get current timestamp in milliseconds since epoch
fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// =============================================================================
// SSE Error Event Types (Category 18 - Streaming Errors)
// =============================================================================

/// SSE error event sent to clients when stream issues occur
///
/// These events help clients understand and recover from streaming issues.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SseErrorEvent {
    /// Stream disconnected - client should reconnect with Last-Event-ID
    StreamDisconnected {
        /// Last event ID sent before disconnect
        last_event_id: u64,
        /// Reason for the disconnect
        reason: String,
        /// Suggested reconnect delay in milliseconds
        reconnect_hint_ms: u64,
    },

    /// Buffer overflow - some events were dropped
    BufferOverflow {
        /// Number of events that were dropped
        dropped_count: u64,
        /// Oldest available event ID after the overflow
        oldest_available_id: u64,
    },

    /// Event gap detected - client missed events that are no longer available
    EventGapDetected {
        /// Client's last known event ID
        client_last_id: u64,
        /// Server's oldest available event ID
        server_oldest_id: u64,
        /// Estimated number of events lost
        events_lost: u64,
        /// Recovery hint for the client
        recovery_hint: EventGapRecoveryHint,
    },

    /// Heartbeat to keep connection alive (not an error, but sent periodically)
    Heartbeat {
        /// Current server event ID
        current_id: u64,
        /// Server timestamp in milliseconds
        timestamp_ms: u64,
    },
}

/// Recovery hints for event gap scenarios
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventGapRecoveryHint {
    /// Client should refetch full state from REST API
    RefetchFullState,
    /// Client can continue with current ID (gap is acceptable)
    ContinueWithGap,
    /// Client should restart the stream from the beginning
    RestartStream,
    /// Gap affects specific resource - refetch that resource only
    RefetchResource {
        resource_type: String,
        resource_id: String,
    },
}

impl SseErrorEvent {
    /// Create a disconnect event
    pub fn disconnected(last_event_id: u64, reason: impl Into<String>) -> Self {
        Self::StreamDisconnected {
            last_event_id,
            reason: reason.into(),
            reconnect_hint_ms: 3000, // Default 3 second delay
        }
    }

    /// Create a buffer overflow event
    pub fn overflow(dropped_count: u64, oldest_available_id: u64) -> Self {
        Self::BufferOverflow {
            dropped_count,
            oldest_available_id,
        }
    }

    /// Create an event gap detection event
    pub fn gap_detected(
        client_last_id: u64,
        server_oldest_id: u64,
        events_lost: u64,
        recovery_hint: EventGapRecoveryHint,
    ) -> Self {
        Self::EventGapDetected {
            client_last_id,
            server_oldest_id,
            events_lost,
            recovery_hint,
        }
    }

    /// Create a heartbeat event
    pub fn heartbeat(current_id: u64) -> Self {
        Self::Heartbeat {
            current_id,
            timestamp_ms: current_timestamp_ms(),
        }
    }

    /// Convert to SseEvent for transmission
    pub fn to_sse_event(&self, id: u64) -> SseEvent {
        let data = serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string());
        SseEvent::new(id, "error", data)
    }

    /// Get the event type name
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::StreamDisconnected { .. } => "stream_disconnected",
            Self::BufferOverflow { .. } => "buffer_overflow",
            Self::EventGapDetected { .. } => "event_gap",
            Self::Heartbeat { .. } => "heartbeat",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_type_capacity() {
        assert_eq!(SseStreamType::Inference.default_capacity(), 2000);
        assert_eq!(SseStreamType::Alerts.default_capacity(), 200);
        assert_eq!(SseStreamType::SystemMetrics.default_capacity(), 1000);
        assert_eq!(SseStreamType::Workers.default_capacity(), 500);
    }

    #[test]
    fn test_stream_type_event_name() {
        assert_eq!(SseStreamType::SystemMetrics.event_name(), "metrics");
        assert_eq!(SseStreamType::Telemetry.event_name(), "telemetry");
        assert_eq!(SseStreamType::Training.event_name(), "training");
        assert_eq!(SseStreamType::Workers.event_name(), "workers");
    }

    #[test]
    fn test_sse_event_creation() {
        let event = SseEvent::new(42, "test", r#"{"key": "value"}"#);
        assert_eq!(event.id, 42);
        assert_eq!(event.event_type, "test");
        assert_eq!(event.data, r#"{"key": "value"}"#);
        assert_eq!(event.retry_ms, Some(3000));
        assert!(event.timestamp_ms > 0);
    }

    #[test]
    fn test_sse_event_builder() {
        let event = SseEvent::new(1, "test", "{}").with_retry(5000);
        assert_eq!(event.retry_ms, Some(5000));

        let event = SseEvent::new(2, "test", "{}").without_retry();
        assert_eq!(event.retry_ms, None);
    }

    #[test]
    fn test_sse_error_event_disconnected() {
        let event = SseErrorEvent::disconnected(100, "server shutdown");
        match event {
            SseErrorEvent::StreamDisconnected {
                last_event_id,
                reason,
                reconnect_hint_ms,
            } => {
                assert_eq!(last_event_id, 100);
                assert_eq!(reason, "server shutdown");
                assert_eq!(reconnect_hint_ms, 3000);
            }
            _ => panic!("Expected StreamDisconnected"),
        }
    }

    #[test]
    fn test_sse_error_event_overflow() {
        let event = SseErrorEvent::overflow(50, 150);
        match event {
            SseErrorEvent::BufferOverflow {
                dropped_count,
                oldest_available_id,
            } => {
                assert_eq!(dropped_count, 50);
                assert_eq!(oldest_available_id, 150);
            }
            _ => panic!("Expected BufferOverflow"),
        }
    }

    #[test]
    fn test_sse_error_event_gap_detected() {
        let event =
            SseErrorEvent::gap_detected(50, 100, 50, EventGapRecoveryHint::RefetchFullState);
        match event {
            SseErrorEvent::EventGapDetected {
                client_last_id,
                server_oldest_id,
                events_lost,
                recovery_hint,
            } => {
                assert_eq!(client_last_id, 50);
                assert_eq!(server_oldest_id, 100);
                assert_eq!(events_lost, 50);
                assert!(matches!(
                    recovery_hint,
                    EventGapRecoveryHint::RefetchFullState
                ));
            }
            _ => panic!("Expected EventGapDetected"),
        }
    }

    #[test]
    fn test_sse_error_event_to_sse() {
        let error_event = SseErrorEvent::heartbeat(42);
        let sse_event = error_event.to_sse_event(1);
        assert_eq!(sse_event.id, 1);
        assert_eq!(sse_event.event_type, "error");
        assert!(sse_event.data.contains("heartbeat"));
    }

    #[test]
    fn test_sse_error_event_serialization() {
        let event = SseErrorEvent::gap_detected(
            10,
            20,
            10,
            EventGapRecoveryHint::RefetchResource {
                resource_type: "adapter".to_string(),
                resource_id: "abc123".to_string(),
            },
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("event_gap"));
        assert!(json.contains("refetch_resource"));
        assert!(json.contains("adapter"));
        assert!(json.contains("abc123"));
    }
}
