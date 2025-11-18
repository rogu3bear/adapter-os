//! Integration tests for the RouterDecision telemetry pipeline

use adapteros_db::Db;
use adapteros_telemetry::events::RouterCandidate;
use adapteros_telemetry::{RouterDecisionEvent, RouterDecisionWriter};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn test_router_decision_telemetry_pipeline() {
    // Create a test database
    let db = Db::new_in_memory().await.expect("Failed to create test database");

    // Create a telemetry writer
    let (writer, mut receiver) = RouterDecisionWriter::new();

    // Create a test router decision event
    let event = RouterDecisionEvent {
        step: 42,
        input_token_id: Some(12345),
        candidate_adapters: vec![
            RouterCandidate {
                adapter_idx: 0,
                raw_score: 0.8,
                gate_q15: 32767,
            },
            RouterCandidate {
                adapter_idx: 1,
                raw_score: 0.6,
                gate_q15: 24576,
            },
        ],
        entropy: 0.85,
        tau: 1.0,
        entropy_floor: 0.01,
        stack_hash: Some("test-stack-hash".to_string()),
        stack_id: Some("test-stack-id".to_string()),
        stack_version: Some(1),
    };

    // Emit the event (should not block)
    let emit_result = writer.emit(event.clone());
    assert!(emit_result.is_ok(), "Failed to emit telemetry event");

    // Receive the event from the channel
    let received_event = receiver.recv().await.expect("Failed to receive event");
    assert_eq!(received_event.step, event.step);
    assert_eq!(received_event.entropy, event.entropy);
    assert_eq!(received_event.candidate_adapters.len(), 2);

    // Test writer statistics
    assert_eq!(writer.total_count(), 1);
    assert_eq!(writer.dropped_count(), 0);
    assert_eq!(writer.drop_rate(), 0.0);
}

#[tokio::test]
async fn test_router_decision_overflow_handling() {
    // Create a writer with very small capacity
    let (writer, _receiver) = RouterDecisionWriter::with_capacity(1);

    // Create test event
    let event = RouterDecisionEvent {
        step: 1,
        input_token_id: None,
        candidate_adapters: vec![RouterCandidate {
            adapter_idx: 0,
            raw_score: 0.5,
            gate_q15: 16384,
        }],
        entropy: 0.7,
        tau: 0.8,
        entropy_floor: 0.01,
        stack_hash: None,
        stack_id: None,
        stack_version: None,
    };

    // Fill the channel
    writer.emit(event.clone()).expect("First emit should succeed");

    // This should fail due to channel being full
    let overflow_result = writer.emit(event);
    assert!(overflow_result.is_err(), "Overflow should cause emit to fail");

    // Check statistics
    assert_eq!(writer.total_count(), 2);
    assert_eq!(writer.dropped_count(), 1);
    assert_eq!(writer.drop_rate(), 0.5);
}

#[tokio::test]
async fn test_telemetry_pipeline_determinism() {
    // Test that identical events produce identical results
    let (writer1, mut receiver1) = RouterDecisionWriter::new();
    let (writer2, mut receiver2) = RouterDecisionWriter::new();

    let event = RouterDecisionEvent {
        step: 100,
        input_token_id: Some(999),
        candidate_adapters: vec![
            RouterCandidate {
                adapter_idx: 5,
                raw_score: 0.9,
                gate_q15: 30000,
            },
        ],
        entropy: 0.95,
        tau: 1.2,
        entropy_floor: 0.001,
        stack_hash: Some("deterministic-hash".to_string()),
        stack_id: Some("deterministic-stack".to_string()),
        stack_version: Some(42),
    };

    // Emit same event to both writers
    writer1.emit(event.clone()).expect("Writer1 emit failed");
    writer2.emit(event.clone()).expect("Writer2 emit failed");

    // Receive events
    let received1 = receiver1.recv().await.expect("Failed to receive from writer1");
    let received2 = receiver2.recv().await.expect("Failed to receive from writer2");

    // Events should be identical
    assert_eq!(received1.step, received2.step);
    assert_eq!(received1.entropy, received2.entropy);
    assert_eq!(received1.stack_hash, received2.stack_hash);
    assert_eq!(received1.stack_id, received2.stack_id);
    assert_eq!(received1.stack_version, received2.stack_version);
}
