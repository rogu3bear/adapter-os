#![cfg(all(test, feature = "extended-tests"))]
//! Event sequence determinism tests for AdapterOS
//!
//! Verifies that event sequences maintain deterministic ordering and content,
//! ensuring replay capability and audit trail integrity.

use super::utils::{EventSequence, EventSequenceComparator, TelemetryEvent};
use std::collections::HashMap;

fn sample_events() -> Vec<TelemetryEvent> {
    vec![
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
    ]
}

#[test]
fn test_event_sequence_construction() {
    let mut comparator = EventSequenceComparator::new();
    let events = sample_events();

    let sequence = EventSequence {
        events: events.clone(),
        metadata: HashMap::new(),
    };

    comparator.add_sequence("seq1", sequence.clone());

    assert_eq!(comparator.sequences.get("seq1").unwrap(), &sequence);
    assert_eq!(sequence.events.len(), 2);
    assert_eq!(sequence.events[0].event_type, "task_start");
    assert_eq!(sequence.events[1].event_type, "task_complete");
}

#[test]
fn test_event_sequence_comparison_equal() {
    let mut comparator = EventSequenceComparator::new();
    let events_a = sample_events();
    let events_b = sample_events(); // identical ordering and data

    comparator.add_sequence(
        "a",
        EventSequence {
            events: events_a,
            metadata: HashMap::new(),
        },
    );
    comparator.add_sequence(
        "b",
        EventSequence {
            events: events_b,
            metadata: HashMap::new(),
        },
    );

    assert!(comparator.compare_sequences("a", "b").is_ok());
}

#[test]
fn test_event_sequence_comparison_mismatch() {
    let mut comparator = EventSequenceComparator::new();
    let mut events_a = sample_events();
    let mut events_b = sample_events();

    // Introduce a difference in the second event
    events_b[1].event_type = "task_failed".to_string();

    comparator.add_sequence(
        "a",
        EventSequence {
            events: events_a,
            metadata: HashMap::new(),
        },
    );
    comparator.add_sequence(
        "b",
        EventSequence {
            events: events_b,
            metadata: HashMap::new(),
        },
    );

    assert!(comparator.compare_sequences("a", "b").is_err());
}

#[test]
fn test_event_sequence_length_mismatch() {
    let mut comparator = EventSequenceComparator::new();
    let short = sample_events();
    let mut long = sample_events();
    long.push(TelemetryEvent {
        event_type: "extra".to_string(),
        timestamp: 1010,
        data: serde_json::json!({"task_id": "task1"}),
    });

    comparator.add_sequence(
        "short",
        EventSequence {
            events: short,
            metadata: HashMap::new(),
        },
    );
    comparator.add_sequence(
        "long",
        EventSequence {
            events: long,
            metadata: HashMap::new(),
        },
    );

    assert!(comparator.compare_sequences("short", "long").is_err());
}

#[test]
fn test_event_sequence_metadata() {
    let mut comparator = EventSequenceComparator::new();
    let events = sample_events();
    let mut metadata = HashMap::new();
    metadata.insert("tenant".to_string(), "tenant-a".to_string());
    metadata.insert("cpid".to_string(), "cpid-123".to_string());

    let sequence = EventSequence { events, metadata };
    comparator.add_sequence("seq", sequence.clone());

    let stored = comparator.sequences.get("seq").unwrap();
    assert_eq!(stored.metadata.get("tenant").unwrap(), "tenant-a");
    assert_eq!(stored.metadata.get("cpid").unwrap(), "cpid-123");
}

