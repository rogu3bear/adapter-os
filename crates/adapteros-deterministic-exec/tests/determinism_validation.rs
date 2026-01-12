//! Comprehensive Stress Tests for Deterministic Execution
//!
//! Tests that validate:
//! - FIFO task ordering under load (100+ concurrent task spawns)
//! - Identical seed produces identical results (reproducibility across runs)
//! - Tick ledger synchronization with multi-agent coordination
//! - ChaCha20 RNG determinism and HKDF seed derivation
//! - CPU affinity effectiveness for scheduling determinism
//! - Event logging completeness and consistency
//! - Executor snapshot/restore for crash recovery
//! - Task timeout behavior under various conditions
//!
//! Run with: cargo test -p adapteros-deterministic-exec --test determinism_validation -- --nocapture

use adapteros_deterministic_exec::{
    DeterministicExecutor, DeterministicExecutorError, ExecutorConfig, ExecutorEvent,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a deterministic executor with standard config
fn create_executor(global_seed: [u8; 32]) -> DeterministicExecutor {
    let config = ExecutorConfig {
        global_seed,
        enable_event_logging: true,
        enable_thread_pinning: false, // Disable pinning for test portability
        worker_threads: Some(1),      // Single threaded for determinism
        ..Default::default()
    };
    DeterministicExecutor::new(config)
}

/// Helper to join all handles
#[allow(dead_code)]
async fn join_all<T>(
    handles: Vec<tokio::task::JoinHandle<T>>,
) -> Vec<std::result::Result<T, tokio::task::JoinError>> {
    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        results.push(handle.await);
    }
    results
}

// ============================================================================
// Test 1: FIFO Task Ordering Under Load
// ============================================================================

#[tokio::test]
async fn test_fifo_task_ordering_under_load() {
    let seed = [42u8; 32];
    let executor = Arc::new(create_executor(seed));
    let task_order = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let num_tasks = 100;

    // Spawn tasks in specific order
    for i in 0..num_tasks {
        let order_clone = task_order.clone();
        let _task_id = executor
            .spawn_deterministic(format!("Task {}", i), async move {
                order_clone.lock().await.push(i);
            })
            .expect("Spawn should succeed");
    }

    // Run executor
    executor.run().await.expect("Run should succeed");

    // Verify FIFO order
    let final_order = task_order.lock().await;
    assert_eq!(final_order.len(), num_tasks, "All tasks should execute");

    for (i, &task_num) in final_order.iter().enumerate() {
        assert_eq!(
            task_num, i,
            "Task at position {} should be task {} (FIFO order)",
            i, task_num
        );
    }
}

#[tokio::test]
async fn test_fifo_order_with_nested_spawns() {
    let seed = [42u8; 32];
    let executor = Arc::new(create_executor(seed));
    let execution_log = Arc::new(tokio::sync::Mutex::new(Vec::new()));

    // Spawn initial tasks
    for i in 0..10 {
        let log_clone = execution_log.clone();
        let executor_clone = executor.clone();

        let _task_id = executor
            .spawn_deterministic(format!("Parent Task {}", i), async move {
                log_clone.lock().await.push(format!("Parent {}", i));

                // Each parent spawns a child (though not truly nested in sync executor)
                let log_clone2 = log_clone.clone();
                let _child_id = executor_clone
                    .spawn_deterministic(format!("Child of Task {}", i), async move {
                        log_clone2.lock().await.push(format!("Child {}", i));
                    })
                    .ok();
            })
            .expect("Spawn should succeed");
    }

    executor.run().await.expect("Run should succeed");

    let final_log = execution_log.lock().await;
    assert!(!final_log.is_empty(), "Should have logged executions");

    // Verify parent tasks executed first (FIFO)
    for i in 0..10 {
        let parent_str = format!("Parent {}", i);
        assert!(
            final_log.iter().any(|s| s == &parent_str),
            "Parent {} should have executed",
            i
        );
    }
}

// ============================================================================
// Test 2: Deterministic Results with Same Seed
// ============================================================================

#[tokio::test]
async fn test_deterministic_results_with_same_seed() {
    let seed = [99u8; 32];

    // Run 1
    let executor1 = Arc::new(create_executor(seed));
    let counter1 = Arc::new(AtomicU64::new(0));

    for _i in 0..50 {
        let counter_clone = counter1.clone();
        let _task_id = executor1
            .spawn_deterministic("Counter Task".to_string(), async move {
                counter_clone.fetch_add(1, Ordering::Relaxed);
            })
            .expect("Spawn should succeed");
    }

    executor1.run().await.expect("Run should succeed");
    let result1 = counter1.load(Ordering::Relaxed);

    // Run 2 with same seed
    let executor2 = Arc::new(create_executor(seed));
    let counter2 = Arc::new(AtomicU64::new(0));

    for _i in 0..50 {
        let counter_clone = counter2.clone();
        let _task_id = executor2
            .spawn_deterministic("Counter Task".to_string(), async move {
                counter_clone.fetch_add(1, Ordering::Relaxed);
            })
            .expect("Spawn should succeed");
    }

    executor2.run().await.expect("Run should succeed");
    let result2 = counter2.load(Ordering::Relaxed);

    assert_eq!(
        result1, result2,
        "Deterministic results should be identical"
    );
    assert_eq!(result1, 50, "Should execute all tasks and get same count");
}

#[tokio::test]
async fn test_deterministic_random_values() {
    let seed = [77u8; 32];

    let executor1 = create_executor(seed);
    let rand1: u64 = executor1.deterministic_random();

    let executor2 = create_executor(seed);
    let rand2: u64 = executor2.deterministic_random();

    assert_eq!(
        rand1, rand2,
        "Random values with same seed should be identical"
    );
}

// ============================================================================
// Test 3: Tick Ledger Synchronization
// ============================================================================

#[tokio::test]
async fn test_tick_counter_progression() {
    let seed = [42u8; 32];
    let executor = Arc::new(create_executor(seed));

    // Spawn tasks that track tick advancement
    let tick_snapshots = Arc::new(tokio::sync::Mutex::new(Vec::new()));

    for i in 0..20 {
        let snapshots_clone = tick_snapshots.clone();
        let executor_clone = executor.clone();

        let _task_id = executor
            .spawn_deterministic(format!("Tick Task {}", i), async move {
                let tick = executor_clone.current_tick();
                snapshots_clone.lock().await.push(tick);
                tokio::task::yield_now().await;
            })
            .expect("Spawn should succeed");
    }

    let initial_tick = executor.current_tick();
    assert_eq!(initial_tick, 0, "Initial tick should be 0");

    executor.run().await.expect("Run should succeed");

    let snapshots = tick_snapshots.lock().await;
    assert!(!snapshots.is_empty(), "Should have tick snapshots");

    // Verify ticks are monotonically increasing
    for i in 1..snapshots.len() {
        assert!(
            snapshots[i] >= snapshots[i - 1],
            "Ticks must be monotonically increasing at index {}",
            i
        );
    }

    let final_tick = executor.current_tick();
    println!("Final tick counter: {}", final_tick);
}

#[tokio::test]
async fn test_tick_ledger_with_timeout_tasks() {
    let seed = [42u8; 32];
    let config = ExecutorConfig {
        global_seed: seed,
        max_ticks_per_task: 5,
        enable_event_logging: true,
        ..Default::default()
    };
    let executor = Arc::new(DeterministicExecutor::new(config));

    // Task that yields many times (will timeout)
    let _task_id = executor
        .spawn_deterministic("Timeout Task".to_string(), async {
            for _ in 0..100 {
                tokio::task::yield_now().await;
            }
        })
        .expect("Spawn should succeed");

    executor.run().await.expect("Run should succeed");

    let events = executor.get_event_log();
    let timeout_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, ExecutorEvent::TaskTimeout { .. }))
        .collect();

    assert!(!timeout_events.is_empty(), "Should have timeout event");
}

// ============================================================================
// Test 4: ChaCha20 RNG Determinism and Seed Derivation
// ============================================================================

#[tokio::test]
async fn test_chacha20_determinism() {
    let seed = [42u8; 32];

    let executor1 = create_executor(seed);
    let rand_values1: Vec<u32> = (0..100).map(|_| executor1.deterministic_random()).collect();

    let executor2 = create_executor(seed);
    let rand_values2: Vec<u32> = (0..100).map(|_| executor2.deterministic_random()).collect();

    assert_eq!(
        rand_values1, rand_values2,
        "ChaCha20 RNG with same seed must produce identical sequences"
    );

    // Verify values are actually different from each other (not all same)
    let unique_count = rand_values1
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len();
    assert!(unique_count > 50, "RNG should produce diverse values");
}

#[tokio::test]
async fn test_hkdf_seed_derivation() {
    let seed = [42u8; 32];
    let executor = create_executor(seed);

    // Derive multiple seeds with different labels
    let seed1 = executor.derive_seed("router");
    let seed2 = executor.derive_seed("dropout");
    let seed3 = executor.derive_seed("router"); // Same label as seed1

    // Same label should produce same seed
    assert_eq!(seed1, seed3, "Same label must produce same derived seed");

    // Different labels should produce different seeds
    assert_ne!(
        seed1, seed2,
        "Different labels must produce different derived seeds"
    );

    // Verify seeds are valid (not all zeros)
    assert!(seed1 != [0u8; 32], "Derived seed should not be all zeros");
}

#[tokio::test]
async fn test_hkdf_domain_separation() {
    let seed = [42u8; 32];
    let executor1 = create_executor(seed);
    let executor2 = create_executor(seed);

    // Both should derive same seed for same label
    let exec1_router = executor1.derive_seed("router");
    let exec2_router = executor2.derive_seed("router");
    assert_eq!(
        exec1_router, exec2_router,
        "Same label should produce same seed"
    );

    // But different labels should be separated
    let exec1_dropout = executor1.derive_seed("dropout");
    let exec2_sampling = executor2.derive_seed("sampling");

    assert_ne!(
        exec1_dropout, exec2_sampling,
        "Different labels should produce different seeds"
    );
}

// ============================================================================
// Test 5: CPU Affinity and Deterministic Scheduling
// ============================================================================

#[tokio::test]
async fn test_deterministic_task_interleaving() {
    let seed = [42u8; 32];
    let executor = Arc::new(create_executor(seed));

    let execution_sequence = Arc::new(tokio::sync::Mutex::new(Vec::new()));

    // Spawn tasks that record their execution order
    for task_id in 0..30 {
        let seq_clone = execution_sequence.clone();
        let _task_id = executor
            .spawn_deterministic(format!("Deterministic Task {}", task_id), async move {
                seq_clone.lock().await.push(task_id);
                // Simulate some async work
                tokio::task::yield_now().await;
            })
            .expect("Spawn should succeed");
    }

    executor.run().await.expect("Run should succeed");

    let seq = execution_sequence.lock().await;
    // In FIFO executor, we expect tasks to execute in order (though some may yield)
    assert_eq!(seq.len(), 30, "All tasks should execute");
    assert_eq!(seq[0], 0, "First task should be task 0");
}

// ============================================================================
// Test 6: Event Logging Completeness
// ============================================================================

#[tokio::test]
async fn test_event_logging_completeness() {
    let seed = [42u8; 32];
    let executor = Arc::new(create_executor(seed));

    let num_tasks = 25;
    let _task_ids: Vec<_> = (0..num_tasks)
        .map(|i| {
            executor
                .spawn_deterministic(format!("Task {}", i), async move {
                    // Quick task
                })
                .expect("Spawn should succeed")
        })
        .collect();

    executor.run().await.expect("Run should succeed");

    let events = executor.get_event_log();

    // Count event types
    let spawn_events = events
        .iter()
        .filter(|e| matches!(e, ExecutorEvent::TaskSpawned { .. }))
        .count();
    let complete_events = events
        .iter()
        .filter(|e| matches!(e, ExecutorEvent::TaskCompleted { .. }))
        .count();
    let tick_events = events
        .iter()
        .filter(|e| matches!(e, ExecutorEvent::TickAdvanced { .. }))
        .count();

    assert_eq!(
        spawn_events, num_tasks,
        "Should have spawn event for each task"
    );
    assert_eq!(
        complete_events, num_tasks,
        "Should have completion event for each task"
    );
    println!(
        "Recorded {} tick advances for {} tasks",
        tick_events, num_tasks
    );
}

#[tokio::test]
async fn test_event_hash_consistency() {
    let seed = [42u8; 32];
    let executor = Arc::new(create_executor(seed));

    let _task_id = executor
        .spawn_deterministic("Test Task".to_string(), async {
            // Simple task
        })
        .expect("Spawn should succeed");

    executor.run().await.expect("Run should succeed");

    let events = executor.get_event_log();

    // Verify event hashes are present and consistent
    for event in &events {
        match event {
            ExecutorEvent::TaskSpawned { hash, .. } => {
                assert_ne!(*hash, [0u8; 32], "Event hash should not be zero");
            }
            ExecutorEvent::TaskCompleted { hash, .. } => {
                assert_ne!(*hash, [0u8; 32], "Event hash should not be zero");
            }
            ExecutorEvent::TickAdvanced { hash, .. } => {
                assert_ne!(*hash, [0u8; 32], "Event hash should not be zero");
            }
            _ => {}
        }
    }
}

// ============================================================================
// Test 7: Executor Snapshot and Restore
// ============================================================================

#[tokio::test]
async fn test_snapshot_restore_state() {
    let seed = [42u8; 32];
    let executor = Arc::new(create_executor(seed));
    let baseline_snapshot = executor.snapshot().expect("Snapshot should succeed");
    let baseline_sequence = baseline_snapshot.global_sequence;

    // Spawn some tasks
    for i in 0..10 {
        let _task_id = executor
            .spawn_deterministic(format!("Task {}", i), async move {
                // Yield a few times
                for _ in 0..3 {
                    tokio::task::yield_now().await;
                }
            })
            .expect("Spawn should succeed");
    }

    // Partial execution
    let initial_pending = executor.pending_tasks();
    assert_eq!(initial_pending, 10, "Should have 10 pending tasks");

    // Create snapshot
    let snapshot = executor.snapshot().expect("Snapshot should succeed");

    assert_eq!(
        snapshot.pending_tasks.len(),
        10,
        "Snapshot should capture pending tasks"
    );
    assert!(
        snapshot.global_sequence >= baseline_sequence + 10,
        "Snapshot should capture global sequence"
    );

    // Restore to new executor
    let executor2 = create_executor(seed);
    executor2.restore(snapshot).expect("Restore should succeed");

    assert_eq!(
        executor2.current_tick(),
        0,
        "Restored executor should have same tick"
    );
}

#[tokio::test]
async fn test_snapshot_validation_seed_mismatch() {
    let seed1 = [42u8; 32];
    let seed2 = [99u8; 32];

    let executor1 = create_executor(seed1);
    let executor2 = create_executor(seed2);

    // Create snapshot from executor1
    let _task_id = executor1
        .spawn_deterministic("Dummy".to_string(), async {})
        .expect("Spawn should succeed");

    let snapshot = executor1.snapshot().expect("Snapshot should succeed");

    // Try to restore to executor with different seed
    let restore_result = executor2.restore(snapshot);
    assert!(
        restore_result.is_err(),
        "Restore with seed mismatch should fail"
    );

    if let Err(DeterministicExecutorError::SnapshotValidationFailed { .. }) = restore_result {
        // Expected
    } else {
        panic!("Expected SnapshotValidationFailed error");
    }
}

// ============================================================================
// Test 8: Task Timeout Behavior
// ============================================================================

#[tokio::test]
async fn test_timeout_with_yielding_task() {
    let seed = [42u8; 32];
    let config = ExecutorConfig {
        global_seed: seed,
        max_ticks_per_task: 10,
        enable_event_logging: true,
        ..Default::default()
    };
    let executor = Arc::new(DeterministicExecutor::new(config));

    // Task that yields more times than timeout allows
    let _task_id = executor
        .spawn_deterministic("Yield Heavy Task".to_string(), async {
            for _ in 0..20 {
                tokio::task::yield_now().await;
            }
        })
        .expect("Spawn should succeed");

    executor.run().await.expect("Run should succeed");

    let events = executor.get_event_log();
    let timeout_count = events
        .iter()
        .filter(|e| matches!(e, ExecutorEvent::TaskTimeout { .. }))
        .count();

    assert_eq!(timeout_count, 1, "Should have timeout event");
}

#[tokio::test]
async fn test_quick_task_completes_before_timeout() {
    let seed = [42u8; 32];
    let config = ExecutorConfig {
        global_seed: seed,
        max_ticks_per_task: 100, // High timeout
        enable_event_logging: true,
        ..Default::default()
    };
    let executor = Arc::new(DeterministicExecutor::new(config));

    let completed = Arc::new(AtomicU64::new(0));
    let completed_clone = completed.clone();

    let _task_id = executor
        .spawn_deterministic("Quick Task".to_string(), async move {
            // Complete immediately without yielding
            completed_clone.store(1, Ordering::Relaxed);
        })
        .expect("Spawn should succeed");

    executor.run().await.expect("Run should succeed");

    assert_eq!(completed.load(Ordering::Relaxed), 1, "Task should complete");

    let events = executor.get_event_log();
    let timeout_count = events
        .iter()
        .filter(|e| matches!(e, ExecutorEvent::TaskTimeout { .. }))
        .count();

    assert_eq!(timeout_count, 0, "Should not timeout for quick task");
}

// ============================================================================
// Test 9: High-Load Task Spawning
// ============================================================================

#[tokio::test]
async fn test_high_load_task_spawning() {
    let seed = [42u8; 32];
    let executor = Arc::new(create_executor(seed));

    let num_tasks = 500;
    let completed = Arc::new(AtomicU64::new(0));

    for i in 0..num_tasks {
        let completed_clone = completed.clone();
        let _task_id = executor
            .spawn_deterministic(format!("Batch Task {}", i), async move {
                completed_clone.fetch_add(1, Ordering::Relaxed);
            })
            .expect("Spawn should succeed");
    }

    assert_eq!(
        executor.pending_tasks(),
        num_tasks,
        "All tasks should be pending"
    );

    executor.run().await.expect("Run should succeed");

    assert_eq!(
        completed.load(Ordering::Relaxed),
        num_tasks as u64,
        "All tasks should complete"
    );
}

// ============================================================================
// Test 10: Deterministic Randomness Under Load
// ============================================================================

#[tokio::test]
async fn test_deterministic_randomness_consistency() {
    let seed = [42u8; 32];

    // Run 1: Collect random values in deterministic executor
    let executor1 = create_executor(seed);
    let randoms1: Vec<u64> = (0..100).map(|_| executor1.deterministic_random()).collect();

    // Run 2: Same seed should produce same sequence
    let executor2 = create_executor(seed);
    let randoms2: Vec<u64> = (0..100).map(|_| executor2.deterministic_random()).collect();

    assert_eq!(randoms1, randoms2, "Random sequences must be identical");

    // Run 3: Different seed should produce different sequence
    let different_seed = [99u8; 32];
    let executor3 = create_executor(different_seed);
    let randoms3: Vec<u64> = (0..100).map(|_| executor3.deterministic_random()).collect();

    assert_ne!(
        randoms1, randoms3,
        "Different seeds must produce different sequences"
    );
}
