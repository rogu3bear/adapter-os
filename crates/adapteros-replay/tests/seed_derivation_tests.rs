//! Tests for seed derivation consistency in replay
//!
//! Verifies that seed derivation is deterministic and consistent across
//! replay sessions, ensuring exact reproducibility.

use adapteros_core::B3Hash;
use adapteros_replay::ReplaySession;
use adapteros_telemetry::replay::RngCheckpoint;
use tempfile::tempdir;

mod test_helpers;
use test_helpers::create_trace_bundle_with_seed;

#[test]
fn test_global_seed_deterministic() {
    // Same input bytes should produce same hash
    let seed1 = B3Hash::hash(b"test_seed");
    let seed2 = B3Hash::hash(b"test_seed");

    assert_eq!(seed1, seed2);
}

#[test]
fn test_global_seed_different_inputs() {
    // Different inputs should produce different hashes
    let seed1 = B3Hash::hash(b"test_seed_1");
    let seed2 = B3Hash::hash(b"test_seed_2");

    assert_ne!(seed1, seed2);
}

#[tokio::test]
async fn test_replay_session_global_seed_consistency() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let seed_bytes = b"deterministic_seed";
    let bundle = create_trace_bundle_with_seed(seed_bytes);
    let expected_seed = bundle.global_seed;

    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    // Create multiple sessions from the same trace
    let session1 = ReplaySession::from_log(&trace_path).expect("Failed to create replay session 1");
    let session2 = ReplaySession::from_log(&trace_path).expect("Failed to create replay session 2");

    // Both sessions should have the same global seed
    let bundle1 = session1.trace_bundle();
    let bundle2 = session2.trace_bundle();

    assert_eq!(bundle1.global_seed, expected_seed);
    assert_eq!(bundle2.global_seed, expected_seed);
    assert_eq!(bundle1.global_seed, bundle2.global_seed);
}

#[tokio::test]
async fn test_replay_session_seed_from_trace() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let seed_bytes = b"unique_seed_12345";
    let bundle = create_trace_bundle_with_seed(seed_bytes);
    let expected_seed = B3Hash::hash(seed_bytes);

    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    let trace_bundle = session.trace_bundle();
    assert_eq!(trace_bundle.global_seed, expected_seed);
}

#[test]
fn test_rng_checkpoint_creation() {
    let checkpoint = RngCheckpoint {
        phase: "initialization".to_string(),
        label: "router_rng".to_string(),
        step_count: 0,
        global_nonce: 12345,
        timestamp_ticks: 0,
    };

    assert_eq!(checkpoint.phase, "initialization");
    assert_eq!(checkpoint.label, "router_rng");
    assert_eq!(checkpoint.step_count, 0);
    assert_eq!(checkpoint.global_nonce, 12345);
}

#[test]
fn test_rng_checkpoint_equality() {
    let checkpoint1 = RngCheckpoint {
        phase: "sampling".to_string(),
        label: "token_sampler".to_string(),
        step_count: 42,
        global_nonce: 9999,
        timestamp_ticks: 0,
    };

    let checkpoint2 = RngCheckpoint {
        phase: "sampling".to_string(),
        label: "token_sampler".to_string(),
        step_count: 42,
        global_nonce: 9999,
        timestamp_ticks: 0,
    };

    // Same checkpoints (ignoring timestamp for semantic equality)
    assert_eq!(checkpoint1.phase, checkpoint2.phase);
    assert_eq!(checkpoint1.label, checkpoint2.label);
    assert_eq!(checkpoint1.step_count, checkpoint2.step_count);
    assert_eq!(checkpoint1.global_nonce, checkpoint2.global_nonce);
}

#[tokio::test]
async fn test_verify_rng_states_identical() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_trace_bundle_with_seed(b"test_seed");
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    let checkpoints = vec![
        RngCheckpoint {
            phase: "init".to_string(),
            label: "router".to_string(),
            step_count: 0,
            global_nonce: 1,
            timestamp_ticks: 0,
        },
        RngCheckpoint {
            phase: "sampling".to_string(),
            label: "token".to_string(),
            step_count: 10,
            global_nonce: 2,
            timestamp_ticks: 0,
        },
    ];

    // Verifying identical states should succeed
    let result = session.verify_rng_states(&checkpoints, &checkpoints);
    assert!(
        result.is_ok(),
        "RNG state verification failed: {:?}",
        result
    );
}

#[tokio::test]
async fn test_verify_rng_states_different_phase() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_trace_bundle_with_seed(b"test_seed");
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    let expected = vec![RngCheckpoint {
        phase: "init".to_string(),
        label: "router".to_string(),
        step_count: 0,
        global_nonce: 1,
        timestamp_ticks: 0,
    }];

    let actual = vec![RngCheckpoint {
        phase: "sampling".to_string(), // Different phase
        label: "router".to_string(),
        step_count: 0,
        global_nonce: 1,
        timestamp_ticks: 0,
    }];

    let result = session.verify_rng_states(&expected, &actual);
    assert!(result.is_err(), "Expected RNG phase mismatch to fail");
}

#[tokio::test]
async fn test_verify_rng_states_different_label() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_trace_bundle_with_seed(b"test_seed");
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    let expected = vec![RngCheckpoint {
        phase: "init".to_string(),
        label: "router".to_string(),
        step_count: 0,
        global_nonce: 1,
        timestamp_ticks: 0,
    }];

    let actual = vec![RngCheckpoint {
        phase: "init".to_string(),
        label: "sampler".to_string(), // Different label
        step_count: 0,
        global_nonce: 1,
        timestamp_ticks: 0,
    }];

    let result = session.verify_rng_states(&expected, &actual);
    assert!(result.is_err(), "Expected RNG label mismatch to fail");
}

#[tokio::test]
async fn test_verify_rng_states_different_step_count() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_trace_bundle_with_seed(b"test_seed");
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    let expected = vec![RngCheckpoint {
        phase: "init".to_string(),
        label: "router".to_string(),
        step_count: 0,
        global_nonce: 1,
        timestamp_ticks: 0,
    }];

    let actual = vec![RngCheckpoint {
        phase: "init".to_string(),
        label: "router".to_string(),
        step_count: 10, // Different step count
        global_nonce: 1,
        timestamp_ticks: 0,
    }];

    let result = session.verify_rng_states(&expected, &actual);
    assert!(result.is_err(), "Expected RNG step count mismatch to fail");
}

#[tokio::test]
async fn test_verify_rng_states_different_nonce() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_trace_bundle_with_seed(b"test_seed");
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    let expected = vec![RngCheckpoint {
        phase: "init".to_string(),
        label: "router".to_string(),
        step_count: 0,
        global_nonce: 1,
        timestamp_ticks: 0,
    }];

    let actual = vec![RngCheckpoint {
        phase: "init".to_string(),
        label: "router".to_string(),
        step_count: 0,
        global_nonce: 2, // Different nonce
        timestamp_ticks: 0,
    }];

    let result = session.verify_rng_states(&expected, &actual);
    assert!(result.is_err(), "Expected global nonce mismatch to fail");
}

#[tokio::test]
async fn test_verify_rng_states_length_mismatch() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_trace_bundle_with_seed(b"test_seed");
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    let expected = vec![
        RngCheckpoint {
            phase: "init".to_string(),
            label: "router".to_string(),
            step_count: 0,
            global_nonce: 1,
            timestamp_ticks: 0,
        },
        RngCheckpoint {
            phase: "sampling".to_string(),
            label: "token".to_string(),
            step_count: 10,
            global_nonce: 2,
            timestamp_ticks: 0,
        },
    ];

    let actual = vec![RngCheckpoint {
        phase: "init".to_string(),
        label: "router".to_string(),
        step_count: 0,
        global_nonce: 1,
        timestamp_ticks: 0,
    }];

    let result = session.verify_rng_states(&expected, &actual);
    assert!(
        result.is_err(),
        "Expected checkpoint count mismatch to fail"
    );
}

#[tokio::test]
async fn test_verify_rng_states_multiple_checkpoints() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_trace_bundle_with_seed(b"test_seed");
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    let checkpoints = vec![
        RngCheckpoint {
            phase: "initialization".to_string(),
            label: "global".to_string(),
            step_count: 0,
            global_nonce: 1,
            timestamp_ticks: 0,
        },
        RngCheckpoint {
            phase: "routing".to_string(),
            label: "router".to_string(),
            step_count: 5,
            global_nonce: 2,
            timestamp_ticks: 0,
        },
        RngCheckpoint {
            phase: "sampling".to_string(),
            label: "token_sampler".to_string(),
            step_count: 15,
            global_nonce: 3,
            timestamp_ticks: 0,
        },
    ];

    let result = session.verify_rng_states(&checkpoints, &checkpoints);
    assert!(
        result.is_ok(),
        "Multiple checkpoint verification failed: {:?}",
        result
    );
}

#[test]
fn test_seed_hash_consistency() {
    // Test that the same manifest bytes produce the same seed
    let manifest_bytes = b"manifest_v1_model_adapters";

    let seed1 = B3Hash::hash(manifest_bytes);
    let seed2 = B3Hash::hash(manifest_bytes);
    let seed3 = B3Hash::hash(manifest_bytes);

    assert_eq!(seed1, seed2);
    assert_eq!(seed2, seed3);
}

#[test]
fn test_seed_hash_sensitivity() {
    // Test that small changes produce different seeds
    let manifest1 = b"manifest_v1";
    let manifest2 = b"manifest_v2";

    let seed1 = B3Hash::hash(manifest1);
    let seed2 = B3Hash::hash(manifest2);

    assert_ne!(seed1, seed2);
}

#[test]
fn test_b3hash_deterministic() {
    // Test B3Hash determinism across multiple invocations
    let data = b"test_data_for_hashing";
    let hashes: Vec<B3Hash> = (0..100).map(|_| B3Hash::hash(data)).collect();

    // All hashes should be identical
    for hash in &hashes {
        assert_eq!(*hash, hashes[0]);
    }
}

#[test]
fn test_b3hash_zero() {
    let zero = B3Hash::zero();
    assert_eq!(zero.as_bytes(), &[0u8; 32]);
}

#[test]
fn test_b3hash_different_inputs() {
    let inputs = vec![
        b"input1".as_slice(),
        b"input2".as_slice(),
        b"input3".as_slice(),
        b"completely_different".as_slice(),
    ];

    let hashes: Vec<B3Hash> = inputs.iter().map(|&input| B3Hash::hash(input)).collect();

    // All hashes should be unique
    for i in 0..hashes.len() {
        for j in (i + 1)..hashes.len() {
            assert_ne!(
                hashes[i], hashes[j],
                "Hash collision detected for different inputs"
            );
        }
    }
}
