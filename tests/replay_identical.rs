//! Tests for bit-identical replay verification

use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;

use adapteros_core::B3Hash;
use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig, ExecutorEvent};
use adapteros_replay::{
    compare_traces, replay_trace, ComparisonResult, ReplaySession, VerificationMode,
};
use adapteros_trace::{
    events::{inference_end_event, inference_start_event, token_generated_event},
    reader::read_trace_bundle,
    schema::{TraceBundle, TraceBundleMetadata},
    writer::write_trace_bundle,
};

/// Create a test trace bundle with deterministic events
fn create_test_trace_bundle() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let trace_path = temp_dir.path().join("test_trace.ndjson");

    let global_seed = B3Hash::hash(b"test_seed");
    let session_id = "test_session".to_string();
    let plan_id = "test_plan".to_string();

    let mut bundle = TraceBundle {
        metadata: TraceBundleMetadata {
            cpid: session_id.clone(),
            plan_id: plan_id.clone(),
            global_seed,
            start_timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            end_timestamp_ns: None,
            event_count: 0,
            total_size_bytes: 0,
            merkle_root: None,
            signature: None,
            toolchain_info: None,
        },
        events: Vec::new(),
    };

    bundle.events.push(inference_start_event(
        1,
        plan_id.clone(),
        "test_cpid".to_string(),
        "test_tenant".to_string(),
        session_id.clone(),
        global_seed,
    ));
    bundle.events.push(token_generated_event(
        2,
        0,
        vec![0.1, 0.2],
        vec!["adapter_a".to_string()],
    ));
    bundle.events.push(token_generated_event(
        3,
        1,
        vec![0.3, 0.4],
        vec!["adapter_b".to_string()],
    ));
    bundle
        .events
        .push(inference_end_event(4, session_id.clone(), 2, 100));

    adapteros_trace::writer::write_trace_bundle(&trace_path, bundle).unwrap();

    (temp_dir, trace_path)
}

#[tokio::test]
async fn test_replay_identical_runs() -> Result<()> {
    let (_temp_dir_1, trace_path_1) = create_test_trace_bundle();
    let (_temp_dir_2, trace_path_2) = create_test_trace_bundle();

    // Compare traces
    let comparison_result = compare_traces(&trace_path_1, &trace_path_2).await?;
    assert!(
        matches!(comparison_result, ComparisonResult::Identical),
        "Traces should be identical"
    );

    // Replay trace 1
    let stats_1 = replay_trace(&trace_path_1).await?;
    assert!(stats_1.is_complete);
    assert_eq!(stats_1.hash_mismatches, 0);
    assert_eq!(stats_1.verified_ops, stats_1.total_events);

    // Replay trace 2
    let stats_2 = replay_trace(&trace_path_2).await?;
    assert!(stats_2.is_complete);
    assert_eq!(stats_2.hash_mismatches, 0);
    assert_eq!(stats_2.verified_ops, stats_2.total_events);

    Ok(())
}

#[tokio::test]
async fn test_replay_session_step_and_reset() -> Result<()> {
    let (_temp_dir, trace_path) = create_test_trace_bundle();
    let mut session = ReplaySession::from_log(&trace_path)?;

    let initial_stats = session.stats().await;
    assert!(!initial_stats.is_complete);
    assert_eq!(initial_stats.current_step, 0);

    // Step through events
    for i in 1..=initial_stats.total_events {
        session.step().await?;
        let current_stats = session.stats().await;
        assert_eq!(current_stats.current_step, i);
        assert_eq!(current_stats.verified_ops, i);
        if i < initial_stats.total_events {
            assert!(!current_stats.is_complete);
        }
    }

    let final_stats = session.stats().await;
    assert!(final_stats.is_complete);
    assert_eq!(final_stats.current_step, final_stats.total_events);
    assert_eq!(final_stats.verified_ops, final_stats.total_events);

    // Reset session
    session.reset();
    let reset_stats = session.stats().await;
    assert_eq!(reset_stats.current_step, 0);
    assert_eq!(reset_stats.verified_ops, 0);
    assert!(!reset_stats.is_complete);

    Ok(())
}

#[tokio::test]
async fn test_replay_session_jump_to_step() -> Result<()> {
    let (_temp_dir, trace_path) = create_test_trace_bundle();
    let mut session = ReplaySession::from_log(&trace_path)?;

    let total_events = session.stats().await.total_events;

    // Jump to an intermediate step
    let jump_step = 2;
    session.jump_to_step(jump_step)?;
    let jump_stats = session.stats().await;
    assert_eq!(jump_stats.current_step, jump_step);
    assert_eq!(jump_stats.verified_ops, jump_step);
    assert!(!jump_stats.is_complete);

    // Continue stepping from there
    for i in jump_step + 1..=total_events {
        session.step().await?;
        let current_stats = session.stats().await;
        assert_eq!(current_stats.current_step, i);
        assert_eq!(current_stats.verified_ops, i);
        if i < total_events {
            assert!(!current_stats.is_complete);
        }
    }

    let final_stats = session.stats().await;
    assert!(final_stats.is_complete);
    assert_eq!(final_stats.current_step, total_events);
    assert_eq!(final_stats.verified_ops, total_events);

    Ok(())
}

#[tokio::test]
async fn test_replay_verification_modes() -> Result<()> {
    let (_temp_dir, trace_path) = create_test_trace_bundle();

    // Strict mode
    let stats_strict = replay_trace(&trace_path).await?;
    assert_eq!(stats_strict.hash_mismatches, 0);

    // Permissive mode (currently same as strict)
    let mut session_permissive =
        ReplaySession::from_log_with_mode(&trace_path, VerificationMode::Permissive)?;
    session_permissive.run().await?;
    let stats_permissive = session_permissive.stats().await;
    assert_eq!(stats_permissive.hash_mismatches, 0);

    // HashOnly mode
    let mut session_hash_only =
        ReplaySession::from_log_with_mode(&trace_path, VerificationMode::HashOnly)?;
    session_hash_only.run().await?;
    let stats_hash_only = session_hash_only.stats().await;
    assert_eq!(stats_hash_only.hash_mismatches, 0); // No mismatches expected as it only checks if hash can be computed

    assert_eq!(stats_strict.verified_ops, stats_permissive.verified_ops);
    assert_eq!(stats_permissive.verified_ops, stats_hash_only.verified_ops);

    Ok(())
}
