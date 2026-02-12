//! SSE Event Manager for reliable streaming with replay support
//!
//! This module provides robust Server-Sent Events (SSE) streaming with:
//!
//! - **Monotonic event IDs** - Sequential u64 IDs per stream type
//! - **Ring buffer storage** - Bounded event history with drop-oldest semantics
//! - **Last-Event-ID support** - Client reconnection with automatic replay
//! - **Gap detection** - Notify clients when events were missed
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    SseEventManager                       │
//! │  ┌───────────────────────────────────────────────────┐  │
//! │  │              DashMap<SseStreamType, Buffer>        │  │
//! │  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐  │  │
//! │  │  │  Metrics    │ │  Telemetry  │ │  Training   │  │  │
//! │  │  │ RingBuffer  │ │ RingBuffer  │ │ RingBuffer  │  │  │
//! │  │  │ seq: 42     │ │ seq: 1000   │ │ seq: 15     │  │  │
//! │  │  └─────────────┘ └─────────────┘ └─────────────┘  │  │
//! │  └───────────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ## Basic Event Creation
//!
//! ```ignore
//! use adapteros_server_api::sse::{SseEventManager, SseStreamType};
//!
//! let manager = SseEventManager::new();
//!
//! // Create event with monotonic ID
//! let event = manager
//!     .create_event(SseStreamType::SystemMetrics, "metrics", json_data)
//!     .await;
//!
//! // Convert to Axum SSE event for streaming
//! let sse_event = SseEventManager::to_axum_event(&event);
//! ```
//!
//! ## Handling Reconnection
//!
//! ```ignore
//! use axum::http::HeaderMap;
//!
//! pub async fn stream_handler(
//!     State(state): State<AppState>,
//!     headers: HeaderMap,
//! ) -> Sse<impl Stream<...>> {
//!     // Parse Last-Event-ID from reconnecting client
//!     let last_id = SseEventManager::parse_last_event_id(&headers);
//!
//!     // Replay missed events
//!     let replay_events = if let Some(id) = last_id {
//!         state.sse_manager.get_replay_events(SseStreamType::Metrics, id).await
//!     } else {
//!         Vec::new()
//!     };
//!
//!     // Chain replay with live stream
//!     let replay_stream = stream::iter(replay_events.into_iter().map(|e|
//!         Ok(SseEventManager::to_axum_event(&e))
//!     ));
//!
//!     let live_stream = /* ... existing stream logic ... */;
//!
//!     Sse::new(replay_stream.chain(live_stream))
//! }
//! ```
//!
//! # SSE Protocol Compliance
//!
//! Events are formatted according to the SSE specification:
//!
//! ```text
//! id: 42
//! event: metrics
//! retry: 3000
//! data: {"cpu": 50, "memory": 60}
//!
//! ```
//!
//! The `id` field enables clients to reconnect using the `Last-Event-ID` header.
//! The `retry` field suggests reconnection timing to the client.

mod event_manager;
pub mod lifecycle_events;
mod ring_buffer;
mod types;

pub use event_manager::{ReplayResult, SseEventManager, DEFAULT_BUFFER_CAPACITY, DEFAULT_RETRY_MS};
pub use lifecycle_events::{
    AdapterLifecycleEvent, AdapterVersionEvent, SystemHealthEvent, TrainingLifecycleEvent,
};
pub use ring_buffer::{BufferStats, SseRingBuffer};
pub use types::{EventGapRecoveryHint, SseErrorEvent, SseEvent, SseStreamType};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use axum::http::HeaderMap;
    use futures_util::stream::{self, StreamExt};
    use std::sync::Arc;

    /// Test full reconnection flow with replay
    #[tokio::test]
    async fn test_reconnection_flow() {
        let manager = Arc::new(SseEventManager::new());

        // Simulate initial connection receiving events 0-4
        for i in 0..5 {
            manager
                .create_event(
                    SseStreamType::SystemMetrics,
                    "metrics",
                    format!(r#"{{"value": {}}}"#, i),
                )
                .await;
        }

        // Client disconnects after receiving event ID 2

        // Server continues producing events 5-9
        for i in 5..10 {
            manager
                .create_event(
                    SseStreamType::SystemMetrics,
                    "metrics",
                    format!(r#"{{"value": {}}}"#, i),
                )
                .await;
        }

        // Client reconnects with Last-Event-ID: 2
        let mut headers = HeaderMap::new();
        headers.insert("Last-Event-ID", "2".parse().unwrap());

        let last_id = SseEventManager::parse_last_event_id(&headers).unwrap();
        assert_eq!(last_id, 2);

        // Get replay events
        let replay = manager
            .get_replay_events(SseStreamType::SystemMetrics, last_id)
            .await;

        // Should receive events 3-9 (7 events)
        assert_eq!(replay.len(), 7);
        assert_eq!(replay[0].id, 3);
        assert_eq!(replay[6].id, 9);

        // Verify monotonic ordering
        for window in replay.windows(2) {
            assert!(window[1].id > window[0].id);
        }
    }

    /// Test gap detection and warning
    #[tokio::test]
    async fn test_gap_warning_flow() {
        let manager = SseEventManager::with_capacity(5);

        // Produce 10 events (buffer holds only 5)
        for _ in 0..10 {
            manager
                .create_event(SseStreamType::Alerts, "alert", "{}".to_string())
                .await;
        }

        // Client reconnects with old ID (gap exists)
        let result = manager
            .get_replay_with_analysis(SseStreamType::Alerts, 2)
            .await;

        assert!(result.has_gap);
        assert_eq!(result.dropped_count, 5);

        // Create gap warning for client
        let warning = manager.create_gap_warning(SseStreamType::Alerts, 2).await;
        assert_eq!(warning.event_type, "warning");
        assert!(warning.data.contains("gap_detected"));
    }

    /// Test stream simulation with replay chain
    #[tokio::test]
    async fn test_stream_chain_simulation() {
        let manager = Arc::new(SseEventManager::new());

        // Create some historical events
        for i in 0..3 {
            manager
                .create_default_event(SseStreamType::Training, format!(r#"{{"epoch": {}}}"#, i))
                .await;
        }

        // Simulate reconnection with Last-Event-ID: 0
        let replay_events = manager.get_replay_events(SseStreamType::Training, 0).await;

        // Create replay stream
        let replay_stream = stream::iter(
            replay_events
                .into_iter()
                .map(|e| Ok::<_, std::convert::Infallible>(SseEventManager::to_axum_event(&e))),
        );

        // Create live stream (simulated)
        let mgr_clone = Arc::clone(&manager);
        let live_stream = stream::unfold(0, move |count| {
            let mgr = Arc::clone(&mgr_clone);
            async move {
                if count >= 2 {
                    return None;
                }
                let event = mgr
                    .create_default_event(
                        SseStreamType::Training,
                        format!(r#"{{"live": {}}}"#, count),
                    )
                    .await;
                Some((
                    Ok::<_, std::convert::Infallible>(SseEventManager::to_axum_event(&event)),
                    count + 1,
                ))
            }
        });

        // Chain replay with live
        let combined = replay_stream.chain(live_stream);

        // Collect all events
        let events: Vec<_> = combined.collect().await;

        // Should have 2 replay + 2 live = 4 events
        assert_eq!(events.len(), 4);
    }
}
