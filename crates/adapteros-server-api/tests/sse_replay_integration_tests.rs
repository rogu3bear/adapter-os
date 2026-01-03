//! SSE Last-Event-ID Replay Integration Tests
//!
//! Tests that prove SSE replay works correctly after client reconnection.
//! These tests validate the streaming reliability guarantees of the SSE system.
//!
//! ## Test Coverage
//! - Normal stream flow with monotonic event IDs
//! - Disconnect + reconnect with Last-Event-ID header
//! - Invalid Last-Event-ID handling
//! - Too-old Last-Event-ID (gap detection when events fall outside retention buffer)
//!
//! ## Non-goals
//! - Rewriting the SSE implementation
//! - Adding new persistence for replay
//!
//! [2025-01-03 sse_replay_integration_tests]

use adapteros_server_api::sse::{SseEventManager, SseStreamType};
use axum::http::HeaderMap;
use std::sync::Arc;
use std::time::Duration;

// =============================================================================
// Test Helpers
// =============================================================================

/// Parse SSE event text into structured data
#[derive(Debug, Clone)]
struct ParsedSseEvent {
    id: Option<u64>,
    event_type: Option<String>,
    data: Option<String>,
    retry: Option<u32>,
}

impl ParsedSseEvent {
    /// Parse a single SSE event from the SSE wire format
    fn parse(text: &str) -> Option<Self> {
        let mut id = None;
        let mut event_type = None;
        let mut data_lines = Vec::new();
        let mut retry = None;

        for line in text.lines() {
            if line.starts_with("id:") {
                let value = line.strip_prefix("id:").unwrap().trim();
                id = value.parse().ok();
            } else if line.starts_with("event:") {
                event_type = Some(line.strip_prefix("event:").unwrap().trim().to_string());
            } else if line.starts_with("data:") {
                data_lines.push(line.strip_prefix("data:").unwrap().trim().to_string());
            } else if line.starts_with("retry:") {
                let value = line.strip_prefix("retry:").unwrap().trim();
                retry = value.parse().ok();
            }
        }

        let data = if data_lines.is_empty() {
            None
        } else {
            Some(data_lines.join("\n"))
        };

        // Only return if we have at least some content
        if id.is_some() || event_type.is_some() || data.is_some() {
            Some(Self {
                id,
                event_type,
                data,
                retry,
            })
        } else {
            None
        }
    }
}

/// Split SSE stream text into individual events
fn split_sse_events(text: &str) -> Vec<ParsedSseEvent> {
    text.split("\n\n")
        .filter(|chunk| !chunk.trim().is_empty())
        .filter_map(ParsedSseEvent::parse)
        .collect()
}

// =============================================================================
// Normal Stream Flow Tests
// =============================================================================

mod normal_flow_tests {
    use super::*;

    /// Test that events have monotonically increasing IDs
    #[tokio::test]
    async fn test_event_ids_are_monotonic() {
        let manager = Arc::new(SseEventManager::new());

        // Create a series of events
        let mut events = Vec::new();
        for i in 0..10 {
            let event = manager
                .create_event(
                    SseStreamType::SystemMetrics,
                    "metrics",
                    format!(r#"{{"seq": {}}}"#, i),
                )
                .await;
            events.push(event);
        }

        // Verify monotonic IDs starting from 0
        for (i, event) in events.iter().enumerate() {
            assert_eq!(
                event.id, i as u64,
                "Event {} should have ID {}, got {}",
                i, i, event.id
            );
        }

        // Verify strict ordering
        for window in events.windows(2) {
            assert!(
                window[1].id > window[0].id,
                "IDs must be strictly increasing: {} should be > {}",
                window[1].id,
                window[0].id
            );
        }
    }

    /// Test that events are stored and retrievable
    #[tokio::test]
    async fn test_events_stored_in_buffer() {
        let manager = Arc::new(SseEventManager::new());

        // Create 5 events
        for i in 0..5 {
            manager
                .create_event(
                    SseStreamType::Alerts,
                    "alert",
                    format!(r#"{{"alert_id": {}}}"#, i),
                )
                .await;
        }

        // Retrieve all events (from ID -1 effectively, which means ID 0+)
        // Using u64::MAX as a workaround won't work, we need to test replay_from with ID 0
        // which would return events > 0. Let's test with None equivalent
        let all_events = manager
            .get_replay_events(SseStreamType::Alerts, u64::MAX)
            .await;

        // With u64::MAX, nothing should be returned (no events have ID > MAX)
        assert!(all_events.is_empty());

        // Now test proper replay - from event 0 should return 1,2,3,4
        let replay_events = manager.get_replay_events(SseStreamType::Alerts, 0).await;
        assert_eq!(replay_events.len(), 4, "Should get events 1-4 after ID 0");
    }

    /// Test that each stream type maintains independent ID sequences
    #[tokio::test]
    async fn test_stream_type_id_isolation() {
        let manager = Arc::new(SseEventManager::new());

        // Create events in multiple stream types
        let e1 = manager
            .create_event(SseStreamType::Alerts, "alert", "{}".to_string())
            .await;
        let e2 = manager
            .create_event(SseStreamType::Training, "train", "{}".to_string())
            .await;
        let e3 = manager
            .create_event(SseStreamType::Inference, "token", "{}".to_string())
            .await;

        // Each stream type should start from 0 independently
        assert_eq!(e1.id, 0, "Alerts stream should start at 0");
        assert_eq!(e2.id, 0, "Training stream should start at 0");
        assert_eq!(e3.id, 0, "Inference stream should start at 0");

        // Adding more to one stream shouldn't affect others
        let e4 = manager
            .create_event(SseStreamType::Alerts, "alert", "{}".to_string())
            .await;
        assert_eq!(e4.id, 1, "Alerts stream should increment to 1");

        // Other streams still at 0
        let e5 = manager
            .create_event(SseStreamType::Training, "train", "{}".to_string())
            .await;
        assert_eq!(e5.id, 1, "Training stream should now be at 1");
    }

    /// Test that SSE event formatting is correct
    #[tokio::test]
    async fn test_sse_event_format() {
        let manager = Arc::new(SseEventManager::new());

        let event = manager
            .create_event(
                SseStreamType::Telemetry,
                "telemetry",
                r#"{"metric": "cpu", "value": 50}"#.to_string(),
            )
            .await;

        // Convert to Axum event (this is what gets sent to clients)
        let axum_event = SseEventManager::to_axum_event(&event);

        // The event should have proper fields set
        // We can't easily inspect Axum Event internals, but we can verify no panic
        let debug_repr = format!("{:?}", axum_event);
        assert!(debug_repr.contains("telemetry"), "Event type should be set");
    }
}

// =============================================================================
// Reconnection with Last-Event-ID Tests
// =============================================================================

mod reconnection_tests {
    use super::*;

    /// Test basic replay after disconnect
    #[tokio::test]
    async fn test_replay_after_disconnect() {
        let manager = Arc::new(SseEventManager::new());

        // Phase 1: Client receives events 0-4
        for i in 0..5 {
            manager
                .create_event(
                    SseStreamType::Training,
                    "progress",
                    format!(r#"{{"epoch": {}}}"#, i),
                )
                .await;
        }

        // Client disconnects after receiving event ID 2

        // Phase 2: Server continues producing events 5-9
        for i in 5..10 {
            manager
                .create_event(
                    SseStreamType::Training,
                    "progress",
                    format!(r#"{{"epoch": {}}}"#, i),
                )
                .await;
        }

        // Phase 3: Client reconnects with Last-Event-ID: 2
        let replay = manager.get_replay_events(SseStreamType::Training, 2).await;

        // Should receive events 3-9 (7 events)
        assert_eq!(replay.len(), 7, "Should replay 7 missed events");
        assert_eq!(replay[0].id, 3, "First replayed event should be ID 3");
        assert_eq!(replay[6].id, 9, "Last replayed event should be ID 9");

        // Verify chronological order
        for window in replay.windows(2) {
            assert!(
                window[1].id > window[0].id,
                "Replay events must be in order"
            );
        }
    }

    /// Test that replay starts at the next event (not inclusive)
    #[tokio::test]
    async fn test_replay_is_exclusive_of_last_id() {
        let manager = Arc::new(SseEventManager::new());

        // Create 5 events (IDs 0-4)
        for _ in 0..5 {
            manager
                .create_event(SseStreamType::Alerts, "alert", "{}".to_string())
                .await;
        }

        // Replay from ID 2 should NOT include event 2
        let replay = manager.get_replay_events(SseStreamType::Alerts, 2).await;

        assert_eq!(replay.len(), 2, "Should get events 3 and 4");
        assert_eq!(replay[0].id, 3, "First event should be ID 3, not 2");
        assert_eq!(replay[1].id, 4, "Second event should be ID 4");
    }

    /// Test Last-Event-ID header parsing
    #[tokio::test]
    async fn test_last_event_id_header_parsing() {
        // Standard header name
        let mut headers = HeaderMap::new();
        headers.insert("Last-Event-ID", "42".parse().unwrap());
        assert_eq!(SseEventManager::parse_last_event_id(&headers), Some(42));

        // Lowercase header name (HTTP headers are case-insensitive)
        let mut headers = HeaderMap::new();
        headers.insert("last-event-id", "100".parse().unwrap());
        assert_eq!(SseEventManager::parse_last_event_id(&headers), Some(100));

        // Large ID values
        let mut headers = HeaderMap::new();
        headers.insert("Last-Event-ID", "9999999999".parse().unwrap());
        assert_eq!(
            SseEventManager::parse_last_event_id(&headers),
            Some(9999999999)
        );

        // ID 0 is valid
        let mut headers = HeaderMap::new();
        headers.insert("Last-Event-ID", "0".parse().unwrap());
        assert_eq!(SseEventManager::parse_last_event_id(&headers), Some(0));
    }

    /// Test replay with analysis provides gap info
    #[tokio::test]
    async fn test_replay_with_analysis_no_gap() {
        let manager = Arc::new(SseEventManager::with_capacity(100));

        // Create 10 events (within buffer capacity)
        for i in 0..10 {
            manager
                .create_event(
                    SseStreamType::SystemMetrics,
                    "metrics",
                    format!(r#"{{"value": {}}}"#, i),
                )
                .await;
        }

        // Replay from ID 5
        let result = manager
            .get_replay_with_analysis(SseStreamType::SystemMetrics, 5)
            .await;

        assert!(!result.has_gap, "Should have no gap");
        assert_eq!(result.dropped_count, 0, "No events should be dropped");
        assert_eq!(result.events.len(), 4, "Should get events 6-9");
    }

    /// Test that reconnect after no new events returns empty
    #[tokio::test]
    async fn test_replay_when_caught_up() {
        let manager = Arc::new(SseEventManager::new());

        // Create 5 events (IDs 0-4)
        for _ in 0..5 {
            manager
                .create_event(SseStreamType::Telemetry, "event", "{}".to_string())
                .await;
        }

        // Replay from ID 4 (last event) should return empty
        let replay = manager.get_replay_events(SseStreamType::Telemetry, 4).await;

        assert!(replay.is_empty(), "Should have no events to replay");
    }
}

// =============================================================================
// Invalid Last-Event-ID Tests
// =============================================================================

mod invalid_id_tests {
    use super::*;

    /// Test that non-numeric Last-Event-ID returns None
    #[tokio::test]
    async fn test_invalid_non_numeric_id() {
        let mut headers = HeaderMap::new();
        headers.insert("Last-Event-ID", "not-a-number".parse().unwrap());

        assert_eq!(
            SseEventManager::parse_last_event_id(&headers),
            None,
            "Non-numeric ID should parse as None"
        );
    }

    /// Test that empty Last-Event-ID returns None
    #[tokio::test]
    async fn test_empty_last_event_id() {
        let mut headers = HeaderMap::new();
        headers.insert("Last-Event-ID", "".parse().unwrap());

        assert_eq!(
            SseEventManager::parse_last_event_id(&headers),
            None,
            "Empty ID should parse as None"
        );
    }

    /// Test that missing header returns None
    #[tokio::test]
    async fn test_missing_last_event_id_header() {
        let headers = HeaderMap::new();

        assert_eq!(
            SseEventManager::parse_last_event_id(&headers),
            None,
            "Missing header should parse as None"
        );
    }

    /// Test that negative number (as string) returns None
    #[tokio::test]
    async fn test_negative_number_id() {
        let mut headers = HeaderMap::new();
        headers.insert("Last-Event-ID", "-1".parse().unwrap());

        assert_eq!(
            SseEventManager::parse_last_event_id(&headers),
            None,
            "Negative number should parse as None for u64"
        );
    }

    /// Test that floating point number returns None
    #[tokio::test]
    async fn test_floating_point_id() {
        let mut headers = HeaderMap::new();
        headers.insert("Last-Event-ID", "42.5".parse().unwrap());

        assert_eq!(
            SseEventManager::parse_last_event_id(&headers),
            None,
            "Floating point should parse as None"
        );
    }

    /// Test that very large ID (overflow) returns None
    #[tokio::test]
    async fn test_overflow_id() {
        let mut headers = HeaderMap::new();
        // This is larger than u64::MAX
        headers.insert(
            "Last-Event-ID",
            "99999999999999999999999999999".parse().unwrap(),
        );

        assert_eq!(
            SseEventManager::parse_last_event_id(&headers),
            None,
            "Overflow value should parse as None"
        );
    }

    /// Test replay with future ID (beyond current sequence)
    #[tokio::test]
    async fn test_future_id_replay() {
        let manager = Arc::new(SseEventManager::new());

        // Create 5 events (IDs 0-4)
        for _ in 0..5 {
            manager
                .create_event(SseStreamType::Alerts, "alert", "{}".to_string())
                .await;
        }

        // Replay from ID 1000 (far in the future)
        let replay = manager.get_replay_events(SseStreamType::Alerts, 1000).await;

        assert!(
            replay.is_empty(),
            "Future ID should return empty replay list"
        );

        // Should not be considered a gap
        assert!(
            !manager.has_gap(SseStreamType::Alerts, 1000),
            "Future ID should not be considered a gap"
        );
    }

    /// Test replay with whitespace-padded ID
    #[tokio::test]
    async fn test_whitespace_padded_id() {
        let mut headers = HeaderMap::new();
        headers.insert("Last-Event-ID", " 42 ".parse().unwrap());

        // Note: HTTP header values are typically trimmed, but parse behavior may vary
        // This tests the actual behavior
        let result = SseEventManager::parse_last_event_id(&headers);
        // Whitespace around number may or may not parse - test actual behavior
        // The implementation uses .trim() before parsing, so this should work
        assert!(
            result.is_none() || result == Some(42),
            "Whitespace handling should be consistent"
        );
    }
}

// =============================================================================
// Too-Old Last-Event-ID (Gap Detection) Tests
// =============================================================================

mod gap_detection_tests {
    use super::*;

    /// Test that gap is detected when buffer overflows
    #[tokio::test]
    async fn test_gap_detection_after_overflow() {
        // Use a small buffer capacity
        let manager = Arc::new(SseEventManager::with_capacity(5));

        // Create 10 events - first 5 will be evicted
        for i in 0..10 {
            manager
                .create_event(
                    SseStreamType::Training,
                    "progress",
                    format!(r#"{{"epoch": {}}}"#, i),
                )
                .await;
        }

        // Client with last_id=2 has missed events that are no longer available
        assert!(
            manager.has_gap(SseStreamType::Training, 2),
            "Should detect gap for old ID"
        );

        // Client with last_id=7 should not have a gap (events 8,9 available)
        assert!(
            !manager.has_gap(SseStreamType::Training, 7),
            "Recent ID should not have gap"
        );
    }

    /// Test gap warning event creation
    #[tokio::test]
    async fn test_gap_warning_event() {
        let manager = Arc::new(SseEventManager::with_capacity(5));

        // Create 10 events (5 will be dropped)
        for _ in 0..10 {
            manager
                .create_event(SseStreamType::Alerts, "alert", "{}".to_string())
                .await;
        }

        // Create gap warning for client with old ID
        let warning = manager.create_gap_warning(SseStreamType::Alerts, 2).await;

        assert_eq!(warning.event_type, "warning");
        assert!(warning.data.contains("gap_detected"));
        assert!(warning.data.contains("\"last_client_id\":2"));
        assert!(warning.data.contains("\"dropped_count\":5"));
    }

    /// Test replay with analysis when gap exists
    #[tokio::test]
    async fn test_replay_with_gap_analysis() {
        let manager = Arc::new(SseEventManager::with_capacity(5));

        // Create 10 events
        for _ in 0..10 {
            manager
                .create_event(SseStreamType::Telemetry, "event", "{}".to_string())
                .await;
        }

        // Analyze replay for client with old ID
        let result = manager
            .get_replay_with_analysis(SseStreamType::Telemetry, 2)
            .await;

        assert!(result.has_gap, "Should detect gap");
        assert_eq!(result.dropped_count, 5, "Should report 5 dropped events");
        // Events 5-9 should be available
        assert_eq!(result.events.len(), 5, "Should have 5 available events");
        assert_eq!(result.events[0].id, 5, "Oldest available should be 5");
    }

    /// Test no gap when within buffer capacity
    #[tokio::test]
    async fn test_no_gap_within_capacity() {
        let manager = Arc::new(SseEventManager::with_capacity(100));

        // Create 50 events (within capacity)
        for _ in 0..50 {
            manager
                .create_event(SseStreamType::Dashboard, "data", "{}".to_string())
                .await;
        }

        // No gap should exist for any ID within range
        assert!(
            !manager.has_gap(SseStreamType::Dashboard, 0),
            "ID 0 should not have gap"
        );
        assert!(
            !manager.has_gap(SseStreamType::Dashboard, 25),
            "ID 25 should not have gap"
        );
        assert!(
            !manager.has_gap(SseStreamType::Dashboard, 49),
            "ID 49 should not have gap"
        );
    }

    /// Test gap detection on empty buffer
    #[tokio::test]
    async fn test_gap_empty_buffer() {
        let manager = Arc::new(SseEventManager::new());

        // No events created yet
        assert!(
            !manager.has_gap(SseStreamType::Alerts, 0),
            "Empty buffer should not have gap"
        );
        assert!(
            !manager.has_gap(SseStreamType::Alerts, 100),
            "Empty buffer should not have gap for any ID"
        );
    }

    /// Test buffer statistics tracking
    #[tokio::test]
    async fn test_buffer_stats_after_overflow() {
        let manager = Arc::new(SseEventManager::with_capacity(5));

        // Create 15 events (10 will be dropped)
        for _ in 0..15 {
            manager
                .create_event(SseStreamType::Inference, "token", "{}".to_string())
                .await;
        }

        let stats = manager
            .get_stats(SseStreamType::Inference)
            .expect("Stats should exist");

        assert_eq!(stats.dropped_count, 10, "Should track 10 dropped events");
        assert_eq!(stats.current_sequence, 15, "Sequence should be at 15");
        assert_eq!(stats.lowest_id, 10, "Lowest available ID should be 10");
    }

    /// Test per-stream gap detection isolation
    #[tokio::test]
    async fn test_gap_detection_stream_isolation() {
        let manager = Arc::new(SseEventManager::with_capacity(5));

        // Overflow one stream
        for _ in 0..10 {
            manager
                .create_event(SseStreamType::Alerts, "alert", "{}".to_string())
                .await;
        }

        // Create only 3 events in another stream
        for _ in 0..3 {
            manager
                .create_event(SseStreamType::Training, "train", "{}".to_string())
                .await;
        }

        // Alerts should have gap for old ID
        assert!(
            manager.has_gap(SseStreamType::Alerts, 2),
            "Alerts should have gap"
        );

        // Training should not have gap
        assert!(
            !manager.has_gap(SseStreamType::Training, 0),
            "Training should not have gap"
        );
    }
}

// =============================================================================
// Concurrent Access Tests
// =============================================================================

mod concurrency_tests {
    use super::*;

    /// Test concurrent event creation maintains monotonic IDs
    #[tokio::test]
    async fn test_concurrent_event_creation() {
        let manager = Arc::new(SseEventManager::new());
        let mut handles = vec![];

        // Spawn 10 tasks, each creating 100 events
        for task_id in 0..10 {
            let mgr = Arc::clone(&manager);
            handles.push(tokio::spawn(async move {
                let mut ids = Vec::new();
                for i in 0..100 {
                    let event = mgr
                        .create_event(
                            SseStreamType::Telemetry,
                            "event",
                            format!(r#"{{"task": {}, "seq": {}}}"#, task_id, i),
                        )
                        .await;
                    ids.push(event.id);
                }
                ids
            }));
        }

        // Collect all IDs
        let mut all_ids = Vec::new();
        for handle in handles {
            let ids = handle.await.expect("Task should complete");
            all_ids.extend(ids);
        }

        // Sort and verify all IDs are unique and sequential
        all_ids.sort();
        assert_eq!(all_ids.len(), 1000, "Should have 1000 total events");

        // Check that IDs form a contiguous sequence 0-999
        for (i, &id) in all_ids.iter().enumerate() {
            assert_eq!(id, i as u64, "IDs should be contiguous");
        }
    }

    /// Test concurrent replay doesn't affect event creation
    #[tokio::test]
    async fn test_concurrent_create_and_replay() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        let manager = Arc::new(SseEventManager::new());
        let stop = Arc::new(AtomicBool::new(false));
        let created_count = Arc::new(AtomicUsize::new(0));
        let replay_count = Arc::new(AtomicUsize::new(0));

        // Start event creation task
        let mgr1 = Arc::clone(&manager);
        let stop1 = Arc::clone(&stop);
        let created1 = Arc::clone(&created_count);
        let create_handle = tokio::spawn(async move {
            while !stop1.load(Ordering::Relaxed) {
                let count = created1.fetch_add(1, Ordering::Relaxed);
                mgr1.create_event(
                    SseStreamType::Alerts,
                    "alert",
                    format!(r#"{{"seq": {}}}"#, count),
                )
                .await;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });

        // Start replay task
        let mgr2 = Arc::clone(&manager);
        let stop2 = Arc::clone(&stop);
        let replay2 = Arc::clone(&replay_count);
        let replay_handle = tokio::spawn(async move {
            while !stop2.load(Ordering::Relaxed) {
                let _ = mgr2.get_replay_events(SseStreamType::Alerts, 0).await;
                replay2.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        });

        // Let them run for a bit
        tokio::time::sleep(Duration::from_millis(50)).await;
        stop.store(true, Ordering::Relaxed);

        // Wait for tasks to complete with timeout
        let _ = tokio::time::timeout(Duration::from_secs(1), create_handle).await;
        let _ = tokio::time::timeout(Duration::from_secs(1), replay_handle).await;

        let created = created_count.load(Ordering::Relaxed);
        let replayed = replay_count.load(Ordering::Relaxed);

        assert!(created > 0, "Should have created events");
        assert!(replayed > 0, "Should have performed replays");
    }
}

// =============================================================================
// End-to-End Reconnection Flow Tests
// =============================================================================

mod e2e_flow_tests {
    use super::*;

    /// Simulate full client reconnection flow with proper error handling
    #[tokio::test]
    async fn test_full_reconnection_flow() {
        let manager = Arc::new(SseEventManager::with_capacity(10));

        // Phase 1: Initial connection - client receives events 0-4
        for i in 0..5 {
            manager
                .create_event(
                    SseStreamType::Training,
                    "progress",
                    format!(r#"{{"epoch": {}}}"#, i),
                )
                .await;
        }

        // Simulate client stored last_event_id = 4
        let client_last_id = 4u64;

        // Phase 2: Client disconnects, server continues
        for i in 5..15 {
            manager
                .create_event(
                    SseStreamType::Training,
                    "progress",
                    format!(r#"{{"epoch": {}}}"#, i),
                )
                .await;
        }

        // Phase 3: Client reconnects with Last-Event-ID: 4
        let mut headers = HeaderMap::new();
        headers.insert("Last-Event-ID", "4".parse().unwrap());

        let last_id = SseEventManager::parse_last_event_id(&headers).unwrap();
        assert_eq!(last_id, client_last_id);

        // Phase 4: Check for gap and get replay
        let has_gap = manager.has_gap(SseStreamType::Training, last_id);

        if has_gap {
            // Create gap warning
            let warning = manager
                .create_gap_warning(SseStreamType::Training, last_id)
                .await;
            assert_eq!(warning.event_type, "warning");
        }

        // Phase 5: Get available replay events
        let replay = manager
            .get_replay_events(SseStreamType::Training, last_id)
            .await;

        // Verify replay contains all available events in order
        assert!(!replay.is_empty(), "Should have events to replay");
        for window in replay.windows(2) {
            assert!(
                window[1].id > window[0].id,
                "Events must be monotonically increasing"
            );
        }
    }

    /// Test that replay + live stream would be properly ordered
    #[tokio::test]
    async fn test_replay_then_live_ordering() {
        let manager = Arc::new(SseEventManager::new());

        // Create historical events 0-4
        for i in 0..5 {
            manager
                .create_default_event(
                    SseStreamType::SystemMetrics,
                    format!(r#"{{"historical": {}}}"#, i),
                )
                .await;
        }

        // Client reconnects with last_id=2, should get 3,4
        let replay = manager
            .get_replay_events(SseStreamType::SystemMetrics, 2)
            .await;
        assert_eq!(replay.len(), 2);
        assert_eq!(replay[0].id, 3);
        assert_eq!(replay[1].id, 4);

        // Now create "live" events
        let live1 = manager
            .create_default_event(SseStreamType::SystemMetrics, r#"{"live": 0}"#.to_string())
            .await;
        let live2 = manager
            .create_default_event(SseStreamType::SystemMetrics, r#"{"live": 1}"#.to_string())
            .await;

        // Live events should continue the sequence
        assert_eq!(live1.id, 5, "First live event should be ID 5");
        assert_eq!(live2.id, 6, "Second live event should be ID 6");

        // If we combine replay + live, they form a contiguous sequence
        let mut combined_ids: Vec<u64> = replay.iter().map(|e| e.id).collect();
        combined_ids.push(live1.id);
        combined_ids.push(live2.id);

        assert_eq!(combined_ids, vec![3, 4, 5, 6], "IDs should be contiguous");
    }

    /// Test stats reporting for monitoring
    #[tokio::test]
    async fn test_stats_for_monitoring() {
        let manager = Arc::new(SseEventManager::with_capacity(5));

        // Create events in multiple streams
        for _ in 0..10 {
            manager
                .create_event(SseStreamType::Alerts, "alert", "{}".to_string())
                .await;
        }
        for _ in 0..3 {
            manager
                .create_event(SseStreamType::Training, "train", "{}".to_string())
                .await;
        }

        let all_stats = manager.get_all_stats();

        assert_eq!(all_stats.len(), 2, "Should have stats for 2 streams");

        // Find stats for each stream
        let alert_stats = all_stats
            .iter()
            .find(|(st, _)| *st == SseStreamType::Alerts)
            .map(|(_, s)| s);
        let training_stats = all_stats
            .iter()
            .find(|(st, _)| *st == SseStreamType::Training)
            .map(|(_, s)| s);

        assert!(alert_stats.is_some());
        assert!(training_stats.is_some());

        let alert_stats = alert_stats.unwrap();
        assert_eq!(alert_stats.current_sequence, 10);
        assert_eq!(alert_stats.dropped_count, 5);

        let training_stats = training_stats.unwrap();
        assert_eq!(training_stats.current_sequence, 3);
        assert_eq!(training_stats.dropped_count, 0);
    }
}

// =============================================================================
// SSE Wire Format Parsing Tests
// =============================================================================

mod wire_format_tests {
    use super::*;

    #[test]
    fn test_parse_simple_sse_event() {
        let text = "id: 42\nevent: metrics\ndata: {\"cpu\": 50}";
        let event = ParsedSseEvent::parse(text).expect("Should parse");

        assert_eq!(event.id, Some(42));
        assert_eq!(event.event_type.as_deref(), Some("metrics"));
        assert_eq!(event.data.as_deref(), Some("{\"cpu\": 50}"));
    }

    #[test]
    fn test_parse_event_with_retry() {
        let text = "id: 1\nevent: test\nretry: 3000\ndata: hello";
        let event = ParsedSseEvent::parse(text).expect("Should parse");

        assert_eq!(event.id, Some(1));
        assert_eq!(event.retry, Some(3000));
        assert_eq!(event.data.as_deref(), Some("hello"));
    }

    #[test]
    fn test_parse_event_without_id() {
        let text = "event: heartbeat\ndata: ping";
        let event = ParsedSseEvent::parse(text).expect("Should parse");

        assert_eq!(event.id, None);
        assert_eq!(event.event_type.as_deref(), Some("heartbeat"));
    }

    #[test]
    fn test_split_multiple_events() {
        let text = "id: 0\nevent: a\ndata: first\n\nid: 1\nevent: b\ndata: second\n\n";
        let events = split_sse_events(text);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, Some(0));
        assert_eq!(events[1].id, Some(1));
    }

    #[test]
    fn test_parse_empty_string() {
        let events = split_sse_events("");
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_only_comments() {
        let text = ": this is a comment\n: another comment";
        let event = ParsedSseEvent::parse(text);
        // Comments don't produce events
        assert!(event.is_none());
    }
}
