//! Event sequence determinism tests for AdapterOS
//!
//! Verifies that event sequences maintain deterministic ordering and content,
//! ensuring replay capability and audit trail integrity.

use super::utils::*;
use adapteros_telemetry::{TelemetryEvent, EventSequence};

/// Test basic event sequence construction
#[test]
fn test_event_sequence_construction() {
    let mut comparator = EventSequenceComparator::new();

    // Create a simple event sequence
    let events = vec![
        TelemetryEvent {
            event_type: "task_start".to_string(),
            timestamp: 1000,
            data: serde_json::json!({"task_id": "task1"}),
        },
        TelemetryEvent {
            event_type: "task_complete".to_string(),
            timestamp: 1005,
            data: serde_json::json!({"task_id": "task1", "result": "success"}),
        },
    ];

    let sequence = EventSequence {
        events: events.clone(),
        metadata: std::collections::HashMap::new(),
    };

    comparator.add_sequence("test_seq", sequence);

    // Verify sequence was stored
    assert!(comparator.sequences.contains_key("test_seq"));
    assert_eq!(comparator.sequences["test_seq"].events.len(), 2);
}

/// Test event sequence determinism
#[test]
fn test_event_sequence_determinism() {
    let mut comparator = EventSequenceComparator::new();

    // Create identical event sequences
    let events1 = vec![
        TelemetryEvent {
            event_type: "computation_start".to_string(),
            timestamp: 1000,
            data: serde_json::json!({"operation": "matrix_multiply"}),
        },
        TelemetryEvent {
            event_type: "computation_complete".to_string(),
            timestamp: 1010,
            data: serde_json::json!({"operation": "matrix_multiply", "flops": 1000000}),
        },
    ];

    let events2 = vec![
        TelemetryEvent {
            event_type: "computation_start".to_string(),
            timestamp: 1000,
            data: serde_json::json!({"operation": "matrix_multiply"}),
        },
        TelemetryEvent {
            event_type: "computation_complete".to_string(),
            timestamp: 1010,
            data: serde_json::json!({"operation": "matrix_multiply", "flops": 1000000}),
        },
    ];

    let seq1 = EventSequence {
        events: events1,
        metadata: std::collections::HashMap::new(),
    };

    let seq2 = EventSequence {
        events: events2,
        metadata: std::collections::HashMap::new(),
    };

    comparator.add_sequence("seq1", seq1);
    comparator.add_sequence("seq2", seq2);

    // Verify sequences are identical
    comparator.compare_sequences("seq1", "seq2").unwrap();
}

/// Test event sequence ordering
#[test]
fn test_event_sequence_ordering() {
    let mut comparator = EventSequenceComparator::new();

    // Create sequences with different orderings
    let events_ordered = vec![
        TelemetryEvent {
            event_type: "start".to_string(),
            timestamp: 1000,
            data: serde_json::json!({"id": 1}),
        },
        TelemetryEvent {
            event_type: "middle".to_string(),
            timestamp: 1005,
            data: serde_json::json!({"id": 2}),
        },
        TelemetryEvent {
            event_type: "end".to_string(),
            timestamp: 1010,
            data: serde_json::json!({"id": 3}),
        },
    ];

    let events_reordered = vec![
        TelemetryEvent {
            event_type: "middle".to_string(),
            timestamp: 1005,
            data: serde_json::json!({"id": 2}),
        },
        TelemetryEvent {
            event_type: "start".to_string(),
            timestamp: 1000,
            data: serde_json::json!({"id": 1}),
        },
        TelemetryEvent {
            event_type: "end".to_string(),
            timestamp: 1010,
            data: serde_json::json!({"id": 3}),
        },
    ];

    let seq_ordered = EventSequence {
        events: events_ordered,
        metadata: std::collections::HashMap::new(),
    };

    let seq_reordered = EventSequence {
        events: events_reordered,
        metadata: std::collections::HashMap::new(),
    };

    comparator.add_sequence("ordered", seq_ordered);
    comparator.add_sequence("reordered", seq_reordered);

    // Verify sequences are different due to ordering
    assert!(comparator.compare_sequences("ordered", "reordered").is_err());
}

/// Test event sequence timestamps
#[test]
fn test_event_sequence_timestamps() {
    let mut comparator = EventSequenceComparator::new();

    // Test deterministic timestamp generation
    let events1 = vec![
        TelemetryEvent {
            event_type: "logical_time".to_string(),
            timestamp: 1000, // Logical timestamp
            data: serde_json::json!({"tick": 1}),
        },
        TelemetryEvent {
            event_type: "logical_time".to_string(),
            timestamp: 1001,
            data: serde_json::json!({"tick": 2}),
        },
    ];

    let events2 = vec![
        TelemetryEvent {
            event_type: "logical_time".to_string(),
            timestamp: 1000,
            data: serde_json::json!({"tick": 1}),
        },
        TelemetryEvent {
            event_type: "logical_time".to_string(),
            timestamp: 1001,
            data: serde_json::json!({"tick": 2}),
        },
    ];

    let seq1 = EventSequence {
        events: events1,
        metadata: std::collections::HashMap::new(),
    };

    let seq2 = EventSequence {
        events: events2,
        metadata: std::collections::HashMap::new(),
    };

    comparator.add_sequence("timestamps1", seq1);
    comparator.add_sequence("timestamps2", seq2);

    // Timestamps should be identical
    comparator.compare_sequences("timestamps1", "timestamps2").unwrap();
}

/// Test event sequence metadata
#[test]
fn test_event_sequence_metadata() {
    let mut comparator = EventSequenceComparator::new();

    // Test metadata consistency
    let mut metadata1 = std::collections::HashMap::new();
    metadata1.insert("version".to_string(), "1.0".to_string());
    metadata1.insert("platform".to_string(), "test".to_string());

    let mut metadata2 = std::collections::HashMap::new();
    metadata2.insert("version".to_string(), "1.0".to_string());
    metadata2.insert("platform".to_string(), "test".to_string());

    let seq1 = EventSequence {
        events: vec![],
        metadata: metadata1,
    };

    let seq2 = EventSequence {
        events: vec![],
        metadata: metadata2,
    };

    comparator.add_sequence("meta1", seq1);
    comparator.add_sequence("meta2", seq2);

    // Metadata should be identical
    assert_eq!(comparator.sequences["meta1"].metadata, comparator.sequences["meta2"].metadata);
}

/// Test event sequence serialization
#[test]
fn test_event_sequence_serialization() {
    let events = vec![
        TelemetryEvent {
            event_type: "test_event".to_string(),
            timestamp: 1234567890,
            data: serde_json::json!({"key": "value", "number": 42}),
        },
    ];

    let sequence = EventSequence {
        events,
        metadata: std::collections::HashMap::new(),
    };

    // Serialize to JSON
    let serialized = serde_json::to_string(&sequence).unwrap();

    // Deserialize back
    let deserialized: EventSequence = serde_json::from_str(&serialized).unwrap();

    // Verify round-trip consistency
    assert_eq!(sequence.events.len(), deserialized.events.len());
    assert_eq!(sequence.metadata, deserialized.metadata);

    for (orig, deser) in sequence.events.iter().zip(deserialized.events.iter()) {
        assert_eq!(orig.event_type, deser.event_type);
        assert_eq!(orig.timestamp, deser.timestamp);
        assert_eq!(orig.data, deser.data);
    }
}

/// Test event sequence filtering
#[test]
fn test_event_sequence_filtering() {
    let events = vec![
        TelemetryEvent {
            event_type: "task_start".to_string(),
            timestamp: 1000,
            data: serde_json::json!({"task": "A"}),
        },
        TelemetryEvent {
            event_type: "network_request".to_string(),
            timestamp: 1001,
            data: serde_json::json!({"url": "http://example.com"}),
        },
        TelemetryEvent {
            event_type: "task_complete".to_string(),
            timestamp: 1002,
            data: serde_json::json!({"task": "A"}),
        },
        TelemetryEvent {
            event_type: "task_start".to_string(),
            timestamp: 1003,
            data: serde_json::json!({"task": "B"}),
        },
    ];

    let sequence = EventSequence {
        events,
        metadata: std::collections::HashMap::new(),
    };

    // Filter task-related events
    let task_events: Vec<&TelemetryEvent> = sequence.events.iter()
        .filter(|e| e.event_type.starts_with("task_"))
        .collect();

    assert_eq!(task_events.len(), 3);

    // Filter by timestamp range
    let time_filtered: Vec<&TelemetryEvent> = sequence.events.iter()
        .filter(|e| e.timestamp >= 1001 && e.timestamp <= 1002)
        .collect();

    assert_eq!(time_filtered.len(), 2);
}

/// Test event sequence replay capability
#[test]
fn test_event_sequence_replay() {
    // Test that event sequences can be replayed to reconstruct execution state

    let events = vec![
        TelemetryEvent {
            event_type: "state_change".to_string(),
            timestamp: 1000,
            data: serde_json::json!({"state": "initializing"}),
        },
        TelemetryEvent {
            event_type: "state_change".to_string(),
            timestamp: 1001,
            data: serde_json::json!({"state": "running"}),
        },
        TelemetryEvent {
            event_type: "state_change".to_string(),
            timestamp: 1002,
            data: serde_json::json!({"state": "completed"}),
        },
    ];

    let sequence = EventSequence {
        events,
        metadata: std::collections::HashMap::new(),
    };

    // Simulate replay by applying state changes in order
    let mut current_state = "unknown".to_string();

    for event in &sequence.events {
        if let Some(state) = event.data.get("state") {
            current_state = state.as_str().unwrap().to_string();
        }
    }

    assert_eq!(current_state, "completed");

    // Verify replay is deterministic
    let mut state2 = "unknown".to_string();
    for event in &sequence.events {
        if let Some(state) = event.data.get("state") {
            state2 = state.as_str().unwrap().to_string();
        }
    }

    assert_eq!(current_state, state2);
}

/// Test concurrent event sequence handling
#[tokio::test]
async fn test_concurrent_event_sequences() {
    let mut comparator = EventSequenceComparator::new();

    // Simulate concurrent operations producing event sequences
    let seq1_future = tokio::spawn(async {
        let events = vec![
            TelemetryEvent {
                event_type: "operation_a".to_string(),
                timestamp: 1000,
                data: serde_json::json!({"step": 1}),
            },
            TelemetryEvent {
                event_type: "operation_a".to_string(),
                timestamp: 1001,
                data: serde_json::json!({"step": 2}),
            },
        ];
        EventSequence {
            events,
            metadata: std::collections::HashMap::new(),
        }
    });

    let seq2_future = tokio::spawn(async {
        let events = vec![
            TelemetryEvent {
                event_type: "operation_a".to_string(),
                timestamp: 1000,
                data: serde_json::json!({"step": 1}),
            },
            TelemetryEvent {
                event_type: "operation_a".to_string(),
                timestamp: 1001,
                data: serde_json::json!({"step": 2}),
            },
        ];
        EventSequence {
            events,
            metadata: std::collections::HashMap::new(),
        }
    });

    let seq1 = seq1_future.await.unwrap();
    let seq2 = seq2_future.await.unwrap();

    comparator.add_sequence("concurrent1", seq1);
    comparator.add_sequence("concurrent2", seq2);

    // Concurrent sequences should be identical
    comparator.compare_sequences("concurrent1", "concurrent2").unwrap();
}

/// Test event sequence performance
#[test]
fn test_event_sequence_performance() {
    let mut comparator = EventSequenceComparator::new();

    // Create a large event sequence
    let mut events = Vec::new();
    for i in 0..10000 {
        events.push(TelemetryEvent {
            event_type: format!("event_{}", i % 10),
            timestamp: 1000 + i as u64,
            data: serde_json::json!({"index": i}),
        });
    }

    let sequence = EventSequence {
        events,
        metadata: std::collections::HashMap::new(),
    };

    let start = std::time::Instant::now();
    comparator.add_sequence("large_seq", sequence);
    let add_duration = start.elapsed();

    let start = std::time::Instant::now();
    let cloned = comparator.sequences.get("large_seq").unwrap().clone();
    let clone_duration = start.elapsed();

    // Performance should be reasonable
    assert!(add_duration < std::time::Duration::from_millis(100),
            "Adding large sequence should be fast: {:?}", add_duration);
    assert!(clone_duration < std::time::Duration::from_millis(50),
            "Cloning large sequence should be fast: {:?}", clone_duration);

    // Verify sequence integrity
    assert_eq!(comparator.sequences["large_seq"].events.len(), 10000);
    assert_eq!(cloned.events.len(), 10000);
}