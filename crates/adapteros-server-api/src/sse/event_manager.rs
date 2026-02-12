//! SSE Event Manager for reliable streaming with replay support
//!
//! Provides centralized management of SSE events across all stream types,
//! including monotonic ID generation, event storage, and client reconnection
//! replay support.

use super::ring_buffer::{BufferStats, SseRingBuffer};
use super::types::{SseEvent, SseStreamType};
use axum::http::HeaderMap;
use axum::response::sse::Event;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Duration;

/// Default buffer capacity for streams without specific configuration
pub const DEFAULT_BUFFER_CAPACITY: usize = 1000;

/// Default retry hint in milliseconds
pub const DEFAULT_RETRY_MS: u32 = 3000;

/// SSE Event Manager for all stream types
///
/// This is the central component for reliable SSE streaming. It manages:
///
/// - **Per-stream ring buffers** for event storage and replay
/// - **Monotonic ID generation** within each stream type
/// - **Last-Event-ID parsing** from HTTP headers
/// - **Event replay** for reconnecting clients
///
/// # Thread Safety
///
/// The manager is fully thread-safe and can be shared across handlers
/// using `Arc<SseEventManager>`.
///
/// # Example
///
/// ```ignore
/// let manager = Arc::new(SseEventManager::new());
///
/// // Create an event with monotonic ID
/// let event = manager
///     .create_event(SseStreamType::SystemMetrics, "metrics", json_data)
///     .await;
///
/// // Convert to Axum SSE event
/// let sse_event = SseEventManager::to_axum_event(&event);
///
/// // On reconnect, replay missed events
/// if let Some(last_id) = SseEventManager::parse_last_event_id(&headers) {
///     let missed = manager.get_replay_events(SseStreamType::SystemMetrics, last_id).await;
/// }
/// ```
pub struct SseEventManager {
    /// Per-stream-type ring buffers
    buffers: DashMap<SseStreamType, Arc<SseRingBuffer>>,

    /// Default buffer capacity for new streams
    default_capacity: usize,

    /// Default retry hint in milliseconds
    default_retry_ms: u32,
}

impl SseEventManager {
    /// Create a new event manager with default settings
    pub fn new() -> Self {
        Self::with_config(DEFAULT_BUFFER_CAPACITY, DEFAULT_RETRY_MS)
    }

    /// Create an event manager with custom buffer capacity
    pub fn with_capacity(default_capacity: usize) -> Self {
        Self::with_config(default_capacity, DEFAULT_RETRY_MS)
    }

    /// Create an event manager with full configuration
    pub fn with_config(default_capacity: usize, default_retry_ms: u32) -> Self {
        Self {
            buffers: DashMap::new(),
            default_capacity,
            default_retry_ms,
        }
    }

    /// Get or create buffer for a stream type
    ///
    /// Uses the minimum of stream type's preferred capacity and manager's
    /// default capacity. This allows tests to use small buffers while
    /// production uses appropriate sizes per stream type.
    fn get_buffer(&self, stream_type: SseStreamType) -> Arc<SseRingBuffer> {
        self.buffers
            .entry(stream_type)
            .or_insert_with(|| {
                // Use minimum of stream-specific and manager default capacity
                // This allows tests to limit buffer size while production
                // uses appropriate sizes per stream type
                let capacity = stream_type.default_capacity().min(self.default_capacity);
                Arc::new(SseRingBuffer::new(capacity))
            })
            .clone()
    }

    /// Create and store a new SSE event
    ///
    /// Generates a monotonic ID, creates the event, stores it in the
    /// appropriate ring buffer, and returns it for immediate streaming.
    ///
    /// # Arguments
    ///
    /// * `stream_type` - The type of stream this event belongs to
    /// * `event_type` - The SSE event type (e.g., "metrics", "token")
    /// * `data` - The JSON-serialized event payload
    ///
    /// # Returns
    ///
    /// The created `SseEvent` with a monotonic ID assigned.
    pub async fn create_event(
        &self,
        stream_type: SseStreamType,
        event_type: &str,
        data: String,
    ) -> SseEvent {
        let buffer = self.get_buffer(stream_type);
        let id = buffer.next_id();

        let event = SseEvent::new(id, event_type, data).with_retry(self.default_retry_ms);

        buffer.push(event.clone()).await;
        event
    }

    /// Create an event using the stream type's default event name
    ///
    /// Convenience method that uses `stream_type.event_name()` as the event type.
    pub async fn create_default_event(&self, stream_type: SseStreamType, data: String) -> SseEvent {
        self.create_event(stream_type, stream_type.event_name(), data)
            .await
    }

    /// Create an error event
    ///
    /// Convenience method for creating error events with consistent formatting.
    pub async fn create_error_event(
        &self,
        stream_type: SseStreamType,
        error_message: &str,
    ) -> SseEvent {
        let data = serde_json::json!({ "error": error_message }).to_string();
        self.create_event(stream_type, "error", data).await
    }

    /// Parse Last-Event-ID from HTTP headers
    ///
    /// Checks for both `Last-Event-ID` and `last-event-id` headers
    /// (HTTP headers are case-insensitive).
    ///
    /// # Arguments
    ///
    /// * `headers` - The HTTP request headers
    ///
    /// # Returns
    ///
    /// The parsed event ID if present and valid, `None` otherwise.
    pub fn parse_last_event_id(headers: &HeaderMap) -> Option<u64> {
        // Try standard header name first
        headers
            .get("Last-Event-ID")
            .or_else(|| headers.get("last-event-id"))
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
    }

    /// Get events to replay after a given ID
    ///
    /// Returns all events with IDs greater than `last_event_id`,
    /// in chronological order.
    ///
    /// # Arguments
    ///
    /// * `stream_type` - The stream type to query
    /// * `last_event_id` - The last event ID the client received
    ///
    /// # Returns
    ///
    /// A vector of events to replay. May be empty if no events
    /// exist after the given ID.
    pub async fn get_replay_events(
        &self,
        stream_type: SseStreamType,
        last_event_id: u64,
    ) -> Vec<SseEvent> {
        let buffer = self.get_buffer(stream_type);
        buffer.replay_from(last_event_id).await
    }

    /// Check if there's a gap in the event history
    ///
    /// Returns `true` if the client has missed events that are no longer
    /// available (they were dropped due to buffer overflow).
    ///
    /// # Arguments
    ///
    /// * `stream_type` - The stream type to check
    /// * `last_event_id` - The client's last received event ID
    pub fn has_gap(&self, stream_type: SseStreamType, last_event_id: u64) -> bool {
        if let Some(buffer) = self.buffers.get(&stream_type) {
            buffer.has_gap(last_event_id)
        } else {
            false
        }
    }

    /// Get buffer statistics for a stream type
    ///
    /// Returns `None` if no buffer exists for the stream type.
    pub fn get_stats(&self, stream_type: SseStreamType) -> Option<BufferStats> {
        self.buffers.get(&stream_type).map(|b| b.stats())
    }

    /// Get statistics for all active stream types
    pub fn get_all_stats(&self) -> Vec<(SseStreamType, BufferStats)> {
        self.buffers
            .iter()
            .map(|entry| (*entry.key(), entry.value().stats()))
            .collect()
    }

    /// Convert an SseEvent to an Axum SSE Event
    ///
    /// This method properly sets the `id`, `event`, `data`, and optionally
    /// `retry` fields on the Axum SSE event.
    pub fn to_axum_event(event: &SseEvent) -> Event {
        let mut sse_event = Event::default()
            .id(event.id.to_string())
            .event(&event.event_type)
            .data(&event.data);

        if let Some(retry_ms) = event.retry_ms {
            sse_event = sse_event.retry(Duration::from_millis(retry_ms as u64));
        }

        sse_event
    }

    /// Create a gap warning event
    ///
    /// Used to notify clients that some events were missed due to
    /// buffer overflow during disconnection.
    pub async fn create_gap_warning(
        &self,
        stream_type: SseStreamType,
        last_client_id: u64,
    ) -> SseEvent {
        let stats = self.get_stats(stream_type);
        let data = serde_json::json!({
            "warning": "gap_detected",
            "last_client_id": last_client_id,
            "oldest_available_id": stats.map(|s| s.lowest_id).unwrap_or(0),
            "dropped_count": stats.map(|s| s.dropped_count).unwrap_or(0),
        })
        .to_string();

        self.create_event(stream_type, "warning", data).await
    }

    /// Clear all events for a stream type
    ///
    /// Useful for testing or when stream state needs to be reset.
    pub async fn clear(&self, stream_type: SseStreamType) {
        if let Some(buffer) = self.buffers.get(&stream_type) {
            buffer.clear().await;
        }
    }

    /// Clear all events across all stream types
    pub async fn clear_all(&self) {
        for entry in self.buffers.iter() {
            entry.value().clear().await;
        }
    }

    /// Emit a typed lifecycle event on the given stream
    ///
    /// Serializes `payload` to JSON and stores it as a new event with the
    /// stream type's default event name. If serialization fails, the event
    /// is silently dropped (callers should ensure payloads are always
    /// serializable).
    pub async fn emit_lifecycle<T: serde::Serialize>(&self, stream: SseStreamType, payload: &T) {
        let data = match serde_json::to_string(payload) {
            Ok(json) => json,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to serialize lifecycle event payload");
                return;
            }
        };
        self.create_event(stream, stream.event_name(), data).await;
    }
}

impl Default for SseEventManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of replay analysis
#[derive(Debug)]
pub struct ReplayResult {
    /// Events to replay to the client
    pub events: Vec<SseEvent>,
    /// Whether there's a gap (some events were lost)
    pub has_gap: bool,
    /// Number of events that were dropped
    pub dropped_count: u64,
}

impl SseEventManager {
    /// Analyze and get replay events with gap information
    ///
    /// This is a convenience method that combines replay and gap detection.
    pub async fn get_replay_with_analysis(
        &self,
        stream_type: SseStreamType,
        last_event_id: u64,
    ) -> ReplayResult {
        let buffer = self.get_buffer(stream_type);
        let events = buffer.replay_from(last_event_id).await;
        let has_gap = buffer.has_gap(last_event_id);
        let stats = buffer.stats();

        ReplayResult {
            events,
            has_gap,
            dropped_count: stats.dropped_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_monotonic_ids_per_stream() {
        let manager = SseEventManager::new();

        let e1 = manager
            .create_event(SseStreamType::SystemMetrics, "test", "{}".to_string())
            .await;
        let e2 = manager
            .create_event(SseStreamType::SystemMetrics, "test", "{}".to_string())
            .await;
        let e3 = manager
            .create_event(SseStreamType::SystemMetrics, "test", "{}".to_string())
            .await;

        assert_eq!(e1.id, 0);
        assert_eq!(e2.id, 1);
        assert_eq!(e3.id, 2);
        assert!(e2.id > e1.id);
        assert!(e3.id > e2.id);
    }

    #[tokio::test]
    async fn test_per_stream_type_isolation() {
        let manager = SseEventManager::new();

        // Events in different streams should have independent IDs
        let e1 = manager
            .create_event(SseStreamType::Alerts, "test", "{}".to_string())
            .await;
        let e2 = manager
            .create_event(SseStreamType::Training, "test", "{}".to_string())
            .await;

        // Both should start from 0
        assert_eq!(e1.id, 0);
        assert_eq!(e2.id, 0);
    }

    #[tokio::test]
    async fn test_replay_from_id() {
        let manager = SseEventManager::new();

        // Create 10 events
        for i in 0..10 {
            manager
                .create_event(
                    SseStreamType::Telemetry,
                    "test",
                    format!(r#"{{"seq": {}}}"#, i),
                )
                .await;
        }

        // Replay from ID 5 should return events 6-9 (4 events)
        let replay = manager.get_replay_events(SseStreamType::Telemetry, 5).await;
        assert_eq!(replay.len(), 4);
        assert_eq!(replay[0].id, 6);
        assert_eq!(replay[3].id, 9);
    }

    #[test]
    fn test_parse_last_event_id() {
        let mut headers = HeaderMap::new();
        headers.insert("Last-Event-ID", "42".parse().unwrap());
        assert_eq!(SseEventManager::parse_last_event_id(&headers), Some(42));

        headers.clear();
        headers.insert("last-event-id", "100".parse().unwrap());
        assert_eq!(SseEventManager::parse_last_event_id(&headers), Some(100));

        headers.clear();
        assert_eq!(SseEventManager::parse_last_event_id(&headers), None);

        // Invalid value
        headers.insert("Last-Event-ID", "not-a-number".parse().unwrap());
        assert_eq!(SseEventManager::parse_last_event_id(&headers), None);
    }

    #[tokio::test]
    async fn test_to_axum_event() {
        let event = SseEvent::new(42, "metrics", r#"{"cpu": 50}"#).with_retry(5000);

        let axum_event = SseEventManager::to_axum_event(&event);

        // Can't easily inspect Axum Event internals, but we can verify it doesn't panic
        // and produces valid output
        let _ = format!("{:?}", axum_event);
    }

    #[tokio::test]
    async fn test_gap_detection() {
        let manager = SseEventManager::with_capacity(5);

        // Create 10 events (buffer holds 5)
        for _ in 0..10 {
            manager
                .create_event(SseStreamType::Alerts, "test", "{}".to_string())
                .await;
        }

        // Client with last_id=2 has a gap
        assert!(manager.has_gap(SseStreamType::Alerts, 2));

        // Client with last_id=7 has no gap
        assert!(!manager.has_gap(SseStreamType::Alerts, 7));
    }

    #[tokio::test]
    async fn test_replay_with_analysis() {
        let manager = SseEventManager::with_capacity(5);

        // Create 10 events
        for _ in 0..10 {
            manager
                .create_event(SseStreamType::Training, "test", "{}".to_string())
                .await;
        }

        let result = manager
            .get_replay_with_analysis(SseStreamType::Training, 2)
            .await;

        assert!(result.has_gap);
        assert_eq!(result.dropped_count, 5);
        // Events 5-9 should be available (5 events)
        assert_eq!(result.events.len(), 5);
    }

    #[tokio::test]
    async fn test_create_default_event() {
        let manager = SseEventManager::new();

        let event = manager
            .create_default_event(SseStreamType::SystemMetrics, r#"{"cpu": 50}"#.to_string())
            .await;

        assert_eq!(event.event_type, "metrics");
    }

    #[tokio::test]
    async fn test_create_error_event() {
        let manager = SseEventManager::new();

        let event = manager
            .create_error_event(SseStreamType::Inference, "Connection failed")
            .await;

        assert_eq!(event.event_type, "error");
        assert!(event.data.contains("Connection failed"));
    }

    #[tokio::test]
    async fn test_get_all_stats() {
        let manager = SseEventManager::new();

        // Create events in multiple streams
        manager
            .create_event(SseStreamType::Alerts, "test", "{}".to_string())
            .await;
        manager
            .create_event(SseStreamType::Training, "test", "{}".to_string())
            .await;
        manager
            .create_event(SseStreamType::Training, "test", "{}".to_string())
            .await;

        let all_stats = manager.get_all_stats();
        assert_eq!(all_stats.len(), 2);
    }

    #[tokio::test]
    async fn test_clear() {
        let manager = SseEventManager::new();

        manager
            .create_event(SseStreamType::Alerts, "test", "{}".to_string())
            .await;
        manager
            .create_event(SseStreamType::Alerts, "test", "{}".to_string())
            .await;

        manager.clear(SseStreamType::Alerts).await;

        let replay = manager.get_replay_events(SseStreamType::Alerts, 0).await;
        assert!(replay.is_empty());
    }
}
