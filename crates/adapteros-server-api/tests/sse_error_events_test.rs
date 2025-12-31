//! SSE Error Events Integration Tests
//!
//! Tests for SSE error event handling including:
//! - SseErrorEvent serialization/deserialization
//! - Gap detection and warning creation
//! - Buffer overflow event generation
//! - Stream disconnect event handling
//!
//! [2025-12-31 sse_error_events_test]

use adapteros_server_api::sse::{
    BufferStats, EventGapRecoveryHint, SseErrorEvent, SseEvent, SseEventManager, SseRingBuffer,
    SseStreamType,
};
use std::sync::Arc;

// =============================================================================
// SseErrorEvent Serialization/Deserialization Tests
// =============================================================================

mod serialization_tests {
    use super::*;

    #[test]
    fn test_stream_disconnected_serialization() {
        let event = SseErrorEvent::disconnected(100, "server shutdown");
        let json = serde_json::to_string(&event).expect("serialization should succeed");

        // Verify JSON structure
        assert!(json.contains("\"type\":\"stream_disconnected\""));
        assert!(json.contains("\"last_event_id\":100"));
        assert!(json.contains("\"reason\":\"server shutdown\""));
        assert!(json.contains("\"reconnect_hint_ms\":3000"));

        // Verify deserialization
        let deserialized: SseErrorEvent =
            serde_json::from_str(&json).expect("deserialization should succeed");
        match deserialized {
            SseErrorEvent::StreamDisconnected {
                last_event_id,
                reason,
                reconnect_hint_ms,
            } => {
                assert_eq!(last_event_id, 100);
                assert_eq!(reason, "server shutdown");
                assert_eq!(reconnect_hint_ms, 3000);
            }
            _ => panic!("Expected StreamDisconnected variant"),
        }
    }

    #[test]
    fn test_buffer_overflow_serialization() {
        let event = SseErrorEvent::overflow(50, 150);
        let json = serde_json::to_string(&event).expect("serialization should succeed");

        // Verify JSON structure
        assert!(json.contains("\"type\":\"buffer_overflow\""));
        assert!(json.contains("\"dropped_count\":50"));
        assert!(json.contains("\"oldest_available_id\":150"));

        // Verify deserialization
        let deserialized: SseErrorEvent =
            serde_json::from_str(&json).expect("deserialization should succeed");
        match deserialized {
            SseErrorEvent::BufferOverflow {
                dropped_count,
                oldest_available_id,
            } => {
                assert_eq!(dropped_count, 50);
                assert_eq!(oldest_available_id, 150);
            }
            _ => panic!("Expected BufferOverflow variant"),
        }
    }

    #[test]
    fn test_event_gap_detected_serialization() {
        let event =
            SseErrorEvent::gap_detected(50, 100, 50, EventGapRecoveryHint::RefetchFullState);
        let json = serde_json::to_string(&event).expect("serialization should succeed");

        // Verify JSON structure
        assert!(json.contains("\"type\":\"event_gap_detected\""));
        assert!(json.contains("\"client_last_id\":50"));
        assert!(json.contains("\"server_oldest_id\":100"));
        assert!(json.contains("\"events_lost\":50"));
        assert!(json.contains("\"recovery_hint\":\"refetch_full_state\""));

        // Verify deserialization
        let deserialized: SseErrorEvent =
            serde_json::from_str(&json).expect("deserialization should succeed");
        match deserialized {
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
            _ => panic!("Expected EventGapDetected variant"),
        }
    }

    #[test]
    fn test_heartbeat_serialization() {
        let event = SseErrorEvent::heartbeat(42);
        let json = serde_json::to_string(&event).expect("serialization should succeed");

        // Verify JSON structure
        assert!(json.contains("\"type\":\"heartbeat\""));
        assert!(json.contains("\"current_id\":42"));
        assert!(json.contains("\"timestamp_ms\""));

        // Verify deserialization
        let deserialized: SseErrorEvent =
            serde_json::from_str(&json).expect("deserialization should succeed");
        match deserialized {
            SseErrorEvent::Heartbeat {
                current_id,
                timestamp_ms,
            } => {
                assert_eq!(current_id, 42);
                assert!(timestamp_ms > 0);
            }
            _ => panic!("Expected Heartbeat variant"),
        }
    }

    #[test]
    fn test_recovery_hint_variants_serialization() {
        // RefetchFullState
        let hint = EventGapRecoveryHint::RefetchFullState;
        let json = serde_json::to_string(&hint).unwrap();
        assert_eq!(json, "\"refetch_full_state\"");

        // ContinueWithGap
        let hint = EventGapRecoveryHint::ContinueWithGap;
        let json = serde_json::to_string(&hint).unwrap();
        assert_eq!(json, "\"continue_with_gap\"");

        // RestartStream
        let hint = EventGapRecoveryHint::RestartStream;
        let json = serde_json::to_string(&hint).unwrap();
        assert_eq!(json, "\"restart_stream\"");

        // RefetchResource
        let hint = EventGapRecoveryHint::RefetchResource {
            resource_type: "adapter".to_string(),
            resource_id: "abc123".to_string(),
        };
        let json = serde_json::to_string(&hint).unwrap();
        assert!(json.contains("\"refetch_resource\""));
        assert!(json.contains("\"resource_type\":\"adapter\""));
        assert!(json.contains("\"resource_id\":\"abc123\""));
    }

    #[test]
    fn test_error_event_to_sse_event() {
        let error_event = SseErrorEvent::disconnected(99, "connection timeout");
        let sse_event = error_event.to_sse_event(1);

        assert_eq!(sse_event.id, 1);
        assert_eq!(sse_event.event_type, "error");
        assert!(sse_event.data.contains("stream_disconnected"));
        assert!(sse_event.data.contains("connection timeout"));
        assert!(sse_event.data.contains("99"));
    }

    #[test]
    fn test_error_event_type_names() {
        assert_eq!(
            SseErrorEvent::disconnected(0, "").event_type(),
            "stream_disconnected"
        );
        assert_eq!(
            SseErrorEvent::overflow(0, 0).event_type(),
            "buffer_overflow"
        );
        assert_eq!(
            SseErrorEvent::gap_detected(0, 0, 0, EventGapRecoveryHint::RestartStream).event_type(),
            "event_gap"
        );
        assert_eq!(SseErrorEvent::heartbeat(0).event_type(), "heartbeat");
    }

    #[test]
    fn test_roundtrip_serialization_all_variants() {
        let events = vec![
            SseErrorEvent::disconnected(1000, "test disconnect"),
            SseErrorEvent::overflow(500, 600),
            SseErrorEvent::gap_detected(10, 100, 90, EventGapRecoveryHint::ContinueWithGap),
            SseErrorEvent::gap_detected(
                5,
                50,
                45,
                EventGapRecoveryHint::RefetchResource {
                    resource_type: "model".to_string(),
                    resource_id: "llama-7b".to_string(),
                },
            ),
            SseErrorEvent::heartbeat(999),
        ];

        for event in events {
            let json = serde_json::to_string(&event).expect("serialization should succeed");
            let roundtrip: SseErrorEvent =
                serde_json::from_str(&json).expect("deserialization should succeed");
            let json2 = serde_json::to_string(&roundtrip).expect("re-serialization should succeed");
            assert_eq!(json, json2, "Roundtrip should produce identical JSON");
        }
    }
}

// =============================================================================
// Gap Detection and Warning Creation Tests
// =============================================================================

mod gap_detection_tests {
    use super::*;

    #[tokio::test]
    async fn test_gap_detection_no_gap() {
        let manager = SseEventManager::with_capacity(10);

        // Create 5 events
        for _ in 0..5 {
            manager
                .create_event(SseStreamType::Alerts, "test", "{}".to_string())
                .await;
        }

        // Client with last_id=3 should have no gap (events 4-4 available)
        assert!(!manager.has_gap(SseStreamType::Alerts, 3));

        // Client with last_id=4 should have no gap
        assert!(!manager.has_gap(SseStreamType::Alerts, 4));
    }

    #[tokio::test]
    async fn test_gap_detection_with_gap() {
        let manager = SseEventManager::with_capacity(5);

        // Create 10 events (buffer holds only 5)
        for _ in 0..10 {
            manager
                .create_event(SseStreamType::Telemetry, "test", "{}".to_string())
                .await;
        }

        // Client with last_id=2 has a gap (events 3,4 were dropped)
        assert!(manager.has_gap(SseStreamType::Telemetry, 2));

        // Client with last_id=0 has a gap
        assert!(manager.has_gap(SseStreamType::Telemetry, 0));

        // Client with last_id=7 has no gap (events 8,9 available)
        assert!(!manager.has_gap(SseStreamType::Telemetry, 7));
    }

    #[tokio::test]
    async fn test_gap_warning_creation() {
        let manager = SseEventManager::with_capacity(5);

        // Create 10 events
        for _ in 0..10 {
            manager
                .create_event(SseStreamType::Training, "test", "{}".to_string())
                .await;
        }

        // Create gap warning for client with last_id=2
        let warning = manager.create_gap_warning(SseStreamType::Training, 2).await;

        assert_eq!(warning.event_type, "warning");
        assert!(warning.data.contains("gap_detected"));
        assert!(warning.data.contains("\"last_client_id\":2"));
        assert!(warning.data.contains("\"dropped_count\":5"));
    }

    #[tokio::test]
    async fn test_replay_with_analysis_detects_gap() {
        let manager = SseEventManager::with_capacity(5);

        // Create 10 events
        for _ in 0..10 {
            manager
                .create_event(SseStreamType::Anomalies, "test", "{}".to_string())
                .await;
        }

        // Analyze replay for client with last_id=2
        let result = manager
            .get_replay_with_analysis(SseStreamType::Anomalies, 2)
            .await;

        assert!(result.has_gap);
        assert_eq!(result.dropped_count, 5);
        // Events 5-9 should be available (5 events)
        assert_eq!(result.events.len(), 5);
        assert_eq!(result.events[0].id, 5);
        assert_eq!(result.events[4].id, 9);
    }

    #[tokio::test]
    async fn test_replay_with_analysis_no_gap() {
        let manager = SseEventManager::with_capacity(100);

        // Create 10 events (within capacity)
        for _ in 0..10 {
            manager
                .create_event(SseStreamType::SystemMetrics, "test", "{}".to_string())
                .await;
        }

        // Analyze replay for client with last_id=5
        let result = manager
            .get_replay_with_analysis(SseStreamType::SystemMetrics, 5)
            .await;

        assert!(!result.has_gap);
        assert_eq!(result.dropped_count, 0);
        // Events 6-9 should be returned (4 events)
        assert_eq!(result.events.len(), 4);
    }

    #[tokio::test]
    async fn test_gap_detection_empty_buffer() {
        let manager = SseEventManager::new();

        // No events created yet
        assert!(!manager.has_gap(SseStreamType::Alerts, 0));
        assert!(!manager.has_gap(SseStreamType::Alerts, 100));
    }

    #[tokio::test]
    async fn test_gap_warning_includes_stats() {
        let manager = SseEventManager::with_capacity(3);

        // Create 8 events (5 will be dropped)
        for i in 0..8 {
            manager
                .create_event(
                    SseStreamType::AdapterState,
                    "test",
                    format!("{{\"seq\":{}}}", i),
                )
                .await;
        }

        let warning = manager
            .create_gap_warning(SseStreamType::AdapterState, 1)
            .await;

        // Parse warning data
        let warning_data: serde_json::Value = serde_json::from_str(&warning.data).unwrap();

        assert_eq!(warning_data["warning"], "gap_detected");
        assert_eq!(warning_data["last_client_id"], 1);
        assert_eq!(warning_data["dropped_count"], 5);
        // Oldest available should be 5 (events 0-4 were dropped)
        assert_eq!(warning_data["oldest_available_id"], 5);
    }
}

// =============================================================================
// Buffer Overflow Event Generation Tests
// =============================================================================

mod buffer_overflow_tests {
    use super::*;

    #[tokio::test]
    async fn test_buffer_overflow_event_creation() {
        let event = SseErrorEvent::overflow(100, 200);

        match event {
            SseErrorEvent::BufferOverflow {
                dropped_count,
                oldest_available_id,
            } => {
                assert_eq!(dropped_count, 100);
                assert_eq!(oldest_available_id, 200);
            }
            _ => panic!("Expected BufferOverflow"),
        }
    }

    #[tokio::test]
    async fn test_buffer_overflow_tracking() {
        let buffer = SseRingBuffer::new(5);

        // Push 10 events
        for i in 0..10 {
            let id = buffer.next_id();
            let event = SseEvent::new(id, "test", format!("{}", i));
            buffer.push(event).await;
        }

        let stats = buffer.stats();
        assert_eq!(stats.dropped_count, 5);
        assert_eq!(stats.current_sequence, 10);
        assert_eq!(stats.lowest_id, 5);
    }

    #[tokio::test]
    async fn test_buffer_overflow_preserves_newest() {
        let buffer = SseRingBuffer::new(3);

        // Push 6 events
        for i in 0..6 {
            let id = buffer.next_id();
            let event = SseEvent::new(id, "test", format!("data_{}", i));
            buffer.push(event).await;
        }

        // Only events 3, 4, 5 should remain
        let all = buffer.get_all().await;
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].id, 3);
        assert_eq!(all[1].id, 4);
        assert_eq!(all[2].id, 5);
    }

    #[tokio::test]
    async fn test_overflow_event_generation_from_stats() {
        let manager = SseEventManager::with_capacity(5);

        // Create 15 events (10 will be dropped)
        for _ in 0..15 {
            manager
                .create_event(SseStreamType::Inference, "test", "{}".to_string())
                .await;
        }

        let stats = manager.get_stats(SseStreamType::Inference).unwrap();

        // Generate overflow event based on stats
        let overflow_event = SseErrorEvent::overflow(stats.dropped_count, stats.lowest_id);

        match overflow_event {
            SseErrorEvent::BufferOverflow {
                dropped_count,
                oldest_available_id,
            } => {
                assert_eq!(dropped_count, 10);
                assert_eq!(oldest_available_id, 10);
            }
            _ => panic!("Expected BufferOverflow"),
        }
    }

    #[tokio::test]
    async fn test_buffer_stats_estimated_size() {
        let buffer = SseRingBuffer::new(5);

        // Push 10 events
        for _ in 0..10 {
            let id = buffer.next_id();
            let event = SseEvent::new(id, "test", "{}");
            buffer.push(event).await;
        }

        let stats = buffer.stats();

        // estimated_size = current_sequence - dropped_count = 10 - 5 = 5
        assert_eq!(stats.estimated_size(), 5);
        assert_eq!(buffer.len().await, 5);
    }

    #[tokio::test]
    async fn test_concurrent_overflow() {
        let buffer = Arc::new(SseRingBuffer::new(100));
        let mut handles = vec![];

        // Spawn 10 tasks, each pushing 50 events
        for _ in 0..10 {
            let buf = Arc::clone(&buffer);
            handles.push(tokio::spawn(async move {
                for _ in 0..50 {
                    let id = buf.next_id();
                    let event = SseEvent::new(id, "test", "{}");
                    buf.push(event).await;
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // 500 events created, buffer holds 100
        let stats = buffer.stats();
        assert_eq!(stats.current_sequence, 500);
        assert_eq!(stats.dropped_count, 400);
        assert_eq!(buffer.len().await, 100);
    }

    #[tokio::test]
    async fn test_no_overflow_when_within_capacity() {
        let buffer = SseRingBuffer::new(100);

        // Push 50 events (within capacity)
        for _ in 0..50 {
            let id = buffer.next_id();
            let event = SseEvent::new(id, "test", "{}");
            buffer.push(event).await;
        }

        let stats = buffer.stats();
        assert_eq!(stats.dropped_count, 0);
        assert_eq!(buffer.len().await, 50);
    }
}

// =============================================================================
// Stream Disconnect Event Handling Tests
// =============================================================================

mod disconnect_tests {
    use super::*;

    #[test]
    fn test_disconnect_event_creation() {
        let event = SseErrorEvent::disconnected(500, "client timeout");

        match event {
            SseErrorEvent::StreamDisconnected {
                last_event_id,
                reason,
                reconnect_hint_ms,
            } => {
                assert_eq!(last_event_id, 500);
                assert_eq!(reason, "client timeout");
                assert_eq!(reconnect_hint_ms, 3000); // Default value
            }
            _ => panic!("Expected StreamDisconnected"),
        }
    }

    #[test]
    fn test_disconnect_event_various_reasons() {
        let reasons = vec![
            "server shutdown",
            "maintenance mode",
            "rate limit exceeded",
            "authentication expired",
            "connection idle timeout",
        ];

        for reason in reasons {
            let event = SseErrorEvent::disconnected(100, reason);
            match event {
                SseErrorEvent::StreamDisconnected {
                    reason: r,
                    reconnect_hint_ms,
                    ..
                } => {
                    assert_eq!(r, reason);
                    assert_eq!(reconnect_hint_ms, 3000);
                }
                _ => panic!("Expected StreamDisconnected"),
            }
        }
    }

    #[test]
    fn test_disconnect_event_to_sse_format() {
        let event = SseErrorEvent::disconnected(250, "graceful shutdown");
        let sse_event = event.to_sse_event(1);

        assert_eq!(sse_event.id, 1);
        assert_eq!(sse_event.event_type, "error");

        // Verify data can be parsed back
        let data: SseErrorEvent = serde_json::from_str(&sse_event.data).unwrap();
        match data {
            SseErrorEvent::StreamDisconnected {
                last_event_id,
                reason,
                ..
            } => {
                assert_eq!(last_event_id, 250);
                assert_eq!(reason, "graceful shutdown");
            }
            _ => panic!("Expected StreamDisconnected"),
        }
    }

    #[tokio::test]
    async fn test_disconnect_with_replay_info() {
        let manager = SseEventManager::with_capacity(100);

        // Create some events
        for i in 0..10 {
            manager
                .create_event(
                    SseStreamType::Dashboard,
                    "metrics",
                    format!("{{\"value\":{}}}", i),
                )
                .await;
        }

        // Simulate disconnect at event 5
        let disconnect_event = SseErrorEvent::disconnected(5, "network error");

        // Client should be able to reconnect and replay from 5
        let replay = manager.get_replay_events(SseStreamType::Dashboard, 5).await;
        assert_eq!(replay.len(), 4); // Events 6, 7, 8, 9

        // No gap should exist
        assert!(!manager.has_gap(SseStreamType::Dashboard, 5));

        // Disconnect event contains correct last_event_id
        match disconnect_event {
            SseErrorEvent::StreamDisconnected { last_event_id, .. } => {
                assert_eq!(last_event_id, 5);
            }
            _ => panic!("Expected StreamDisconnected"),
        }
    }

    #[tokio::test]
    async fn test_disconnect_with_gap_scenario() {
        let manager = SseEventManager::with_capacity(5);

        // Create 15 events
        for _ in 0..15 {
            manager
                .create_event(SseStreamType::Activity, "test", "{}".to_string())
                .await;
        }

        // Client disconnected at event 3 (now there's a gap)
        let disconnect_id = 3u64;

        // After reconnect, there should be a gap
        assert!(manager.has_gap(SseStreamType::Activity, disconnect_id));

        // Create gap detection event
        let stats = manager.get_stats(SseStreamType::Activity).unwrap();
        let gap_event = SseErrorEvent::gap_detected(
            disconnect_id,
            stats.lowest_id,
            stats.lowest_id - disconnect_id - 1,
            EventGapRecoveryHint::RefetchFullState,
        );

        match gap_event {
            SseErrorEvent::EventGapDetected {
                client_last_id,
                server_oldest_id,
                events_lost,
                recovery_hint,
            } => {
                assert_eq!(client_last_id, 3);
                assert_eq!(server_oldest_id, 10);
                assert_eq!(events_lost, 6); // Events 4-9 were lost
                assert!(matches!(
                    recovery_hint,
                    EventGapRecoveryHint::RefetchFullState
                ));
            }
            _ => panic!("Expected EventGapDetected"),
        }
    }

    #[test]
    fn test_disconnect_event_json_structure() {
        let event = SseErrorEvent::disconnected(999, "test reason");
        let json = serde_json::to_string_pretty(&event).unwrap();

        // Verify expected JSON structure for client consumption
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "stream_disconnected");
        assert_eq!(parsed["last_event_id"], 999);
        assert_eq!(parsed["reason"], "test reason");
        assert_eq!(parsed["reconnect_hint_ms"], 3000);
    }
}

// =============================================================================
// Integration Tests - End-to-End Scenarios
// =============================================================================

mod integration_tests {
    use super::*;
    use axum::http::HeaderMap;

    #[tokio::test]
    async fn test_full_reconnection_flow_with_error_events() {
        let manager = Arc::new(SseEventManager::with_capacity(10));

        // Phase 1: Initial connection - client receives events 0-4
        for i in 0..5 {
            manager
                .create_event(
                    SseStreamType::Training,
                    "progress",
                    format!("{{\"epoch\":{}}}", i),
                )
                .await;
        }

        // Client stores last_event_id = 4 before disconnect
        let client_last_id = 4u64;

        // Phase 2: Server continues producing events while client is disconnected
        for i in 5..15 {
            manager
                .create_event(
                    SseStreamType::Training,
                    "progress",
                    format!("{{\"epoch\":{}}}", i),
                )
                .await;
        }

        // Phase 3: Client reconnects with Last-Event-ID: 4
        let mut headers = HeaderMap::new();
        headers.insert("Last-Event-ID", "4".parse().unwrap());

        let last_id = SseEventManager::parse_last_event_id(&headers).unwrap();
        assert_eq!(last_id, client_last_id);

        // Phase 4: Check for gap and generate appropriate events
        let has_gap = manager.has_gap(SseStreamType::Training, last_id);

        if has_gap {
            // Some events were lost - notify client
            let result = manager
                .get_replay_with_analysis(SseStreamType::Training, last_id)
                .await;

            let gap_event = SseErrorEvent::gap_detected(
                last_id,
                result.events.first().map(|e| e.id).unwrap_or(0),
                result.dropped_count,
                EventGapRecoveryHint::RefetchFullState,
            );

            // Verify gap event
            match gap_event {
                SseErrorEvent::EventGapDetected {
                    client_last_id: cid,
                    events_lost,
                    ..
                } => {
                    assert_eq!(cid, 4);
                    assert!(events_lost > 0);
                }
                _ => panic!("Expected EventGapDetected"),
            }
        }

        // Phase 5: Get available replay events
        let replay = manager
            .get_replay_events(SseStreamType::Training, last_id)
            .await;

        // Verify replay contains available events in order
        for window in replay.windows(2) {
            assert!(
                window[1].id > window[0].id,
                "Events must be monotonically increasing"
            );
        }
    }

    #[tokio::test]
    async fn test_heartbeat_integration() {
        let manager = SseEventManager::new();

        // Create some events
        for _ in 0..5 {
            manager
                .create_event(SseStreamType::SystemMetrics, "metrics", "{}".to_string())
                .await;
        }

        // Get current state for heartbeat
        let stats = manager.get_stats(SseStreamType::SystemMetrics).unwrap();
        let heartbeat = SseErrorEvent::heartbeat(stats.current_sequence);

        match heartbeat {
            SseErrorEvent::Heartbeat {
                current_id,
                timestamp_ms,
            } => {
                assert_eq!(current_id, 5); // Next sequence after 5 events
                assert!(timestamp_ms > 0);
            }
            _ => panic!("Expected Heartbeat"),
        }

        // Convert to SSE event for transmission
        let sse_event = heartbeat.to_sse_event(100);
        assert_eq!(sse_event.event_type, "error"); // Heartbeats use error event type
        assert!(sse_event.data.contains("heartbeat"));
    }

    #[tokio::test]
    async fn test_multiple_stream_types_isolation() {
        let manager = SseEventManager::with_capacity(5);

        // Create events in different streams
        for _ in 0..10 {
            manager
                .create_event(SseStreamType::Alerts, "alert", "{}".to_string())
                .await;
            manager
                .create_event(SseStreamType::Training, "train", "{}".to_string())
                .await;
        }

        // Gap detection should be per-stream
        assert!(manager.has_gap(SseStreamType::Alerts, 2));
        assert!(manager.has_gap(SseStreamType::Training, 2));

        // Stats should be separate
        let alert_stats = manager.get_stats(SseStreamType::Alerts).unwrap();
        let training_stats = manager.get_stats(SseStreamType::Training).unwrap();

        assert_eq!(alert_stats.dropped_count, 5);
        assert_eq!(training_stats.dropped_count, 5);
    }

    #[tokio::test]
    async fn test_error_event_chain() {
        // Simulate a sequence of error events for a troubled connection
        let events = vec![
            SseErrorEvent::heartbeat(100),
            SseErrorEvent::overflow(10, 110),
            SseErrorEvent::gap_detected(90, 110, 20, EventGapRecoveryHint::ContinueWithGap),
            SseErrorEvent::disconnected(120, "timeout"),
        ];

        // Convert all to SSE events and verify they can be serialized
        for (idx, event) in events.iter().enumerate() {
            let sse = event.to_sse_event(idx as u64);
            assert_eq!(sse.id, idx as u64);
            assert_eq!(sse.event_type, "error");

            // Verify data is valid JSON
            let _: SseErrorEvent =
                serde_json::from_str(&sse.data).expect("SSE event data should be valid JSON");
        }
    }

    #[tokio::test]
    async fn test_recovery_hints_for_different_scenarios() {
        // Test that appropriate recovery hints are generated for different scenarios

        // Scenario 1: Complete state loss - suggest full refetch
        let gap_event1 =
            SseErrorEvent::gap_detected(0, 1000, 1000, EventGapRecoveryHint::RefetchFullState);

        // Scenario 2: Minor gap - can continue
        let gap_event2 =
            SseErrorEvent::gap_detected(998, 1000, 2, EventGapRecoveryHint::ContinueWithGap);

        // Scenario 3: Specific resource affected
        let gap_event3 = SseErrorEvent::gap_detected(
            500,
            600,
            100,
            EventGapRecoveryHint::RefetchResource {
                resource_type: "adapter".to_string(),
                resource_id: "my-adapter-v1".to_string(),
            },
        );

        // All should serialize correctly
        for event in [gap_event1, gap_event2, gap_event3] {
            let json = serde_json::to_string(&event).unwrap();
            let _: SseErrorEvent = serde_json::from_str(&json).unwrap();
        }
    }

    #[tokio::test]
    async fn test_axum_event_conversion() {
        let manager = SseEventManager::new();

        // Create an event
        let event = manager
            .create_event(
                SseStreamType::Inference,
                "token",
                r#"{"text":"Hello"}"#.to_string(),
            )
            .await;

        // Convert to Axum SSE event
        let axum_event = SseEventManager::to_axum_event(&event);

        // Verify it doesn't panic and produces valid output
        let _ = format!("{:?}", axum_event);
    }

    #[tokio::test]
    async fn test_clear_and_stats_reset() {
        let manager = SseEventManager::with_capacity(5);

        // Create events with overflow
        for _ in 0..10 {
            manager
                .create_event(SseStreamType::GitProgress, "test", "{}".to_string())
                .await;
        }

        // Verify overflow occurred
        let stats_before = manager.get_stats(SseStreamType::GitProgress).unwrap();
        assert_eq!(stats_before.dropped_count, 5);

        // Clear the buffer
        manager.clear(SseStreamType::GitProgress).await;

        // Verify cleared state
        let replay = manager
            .get_replay_events(SseStreamType::GitProgress, 0)
            .await;
        assert!(replay.is_empty());

        // Create new event - sequence should continue
        let new_event = manager
            .create_event(SseStreamType::GitProgress, "test", "{}".to_string())
            .await;
        assert_eq!(new_event.id, 10); // Sequence preserved
    }
}

// =============================================================================
// Edge Cases and Error Handling Tests
// =============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn test_empty_reason_disconnect() {
        let event = SseErrorEvent::disconnected(0, "");
        match event {
            SseErrorEvent::StreamDisconnected { reason, .. } => {
                assert_eq!(reason, "");
            }
            _ => panic!("Expected StreamDisconnected"),
        }
    }

    #[test]
    fn test_max_values() {
        let event = SseErrorEvent::disconnected(u64::MAX, "max test");
        match event {
            SseErrorEvent::StreamDisconnected { last_event_id, .. } => {
                assert_eq!(last_event_id, u64::MAX);
            }
            _ => panic!("Expected StreamDisconnected"),
        }

        // Verify serialization handles max values
        let json = serde_json::to_string(&event).unwrap();
        let _: SseErrorEvent = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_special_characters_in_reason() {
        let reasons = vec![
            "error with \"quotes\"",
            "error with\nnewline",
            "error with\ttab",
            "error with unicode: \u{1F600}",
            "error with backslash: \\path\\to\\file",
        ];

        for reason in reasons {
            let event = SseErrorEvent::disconnected(1, reason);
            let json = serde_json::to_string(&event).unwrap();
            let deserialized: SseErrorEvent = serde_json::from_str(&json).unwrap();

            match deserialized {
                SseErrorEvent::StreamDisconnected { reason: r, .. } => {
                    assert_eq!(r, reason);
                }
                _ => panic!("Expected StreamDisconnected"),
            }
        }
    }

    #[tokio::test]
    async fn test_zero_capacity_buffer() {
        // Edge case: buffer with capacity 1
        let buffer = SseRingBuffer::new(1);

        // Push 5 events
        for i in 0..5 {
            let id = buffer.next_id();
            let event = SseEvent::new(id, "test", format!("{}", i));
            buffer.push(event).await;
        }

        // Only last event should remain
        let all = buffer.get_all().await;
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, 4);
        assert_eq!(buffer.stats().dropped_count, 4);
    }

    #[tokio::test]
    async fn test_replay_from_future_id() {
        let manager = SseEventManager::new();

        // Create 5 events (IDs 0-4)
        for _ in 0..5 {
            manager
                .create_event(SseStreamType::Alerts, "test", "{}".to_string())
                .await;
        }

        // Replay from ID 100 (future) should return empty
        let replay = manager.get_replay_events(SseStreamType::Alerts, 100).await;
        assert!(replay.is_empty());

        // No gap should be detected for future IDs
        assert!(!manager.has_gap(SseStreamType::Alerts, 100));
    }

    #[tokio::test]
    async fn test_get_stats_nonexistent_stream() {
        let manager = SseEventManager::new();

        // Get stats for stream that has no events yet
        let stats = manager.get_stats(SseStreamType::BootProgress);
        assert!(stats.is_none());
    }

    #[test]
    fn test_buffer_stats_default_values() {
        let stats = BufferStats {
            capacity: 100,
            current_sequence: 0,
            dropped_count: 0,
            lowest_id: 0,
        };

        assert_eq!(stats.estimated_size(), 0);
    }
}
