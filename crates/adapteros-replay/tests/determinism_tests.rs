//! Tests for determinism verification in replay
//!
//! Verifies that replay produces identical outputs when executed multiple times
//! with the same inputs, and that hash verification catches divergences.

use adapteros_core::B3Hash;
use adapteros_replay::{
    compare_events_permissive, compare_traces, ComparisonResult, HashVerifier, ReplaySession,
    TolerantVerifier, VerificationMode,
};
use tempfile::tempdir;

mod test_helpers;
use test_helpers::{create_deterministic_event, create_trace_bundle_with_values};



#[test]
fn test_hash_verifier_strict_identical() {
    let hash1 = B3Hash::hash(b"test_data");
    let hash2 = B3Hash::hash(b"test_data");

    assert!(HashVerifier::verify_strict(&hash1, &hash2));
}

#[test]
fn test_hash_verifier_strict_different() {
    let hash1 = B3Hash::hash(b"test_data_1");
    let hash2 = B3Hash::hash(b"test_data_2");

    assert!(!HashVerifier::verify_strict(&hash1, &hash2));
}

#[test]
fn test_tolerant_verifier_floating_point_exact() {
    assert!(TolerantVerifier::compare_floating_point(1.0, 1.0));
    assert!(TolerantVerifier::compare_floating_point(0.0, 0.0));
    assert!(TolerantVerifier::compare_floating_point(-1.0, -1.0));
}

#[test]
fn test_tolerant_verifier_floating_point_epsilon() {
    // Values within epsilon should match
    assert!(TolerantVerifier::compare_floating_point(
        1.0,
        1.0 + 1e-10
    ));
    assert!(TolerantVerifier::compare_floating_point(
        1.0,
        1.0 - 1e-10
    ));

    // Values outside epsilon should not match
    assert!(!TolerantVerifier::compare_floating_point(1.0, 1.01));
    assert!(!TolerantVerifier::compare_floating_point(1.0, 0.99));
}

#[test]
fn test_tolerant_verifier_floating_point_special_values() {
    // NaN cases
    assert!(TolerantVerifier::compare_floating_point(
        f64::NAN,
        f64::NAN
    ));
    assert!(!TolerantVerifier::compare_floating_point(f64::NAN, 1.0));
    assert!(!TolerantVerifier::compare_floating_point(1.0, f64::NAN));

    // Infinity cases
    assert!(TolerantVerifier::compare_floating_point(
        f64::INFINITY,
        f64::INFINITY
    ));
    assert!(TolerantVerifier::compare_floating_point(
        f64::NEG_INFINITY,
        f64::NEG_INFINITY
    ));
    assert!(!TolerantVerifier::compare_floating_point(
        f64::INFINITY,
        f64::NEG_INFINITY
    ));
}

#[test]
fn test_tolerant_verifier_f32() {
    assert!(TolerantVerifier::compare_f32(1.0f32, 1.0f32));
    assert!(TolerantVerifier::compare_f32(
        1.0f32,
        1.0f32 + 1e-9f32
    ));
    assert!(!TolerantVerifier::compare_f32(1.0f32, 1.1f32));
}

#[test]
fn test_tolerant_verifier_float_arrays() {
    let arr1 = vec![1.0, 2.0, 3.0];
    let arr2 = vec![1.0, 2.0, 3.0];
    assert!(TolerantVerifier::compare_float_arrays(&arr1, &arr2));

    let arr3 = vec![1.0, 2.0, 3.0 + 1e-10];
    assert!(TolerantVerifier::compare_float_arrays(&arr1, &arr3));

    let arr4 = vec![1.0, 2.0, 4.0];
    assert!(!TolerantVerifier::compare_float_arrays(&arr1, &arr4));

    let arr5 = vec![1.0, 2.0];
    assert!(!TolerantVerifier::compare_float_arrays(&arr1, &arr5));
}

#[test]
fn test_tolerant_verifier_json_numbers() {
    let val1 = serde_json::json!(1.0);
    let val2 = serde_json::json!(1.0);
    assert!(TolerantVerifier::compare_json_values(&val1, &val2));

    let val3 = serde_json::json!(1.0 + 1e-10);
    assert!(TolerantVerifier::compare_json_values(&val1, &val3));

    let val4 = serde_json::json!(2.0);
    assert!(!TolerantVerifier::compare_json_values(&val1, &val4));
}

#[test]
fn test_tolerant_verifier_json_arrays() {
    let arr1 = serde_json::json!([1.0, 2.0, 3.0]);
    let arr2 = serde_json::json!([1.0, 2.0, 3.0]);
    assert!(TolerantVerifier::compare_json_values(&arr1, &arr2));

    let arr3 = serde_json::json!([1.0, 2.0, 3.0 + 1e-10]);
    assert!(TolerantVerifier::compare_json_values(&arr1, &arr3));

    let arr4 = serde_json::json!([1.0, 2.0, 4.0]);
    assert!(!TolerantVerifier::compare_json_values(&arr1, &arr4));
}

#[test]
fn test_tolerant_verifier_json_objects() {
    let obj1 = serde_json::json!({"a": 1.0, "b": 2.0});
    let obj2 = serde_json::json!({"a": 1.0, "b": 2.0});
    assert!(TolerantVerifier::compare_json_values(&obj1, &obj2));

    let obj3 = serde_json::json!({"a": 1.0 + 1e-10, "b": 2.0});
    assert!(TolerantVerifier::compare_json_values(&obj1, &obj3));

    let obj4 = serde_json::json!({"a": 1.0, "b": 3.0});
    assert!(!TolerantVerifier::compare_json_values(&obj1, &obj4));

    let obj5 = serde_json::json!({"a": 1.0});
    assert!(!TolerantVerifier::compare_json_values(&obj1, &obj5));
}

#[test]
fn test_compare_events_permissive_identical() {
    let event1 = create_deterministic_event(0, "test", 42);
    let event2 = create_deterministic_event(0, "test", 42);

    assert!(compare_events_permissive(&event1, &event2));
}

#[test]
fn test_compare_events_permissive_different_tick() {
    let event1 = create_deterministic_event(0, "test", 42);
    let event2 = create_deterministic_event(1, "test", 42);

    assert!(!compare_events_permissive(&event1, &event2));
}

#[test]
fn test_compare_events_permissive_different_type() {
    let event1 = create_deterministic_event(0, "test_a", 42);
    let event2 = create_deterministic_event(0, "test_b", 42);

    assert!(!compare_events_permissive(&event1, &event2));
}

#[test]
fn test_compare_events_permissive_floating_point_tolerance() {
    let mut event1 = create_deterministic_event(0, "test", 42);
    let mut event2 = create_deterministic_event(0, "test", 42);

    // Add floating point outputs with slight differences
    event1
        .outputs
        .insert("float_val".to_string(), serde_json::json!(1.0));
    event2
        .outputs
        .insert("float_val".to_string(), serde_json::json!(1.0 + 1e-10));

    assert!(compare_events_permissive(&event1, &event2));
}

#[tokio::test]
async fn test_compare_traces_identical() {
    let temp_dir = tempdir().unwrap();
    let trace_a_path = temp_dir.path().join("trace_a.ndjson");
    let trace_b_path = temp_dir.path().join("trace_b.ndjson");

    let values = vec![1, 2, 3, 4, 5];
    let bundle = create_trace_bundle_with_values(values);
    let bundle_clone = bundle.clone();

    adapteros_trace::writer::write_trace_bundle(&trace_a_path, bundle).unwrap();
    adapteros_trace::writer::write_trace_bundle(&trace_b_path, bundle_clone).unwrap();

    let result = compare_traces(&trace_a_path, &trace_b_path)
        .await
        .expect("Failed to compare traces");

    match result {
        ComparisonResult::Identical => (),
        ComparisonResult::Divergent { reason, step } => {
            panic!("Expected identical traces, got divergent: {} at step {}", reason, step);
        }
    }
}

#[tokio::test]
async fn test_compare_traces_different_lengths() {
    let temp_dir = tempdir().unwrap();
    let trace_a_path = temp_dir.path().join("trace_a.ndjson");
    let trace_b_path = temp_dir.path().join("trace_b.ndjson");

    let bundle_a = create_trace_bundle_with_values(vec![1, 2, 3]);
    let bundle_b = create_trace_bundle_with_values(vec![1, 2, 3, 4, 5]);

    adapteros_trace::writer::write_trace_bundle(&trace_a_path, bundle_a).unwrap();
    adapteros_trace::writer::write_trace_bundle(&trace_b_path, bundle_b).unwrap();

    let result = compare_traces(&trace_a_path, &trace_b_path)
        .await
        .expect("Failed to compare traces");

    match result {
        ComparisonResult::Divergent { reason, step } => {
            assert_eq!(step, 0);
            assert!(reason.contains("length mismatch"));
        }
        ComparisonResult::Identical => {
            panic!("Expected divergent traces, got identical");
        }
    }
}

#[tokio::test]
async fn test_compare_traces_different_values() {
    let temp_dir = tempdir().unwrap();
    let trace_a_path = temp_dir.path().join("trace_a.ndjson");
    let trace_b_path = temp_dir.path().join("trace_b.ndjson");

    let bundle_a = create_trace_bundle_with_values(vec![1, 2, 3]);
    let bundle_b = create_trace_bundle_with_values(vec![1, 2, 999]);

    adapteros_trace::writer::write_trace_bundle(&trace_a_path, bundle_a).unwrap();
    adapteros_trace::writer::write_trace_bundle(&trace_b_path, bundle_b).unwrap();

    let result = compare_traces(&trace_a_path, &trace_b_path)
        .await
        .expect("Failed to compare traces");

    match result {
        ComparisonResult::Divergent { reason, step } => {
            assert!(step > 0);
            assert!(reason.contains("Hash mismatch") || reason.contains("Input/output mismatch"));
        }
        ComparisonResult::Identical => {
            panic!("Expected divergent traces, got identical");
        }
    }
}

#[tokio::test]
async fn test_replay_determinism_multiple_runs() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_trace_bundle_with_values(vec![1, 2, 3, 4, 5]);
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    // First run
    let mut session1 =
        ReplaySession::from_log(&trace_path).expect("Failed to create replay session 1");
    session1.run().await.expect("First replay run failed");
    let stats1 = session1.stats().await;

    // Second run (reset and replay)
    session1.reset().await;
    session1.run().await.expect("Second replay run failed");
    let stats2 = session1.stats().await;

    // Both runs should have identical stats
    assert_eq!(stats1.total_events, stats2.total_events);
    assert_eq!(stats1.verified_ops, stats2.verified_ops);
    assert_eq!(stats1.hash_mismatches, stats2.hash_mismatches);
    assert_eq!(stats1.is_complete, stats2.is_complete);
}

#[tokio::test]
async fn test_replay_determinism_validation_during_execution() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_trace_bundle_with_values(vec![1, 2, 3, 4, 5]);
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    // Validate determinism at start
    let result = session.validate_determinism();
    assert!(result.is_ok(), "Initial determinism validation failed");

    // Step through events and validate determinism at each step
    for i in 0..5 {
        session.step().await.expect("Step failed");

        let result = session.validate_determinism();
        assert!(
            result.is_ok(),
            "Determinism validation failed at step {}: {:?}",
            i,
            result
        );
    }
}

#[tokio::test]
async fn test_replay_verification_mode_strict() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_trace_bundle_with_values(vec![1, 2, 3]);
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let mut session =
        ReplaySession::from_log_with_mode(&trace_path, VerificationMode::Strict)
            .expect("Failed to create strict session");

    let result = session.run().await;
    assert!(result.is_ok(), "Strict verification failed: {:?}", result);

    let stats = session.stats().await;
    assert_eq!(stats.hash_mismatches, 0);
    assert_eq!(stats.verified_ops, 3);
}

#[tokio::test]
async fn test_replay_verification_mode_hash_only() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_trace_bundle_with_values(vec![1, 2, 3]);
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let mut session =
        ReplaySession::from_log_with_mode(&trace_path, VerificationMode::HashOnly)
            .expect("Failed to create hash-only session");

    let result = session.run().await;
    assert!(
        result.is_ok(),
        "Hash-only verification failed: {:?}",
        result
    );

    let stats = session.stats().await;
    assert_eq!(stats.verified_ops, 3);
}

#[test]
fn test_event_hash_computation_deterministic() {
    let event1 = create_deterministic_event(42, "test_event", 123);
    let event2 = create_deterministic_event(42, "test_event", 123);

    // Same inputs should produce same hash
    assert_eq!(event1.blake3_hash, event2.blake3_hash);

    // Verify hash is correctly computed
    let computed_hash = event1.compute_hash();
    assert_eq!(event1.blake3_hash, computed_hash);
}

#[test]
fn test_event_hash_computation_different_inputs() {
    let event1 = create_deterministic_event(42, "test_event", 123);
    let event2 = create_deterministic_event(42, "test_event", 456);

    // Different inputs should produce different hashes
    assert_ne!(event1.blake3_hash, event2.blake3_hash);
}

#[test]
fn test_event_verify_hash() {
    let event = create_deterministic_event(42, "test_event", 123);

    // Event hash should verify
    assert!(event.verify_hash());
}
