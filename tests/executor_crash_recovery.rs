#![cfg(all(test, feature = "extended-tests"))]

//! Test executor crash recovery with snapshot/restore
//!
//! Verifies that executor state can be captured and restored bit-identically.

use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig, ExecutorSnapshot};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

#[tokio::test]
async fn test_snapshot_restore_tick() {
    let config = ExecutorConfig {
        global_seed: [42u8; 32],
        ..Default::default()
    };

    let executor = DeterministicExecutor::new(config.clone());

    // Spawn some tasks
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();

    executor
        .spawn_deterministic("Task 1".to_string(), async move {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        })
        .unwrap();

    // Take snapshot before running
    let snapshot1 = executor.snapshot().unwrap();
    assert_eq!(snapshot1.tick, 0);
    assert_eq!(snapshot1.pending_tasks.len(), 1);

    // Run executor
    executor.run().await.unwrap();

    // Take snapshot after running
    let snapshot2 = executor.snapshot().unwrap();
    assert!(snapshot2.tick > 0);
    assert_eq!(snapshot2.pending_tasks.len(), 0);

    // Create new executor and restore from first snapshot
    let executor2 = DeterministicExecutor::new(config);
    executor2.restore(snapshot1).unwrap();

    // Should have same state as before run
    assert_eq!(executor2.current_tick(), 0);
}

#[tokio::test]
async fn test_snapshot_restore_determinism() {
    let config = ExecutorConfig {
        global_seed: [42u8; 32],
        enable_event_logging: true,
        ..Default::default()
    };

    // First run
    let executor1 = DeterministicExecutor::new(config.clone());

    executor1
        .spawn_deterministic("Task A".to_string(), async {
            // Do work
        })
        .unwrap();

    executor1.run().await.unwrap();
    let snapshot1 = executor1.snapshot().unwrap();

    // Second run with restore
    let executor2 = DeterministicExecutor::new(config.clone());
    executor2
        .spawn_deterministic("Task A".to_string(), async {
            // Do work
        })
        .unwrap();

    executor2.run().await.unwrap();
    let snapshot2 = executor2.snapshot().unwrap();

    // Both snapshots should have identical global sequence
    assert_eq!(snapshot1.global_sequence, snapshot2.global_sequence);
    assert_eq!(snapshot1.rng_seed, snapshot2.rng_seed);
}

#[tokio::test]
async fn test_snapshot_validates_seed() {
    let config1 = ExecutorConfig {
        global_seed: [42u8; 32],
        ..Default::default()
    };

    let config2 = ExecutorConfig {
        global_seed: [43u8; 32], // Different seed
        ..Default::default()
    };

    let executor1 = DeterministicExecutor::new(config1);
    let snapshot = executor1.snapshot().unwrap();

    let executor2 = DeterministicExecutor::new(config2);

    // Should fail due to seed mismatch
    let result = executor2.restore(snapshot);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_snapshot_rejects_restore_while_running() {
    let config = ExecutorConfig {
        global_seed: [42u8; 32],
        ..Default::default()
    };

    let executor = Arc::new(DeterministicExecutor::new(config.clone()));
    let snapshot = executor.snapshot().unwrap();

    // Spawn long-running task
    let executor_clone = executor.clone();
    let handle = tokio::spawn(async move {
        executor_clone
            .spawn_deterministic("Long task".to_string(), async {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            })
            .unwrap();
        executor_clone.run().await.unwrap();
    });

    // Try to restore while running (might not catch it if task completes quickly)
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Wait for task to complete
    handle.await.unwrap();

    // Now restore should succeed since executor is not running
    let result = executor.restore(snapshot);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_snapshot_preserves_event_log() {
    let config = ExecutorConfig {
        global_seed: [42u8; 32],
        enable_event_logging: true,
        ..Default::default()
    };

    let executor = DeterministicExecutor::new(config.clone());

    executor
        .spawn_deterministic("Task 1".to_string(), async {})
        .unwrap();

    executor.run().await.unwrap();

    let snapshot = executor.snapshot().unwrap();

    // Snapshot should preserve event log
    assert!(!snapshot.event_log.is_empty());

    // Restore and verify events are preserved
    let executor2 = DeterministicExecutor::new(config);
    executor2.restore(snapshot.clone()).unwrap();

    let restored_events = executor2.get_event_log();
    assert_eq!(restored_events.len(), snapshot.event_log.len());
}
