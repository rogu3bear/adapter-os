//! Tests for replay execution and verification
//!
//! Verifies that replay sessions correctly execute trace bundles and
//! verify hash chains during replay.

use adapteros_crypto::signature::Keypair;
use adapteros_replay::{ReplaySession, VerificationMode};
use tempfile::tempdir;

mod test_helpers;
use test_helpers::create_test_trace_bundle;

#[tokio::test]
async fn test_replay_session_creation_from_log() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    // Create and write test trace bundle
    let bundle = create_test_trace_bundle(5);
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    // Create replay session
    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    // Verify session initialization
    let stats = session.stats().await;
    assert_eq!(stats.total_events, 5);
    assert_eq!(stats.current_step, 0);
    assert_eq!(stats.verified_ops, 0);
    assert!(!stats.is_complete);
}

#[tokio::test]
async fn test_replay_session_creation_with_verification_mode() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_test_trace_bundle(3);
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    // Test strict mode
    let session_strict = ReplaySession::from_log_with_mode(&trace_path, VerificationMode::Strict)
        .expect("Failed to create strict session");
    let stats = session_strict.stats().await;
    assert_eq!(stats.total_events, 3);

    // Test permissive mode
    let session_permissive =
        ReplaySession::from_log_with_mode(&trace_path, VerificationMode::Permissive)
            .expect("Failed to create permissive session");
    let stats = session_permissive.stats().await;
    assert_eq!(stats.total_events, 3);

    // Test hash-only mode
    let session_hash_only =
        ReplaySession::from_log_with_mode(&trace_path, VerificationMode::HashOnly)
            .expect("Failed to create hash-only session");
    let stats = session_hash_only.stats().await;
    assert_eq!(stats.total_events, 3);
}

#[tokio::test]
async fn test_replay_session_step_execution() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_test_trace_bundle(5);
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    // Step through events
    for i in 0..5 {
        let result = session.step().await;
        assert!(result.is_ok(), "Step {} failed: {:?}", i, result);

        let stats = session.stats().await;
        assert_eq!(stats.current_step, i + 1);
        assert_eq!(stats.verified_ops, i + 1);
    }

    // Final stats should show completion
    let final_stats = session.stats().await;
    assert!(final_stats.is_complete);
    assert_eq!(final_stats.verified_ops, 5);
    assert_eq!(final_stats.current_step, 5);
}

#[tokio::test]
async fn test_replay_session_run_complete() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_test_trace_bundle(10);
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let mut session =
        ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    // Run complete replay
    let result = session.run().await;
    assert!(result.is_ok(), "Replay run failed: {:?}", result);

    // Verify final state
    let stats = session.stats().await;
    assert!(stats.is_complete);
    assert_eq!(stats.current_step, 10);
    assert_eq!(stats.verified_ops, 10);
    assert_eq!(stats.progress_percent, 100.0);
}

#[tokio::test]
async fn test_replay_session_run_with_progress() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_test_trace_bundle(5);
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let mut session =
        ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    let mut progress_calls = 0;
    let result = session
        .run_with_progress(|stats| {
            progress_calls += 1;
            assert!(stats.progress_percent >= 0.0);
            assert!(stats.progress_percent <= 100.0);
        })
        .await;

    assert!(result.is_ok());
    assert!(progress_calls > 0, "Progress callback was not called");
}

#[tokio::test]
async fn test_replay_session_reset() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_test_trace_bundle(5);
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let mut session =
        ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    // Run some steps
    session.step().await.unwrap();
    session.step().await.unwrap();
    session.step().await.unwrap();

    let stats_before = session.stats().await;
    assert_eq!(stats_before.current_step, 3);
    assert_eq!(stats_before.verified_ops, 3);

    // Reset session
    session.reset().await;

    // Verify reset state
    let stats_after = session.stats().await;
    assert_eq!(stats_after.current_step, 0);
    assert_eq!(stats_after.verified_ops, 0);
    assert!(!stats_after.is_complete);
    assert_eq!(stats_after.total_events, 5);
}

#[tokio::test]
async fn test_replay_session_jump_to_step() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_test_trace_bundle(10);
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let mut session =
        ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    // Jump to step 5
    let result = session.jump_to_step(5).await;
    assert!(result.is_ok());

    let stats = session.stats().await;
    assert_eq!(stats.current_step, 5);
    assert_eq!(stats.verified_ops, 5);
    assert!(!stats.is_complete);

    // Jump to end
    let result = session.jump_to_step(10).await;
    assert!(result.is_ok());

    let stats = session.stats().await;
    assert_eq!(stats.current_step, 10);
    assert_eq!(stats.verified_ops, 10);

    // Jump out of bounds should fail
    let result = session.jump_to_step(100).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_replay_session_extract_state() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_test_trace_bundle(3);
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    // Extract executor state
    let executor_state = session.extract_state();
    assert_eq!(executor_state.current_tick, 0);
    assert!(executor_state.event_log.is_empty());
}

#[tokio::test]
async fn test_replay_session_validate_determinism() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_test_trace_bundle(5);
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    // Initially should validate (no steps taken yet)
    let result = session.validate_determinism();
    assert!(result.is_ok());

    // Step through a few events
    session.step().await.unwrap();
    session.step().await.unwrap();

    // Should still validate
    let result = session.validate_determinism();
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_replay_session_with_trusted_pubkey() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let keypair = Keypair::generate();
    let mut bundle = create_test_trace_bundle(3);

    // Sign the bundle
    let signature = keypair.sign(bundle.bundle_hash.as_bytes());
    bundle.metadata.signature = Some(hex::encode(signature.to_bytes()));

    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    // Create session with trusted pubkey
    let session = ReplaySession::from_log(&trace_path)
        .expect("Failed to create replay session")
        .with_trusted_pubkey(keypair.public_key());

    // Verify signature
    let result = session.verify_replay_signature();
    assert!(
        result.is_ok(),
        "Signature verification failed: {:?}",
        result
    );
}

#[tokio::test]
async fn test_replay_session_signature_verification_failure() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let keypair = Keypair::generate();
    let wrong_keypair = Keypair::generate();
    let mut bundle = create_test_trace_bundle(3);

    // Sign with one keypair
    let signature = keypair.sign(bundle.bundle_hash.as_bytes());
    bundle.metadata.signature = Some(hex::encode(signature.to_bytes()));

    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    // Create session with different pubkey
    let session = ReplaySession::from_log(&trace_path)
        .expect("Failed to create replay session")
        .with_trusted_pubkey(wrong_keypair.public_key());

    // Signature verification should fail
    let result = session.verify_replay_signature();
    assert!(result.is_err(), "Expected signature verification to fail");
}

#[tokio::test]
async fn test_replay_session_no_signature() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let keypair = Keypair::generate();
    let bundle = create_test_trace_bundle(3);
    // Don't sign the bundle

    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path)
        .expect("Failed to create replay session")
        .with_trusted_pubkey(keypair.public_key());

    // Signature verification should fail (no signature present)
    let result = session.verify_replay_signature();
    assert!(result.is_err(), "Expected signature verification to fail");
}

#[tokio::test]
async fn test_replay_session_executor_access() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_test_trace_bundle(3);
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    // Access executor
    let executor = session.executor();
    assert_eq!(executor.current_tick(), 0);
    assert!(!executor.is_running());
}

#[tokio::test]
async fn test_replay_session_trace_bundle_access() {
    let temp_dir = tempdir().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let bundle = create_test_trace_bundle(5);
    let expected_hash = bundle.bundle_hash;
    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    let session = ReplaySession::from_log(&trace_path).expect("Failed to create replay session");

    // Access trace bundle
    let trace_bundle = session.trace_bundle();
    assert_eq!(trace_bundle.events.len(), 5);
    assert_eq!(trace_bundle.bundle_hash, expected_hash);
}
