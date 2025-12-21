//! Determinism tests for telemetry pipeline

use adapteros_telemetry::events::RouterCandidate;
use adapteros_telemetry::{RouterDecisionEvent, RouterDecisionWriter};

#[test]
fn test_router_decision_determinism() {
    // Test that telemetry events maintain deterministic behavior
    let (writer, mut receiver) = RouterDecisionWriter::new();

    // Create identical events
    let event1 = RouterDecisionEvent {
        step: 50,
        input_token_id: Some(1000),
        candidate_adapters: vec![
            RouterCandidate {
                adapter_idx: 0,
                raw_score: 0.75,
                gate_q15: 24576,
            },
            RouterCandidate {
                adapter_idx: 1,
                raw_score: 0.65,
                gate_q15: 19661,
            },
        ],
        entropy: 0.8,
        tau: 1.1,
        entropy_floor: 0.005,
        stack_hash: Some("deterministic-stack-hash-123".to_string()),
        stack_id: Some("deterministic-stack-id".to_string()),
        stack_version: Some(5),
    };

    let event2 = RouterDecisionEvent {
        step: 50, // Same step
        input_token_id: Some(1000), // Same input
        candidate_adapters: vec![
            RouterCandidate {
                adapter_idx: 0,
                raw_score: 0.75, // Same scores
                gate_q15: 24576,
            },
            RouterCandidate {
                adapter_idx: 1,
                raw_score: 0.65,
                gate_q15: 19661,
            },
        ],
        entropy: 0.8, // Same entropy
        tau: 1.1, // Same tau
        entropy_floor: 0.005, // Same floor
        stack_hash: Some("deterministic-stack-hash-123".to_string()), // Same hash
        stack_id: Some("deterministic-stack-id".to_string()), // Same ID
        stack_version: Some(5), // Same version
    };

    // Emit both events
    writer.emit(event1).expect("Failed to emit event1");
    writer.emit(event2).expect("Failed to emit event2");

    // Receive and compare
    let received1 = receiver.blocking_recv().expect("Failed to receive event1");
    let received2 = receiver.blocking_recv().expect("Failed to receive event2");

    // All fields should be identical
    assert_eq!(received1.step, received2.step);
    assert_eq!(received1.input_token_id, received2.input_token_id);
    assert_eq!(received1.entropy, received2.entropy);
    assert_eq!(received1.tau, received2.tau);
    assert_eq!(received1.entropy_floor, received2.entropy_floor);
    assert_eq!(received1.stack_hash, received2.stack_hash);
    assert_eq!(received1.stack_id, received2.stack_id);
    assert_eq!(received1.stack_version, received2.stack_version);

    // Candidate adapters should be identical
    assert_eq!(received1.candidate_adapters.len(), received2.candidate_adapters.len());
    for (c1, c2) in received1.candidate_adapters.iter().zip(received2.candidate_adapters.iter()) {
        assert_eq!(c1.adapter_idx, c2.adapter_idx);
        assert_eq!(c1.raw_score, c2.raw_score);
        assert_eq!(c1.gate_q15, c2.gate_q15);
    }
}

#[test]
fn test_telemetry_event_ordering() {
    // Test that events maintain ordering in the channel
    let (writer, mut receiver) = RouterDecisionWriter::new();

    let events: Vec<RouterDecisionEvent> = (0..10).map(|i| RouterDecisionEvent {
        step: i,
        input_token_id: Some(i * 100),
        candidate_adapters: vec![RouterCandidate {
            adapter_idx: i as u16,
            raw_score: 0.5 + (i as f32 * 0.05),
            gate_q15: (16384 + i * 1000) as i16,
        }],
        entropy: 0.7 + (i as f32 * 0.02),
        tau: 1.0,
        entropy_floor: 0.01,
        stack_hash: Some(format!("stack-hash-{}", i)),
        stack_id: Some(format!("stack-id-{}", i)),
        stack_version: Some(i as i64),
    }).collect();

    // Emit events in order
    for event in &events {
        writer.emit(event.clone()).expect("Failed to emit event");
    }

    // Receive events and verify ordering
    for expected in &events {
        let received = receiver.blocking_recv().expect("Failed to receive event");
        assert_eq!(received.step, expected.step);
        assert_eq!(received.input_token_id, expected.input_token_id);
        assert_eq!(received.stack_hash, expected.stack_hash);
    }
}

#[test]
fn test_telemetry_statistics_accuracy() {
    // Test that writer statistics are accurate
    let (writer, mut receiver) = RouterDecisionWriter::with_capacity(3);

    let event = RouterDecisionEvent {
        step: 1,
        input_token_id: None,
        candidate_adapters: vec![RouterCandidate {
            adapter_idx: 0,
            raw_score: 0.5,
            gate_q15: 16384,
        }],
        entropy: 0.6,
        tau: 0.9,
        entropy_floor: 0.01,
        stack_hash: None,
        stack_id: None,
        stack_version: None,
    };

    // Emit 2 events successfully
    writer.emit(event.clone()).expect("Emit 1 failed");
    writer.emit(event.clone()).expect("Emit 2 failed");

    // Fill the channel (capacity 3, so this should succeed)
    writer.emit(event.clone()).expect("Emit 3 failed");

    // This should fail (channel full)
    let result = writer.emit(event.clone());
    assert!(result.is_err());

    // Drain one event to make room
    let _ = receiver.blocking_recv();

    // This should succeed now
    writer.emit(event).expect("Emit after drain failed");

    // Check final statistics
    assert_eq!(writer.total_count(), 5);
    assert_eq!(writer.dropped_count(), 1);
    assert_eq!(writer.drop_rate(), 0.2); // 1 dropped out of 5 total
}
