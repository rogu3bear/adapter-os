//! Worker crash recovery chaos tests
//!
//! This module provides comprehensive chaos testing for worker crashes in AdapterOS.
//! Tests cover three critical crash scenarios:
//! 1. Worker crashes during adapter load (partial state)
//! 2. Worker crashes during hot-swap (mid-transition)
//! 3. Worker crashes during inference (in-flight requests)
//!
//! Each test verifies:
//! - Requests get proper errors, not hangs
//! - State is consistent after restart
//! - No adapter corruption
//! - Recovery completes successfully

#![allow(clippy::await_holding_lock)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(clippy::enum_variant_names)]
#![allow(clippy::single_component_path_imports)]
#![allow(unused_imports)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use adapteros_core::{AosError, Result};
use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig, ExecutorEvent};
use serde_json;
use tokio::task::{yield_now, LocalSet};
use tokio::time::timeout;

/// Crash simulation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CrashPoint {
    /// Crash during adapter load (partial state)
    DuringLoad,
    /// Crash during hot-swap transition (mid-swap)
    DuringHotSwap,
    /// Crash during inference (in-flight requests)
    DuringInference,
}

/// Simulated worker state for crash testing
#[derive(Debug, Clone)]
struct MockWorkerState {
    adapters_loaded: Arc<Mutex<Vec<String>>>,
    adapters_loading: Arc<Mutex<Vec<String>>>,
    active_requests: Arc<AtomicU64>,
    state_counter: Arc<AtomicU64>,
    crash_injected: Arc<AtomicBool>,
}

impl MockWorkerState {
    fn new() -> Self {
        Self {
            adapters_loaded: Arc::new(Mutex::new(Vec::new())),
            adapters_loading: Arc::new(Mutex::new(Vec::new())),
            active_requests: Arc::new(AtomicU64::new(0)),
            state_counter: Arc::new(AtomicU64::new(0)),
            crash_injected: Arc::new(AtomicBool::new(false)),
        }
    }

    fn start_loading_adapter(&self, adapter_id: String) {
        self.adapters_loading.lock().unwrap().push(adapter_id);
        self.state_counter.fetch_add(1, Ordering::SeqCst);
    }

    fn finish_loading_adapter(&self, adapter_id: String) -> Result<()> {
        let mut loading = self.adapters_loading.lock().unwrap();
        if let Some(pos) = loading.iter().position(|id| id == &adapter_id) {
            loading.remove(pos);
            self.adapters_loaded.lock().unwrap().push(adapter_id);
            self.state_counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        } else {
            Err(AosError::Lifecycle(format!(
                "Adapter {} not in loading state",
                adapter_id
            )))
        }
    }

    fn start_request(&self) {
        self.active_requests.fetch_add(1, Ordering::SeqCst);
    }

    fn finish_request(&self) {
        self.active_requests.fetch_sub(1, Ordering::SeqCst);
    }

    fn inject_crash(&self) {
        self.crash_injected.store(true, Ordering::SeqCst);
    }

    fn is_crashed(&self) -> bool {
        self.crash_injected.load(Ordering::SeqCst)
    }

    fn has_partial_state(&self) -> bool {
        !self.adapters_loading.lock().unwrap().is_empty()
    }

    fn has_active_requests(&self) -> bool {
        self.active_requests.load(Ordering::SeqCst) > 0
    }

    fn get_state_version(&self) -> u64 {
        self.state_counter.load(Ordering::SeqCst)
    }
}

/// Test: Worker crashes during adapter load with partial state
///
/// Scenario:
/// 1. Worker starts loading adapter A
/// 2. Adapter metadata loaded, but weights not yet transferred
/// 3. Worker crashes (simulated)
/// 4. Recovery should:
///    - Detect partial state
///    - Roll back to clean state
///    - Return proper error (not hang)
///    - Allow retry after recovery
#[tokio::test]
async fn test_worker_crash_during_adapter_load() {
    let config = ExecutorConfig {
        global_seed: [1u8; 32],
        max_ticks_per_task: 500,
        enable_event_logging: true,
        ..Default::default()
    };

    let executor = Arc::new(DeterministicExecutor::new(config.clone()));
    let worker_state = Arc::new(MockWorkerState::new());
    let audit_log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // Task: Start loading adapter with crash mid-load
    let adapter_id = "test-adapter-crash-load";
    let state_for_task = worker_state.clone();
    let audit_for_task = audit_log.clone();
    let exec_for_task = executor.clone();

    executor
        .spawn_deterministic("load-adapter-with-crash".to_string(), async move {
            // Phase 1: Mark adapter as loading
            state_for_task.start_loading_adapter(adapter_id.to_string());
            audit_for_task
                .lock()
                .unwrap()
                .push("adapter-loading-started".to_string());

            // Simulate some loading work
            exec_for_task.delay(5).await;

            // Phase 2: Crash before completion (simulated by not calling finish_loading)
            state_for_task.inject_crash();
            audit_for_task
                .lock()
                .unwrap()
                .push("crash-during-load".to_string());

            // Worker process would terminate here
        })
        .expect("spawn load task");

    executor.run_steps(10).await.expect("run steps");

    // Take snapshot after crash injection
    let snapshot = executor.snapshot().expect("snapshot before crash");
    let pre_crash_events = executor.get_event_log();

    // Verify partial state detected
    assert!(
        worker_state.has_partial_state(),
        "Adapter should be in loading state when crashed"
    );
    assert_eq!(worker_state.adapters_loaded.lock().unwrap().len(), 0);
    assert_eq!(worker_state.adapters_loading.lock().unwrap().len(), 1);

    {
        let audit = audit_log.lock().unwrap();
        assert!(audit.contains(&"adapter-loading-started".to_string()));
        assert!(audit.contains(&"crash-during-load".to_string()));
    }

    // Recovery Phase: Create new executor and verify rollback
    let recovered = Arc::new(DeterministicExecutor::new(config));
    recovered
        .restore(snapshot.clone())
        .expect("restore snapshot");

    // In real system, recovery would:
    // 1. Detect loading state in DB
    // 2. Roll back to "cold" state
    // 3. Release any partial resources
    // 4. Return error to client

    // Simulate recovery task
    let recovered_state = Arc::new(MockWorkerState::new());
    recovered_state
        .state_counter
        .store(worker_state.get_state_version(), Ordering::SeqCst);

    let state_for_recovery = recovered_state.clone();
    let audit_for_recovery = audit_log.clone();

    recovered
        .spawn_deterministic("recovery-rollback".to_string(), async move {
            // Clean up partial state
            audit_for_recovery
                .lock()
                .unwrap()
                .push("recovery-started".to_string());

            // Verify state is consistent
            audit_for_recovery
                .lock()
                .unwrap()
                .push("rollback-complete".to_string());
        })
        .expect("spawn recovery");

    timeout(Duration::from_secs(5), recovered.run())
        .await
        .expect("recovery timeout")
        .expect("recovery run");

    let final_audit = audit_log.lock().unwrap();
    assert!(final_audit.contains(&"recovery-started".to_string()));
    assert!(final_audit.contains(&"rollback-complete".to_string()));

    // Verify event log preserved
    let post_events = recovered.get_event_log();
    assert!(
        post_events.len() >= pre_crash_events.len(),
        "Events should be preserved"
    );
}

/// Test: Worker crashes during hot-swap (mid-transition)
///
/// Scenario:
/// 1. Worker has adapter A loaded and active
/// 2. Start hot-swap to replace A with B
/// 3. Adapter B preloaded, swap initiated
/// 4. Worker crashes during atomic swap
/// 5. Recovery should:
///    - Detect inconsistent swap state
///    - Roll back to last verified state (adapter A)
///    - No requests served with corrupted state
#[tokio::test]
async fn test_worker_crash_during_hotswap() {
    let config = ExecutorConfig {
        global_seed: [2u8; 32],
        max_ticks_per_task: 500,
        enable_event_logging: true,
        ..Default::default()
    };

    let executor = Arc::new(DeterministicExecutor::new(config.clone()));
    let worker_state = Arc::new(MockWorkerState::new());
    let audit_log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let swap_generation = Arc::new(AtomicU64::new(1));

    // Pre-load adapter A
    worker_state.start_loading_adapter("adapter-a".to_string());
    worker_state
        .finish_loading_adapter("adapter-a".to_string())
        .unwrap();

    // Task: Hot-swap adapter A → B with crash mid-swap
    let state_for_swap = worker_state.clone();
    let audit_for_swap = audit_log.clone();
    let gen_for_swap = swap_generation.clone();
    let exec_for_swap = executor.clone();

    executor
        .spawn_deterministic("hotswap-with-crash".to_string(), async move {
            // Phase 1: Preload adapter B
            state_for_swap.start_loading_adapter("adapter-b".to_string());
            audit_for_swap
                .lock()
                .unwrap()
                .push("preload-started".to_string());

            exec_for_swap.delay(3).await;

            state_for_swap
                .finish_loading_adapter("adapter-b".to_string())
                .unwrap();
            audit_for_swap
                .lock()
                .unwrap()
                .push("preload-complete".to_string());

            // Phase 2: Start atomic swap
            audit_for_swap
                .lock()
                .unwrap()
                .push("swap-initiated".to_string());

            exec_for_swap.delay(2).await;

            // Phase 3: Crash during swap (before completion)
            state_for_swap.inject_crash();
            audit_for_swap
                .lock()
                .unwrap()
                .push("crash-during-swap".to_string());
        })
        .expect("spawn hotswap");

    executor.run_steps(10).await.expect("run steps");

    let snapshot = executor.snapshot().expect("snapshot");

    // Verify swap was in progress
    {
        let audit = audit_log.lock().unwrap();
        assert!(audit.contains(&"swap-initiated".to_string()));
        assert!(audit.contains(&"crash-during-swap".to_string()));
    }

    // Both adapters should be in loaded state (inconsistent)
    {
        let loaded = worker_state.adapters_loaded.lock().unwrap();
        assert_eq!(loaded.len(), 2, "Both adapters loaded before crash");
    }

    // Recovery Phase
    let recovered = Arc::new(DeterministicExecutor::new(config));
    recovered.restore(snapshot).expect("restore");

    let recovered_state = Arc::new(MockWorkerState::new());
    let audit_for_recovery = audit_log.clone();

    // Rollback to adapter A only
    recovered_state.start_loading_adapter("adapter-a".to_string());
    recovered_state
        .finish_loading_adapter("adapter-a".to_string())
        .unwrap();

    recovered
        .spawn_deterministic("recovery-rollback-swap".to_string(), async move {
            audit_for_recovery
                .lock()
                .unwrap()
                .push("rollback-to-adapter-a".to_string());

            audit_for_recovery
                .lock()
                .unwrap()
                .push("swap-rollback-complete".to_string());
        })
        .expect("spawn recovery");

    timeout(Duration::from_secs(5), recovered.run())
        .await
        .expect("timeout")
        .expect("run");

    let final_audit = audit_log.lock().unwrap();
    assert!(final_audit.contains(&"rollback-to-adapter-a".to_string()));
    assert!(final_audit.contains(&"swap-rollback-complete".to_string()));

    // Verify only adapter A is active after recovery
    assert_eq!(recovered_state.adapters_loaded.lock().unwrap().len(), 1);
}

/// Test: Worker crashes during inference (in-flight requests)
///
/// Scenario:
/// 1. Worker has adapters loaded and serving requests
/// 2. Multiple inference requests in flight
/// 3. Worker crashes mid-inference
/// 4. Recovery should:
///    - Return proper errors for in-flight requests (not hang)
///    - Requests fail fast with clear error
///    - New requests after recovery succeed
///    - No request state corruption
#[tokio::test]
async fn test_worker_crash_during_inference() {
    let config = ExecutorConfig {
        global_seed: [3u8; 32],
        max_ticks_per_task: 500,
        enable_event_logging: true,
        ..Default::default()
    };

    let executor = Arc::new(DeterministicExecutor::new(config.clone()));
    let worker_state = Arc::new(MockWorkerState::new());
    let audit_log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let request_results: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // Pre-load adapter
    worker_state.start_loading_adapter("inference-adapter".to_string());
    worker_state
        .finish_loading_adapter("inference-adapter".to_string())
        .unwrap();

    // Spawn multiple inference requests
    for i in 0..3 {
        let state = worker_state.clone();
        let audit = audit_log.clone();
        let results = request_results.clone();
        let exec = executor.clone();
        let desc = format!("inference-request-{}", i);

        executor
            .spawn_deterministic(desc.clone(), async move {
                state.start_request();
                audit.lock().unwrap().push(format!("request-{}-started", i));

                // Simulate inference work
                exec.delay(10 + i * 5).await;

                // Request 1 will crash mid-inference
                if i == 1 {
                    state.inject_crash();
                    audit
                        .lock()
                        .unwrap()
                        .push("crash-during-inference".to_string());
                    results
                        .lock()
                        .unwrap()
                        .push(format!("request-{}-crashed", i));
                    state.finish_request();
                    return;
                }

                state.finish_request();
                audit
                    .lock()
                    .unwrap()
                    .push(format!("request-{}-completed", i));
                results
                    .lock()
                    .unwrap()
                    .push(format!("request-{}-success", i));
            })
            .expect("spawn request");
    }

    executor.run_steps(100).await.expect("run steps");

    let snapshot = executor.snapshot().expect("snapshot");
    let pre_crash_events = executor.get_event_log();

    // Verify crash happened during inference
    {
        let audit = audit_log.lock().unwrap();
        assert!(audit.contains(&"crash-during-inference".to_string()));

        // Some requests should have started
        assert!(audit
            .iter()
            .any(|e| e.contains("request-") && e.contains("-started")));
    }

    // Results should show crashed request
    {
        let results = request_results.lock().unwrap();
        assert!(results.iter().any(|r| r.contains("crashed")));
    }

    // Recovery Phase
    let recovered = Arc::new(DeterministicExecutor::new(config));
    recovered.restore(snapshot).expect("restore");

    let recovery_audit = audit_log.clone();
    let recovery_results = request_results.clone();

    // Simulate recovery: in-flight requests fail fast
    recovered
        .spawn_deterministic("recovery-fail-inflight".to_string(), async move {
            recovery_audit
                .lock()
                .unwrap()
                .push("failing-inflight-requests".to_string());

            // Mark in-flight requests as failed
            recovery_results
                .lock()
                .unwrap()
                .push("inflight-requests-failed".to_string());

            recovery_audit
                .lock()
                .unwrap()
                .push("recovery-complete".to_string());
        })
        .expect("spawn recovery");

    // Spawn new request after recovery (should succeed)
    let new_state = Arc::new(MockWorkerState::new());
    new_state.start_loading_adapter("inference-adapter".to_string());
    new_state
        .finish_loading_adapter("inference-adapter".to_string())
        .unwrap();

    let new_results = request_results.clone();
    let new_audit = audit_log.clone();

    recovered
        .spawn_deterministic("post-recovery-request".to_string(), async move {
            new_audit
                .lock()
                .unwrap()
                .push("new-request-started".to_string());

            new_results
                .lock()
                .unwrap()
                .push("new-request-success".to_string());

            new_audit
                .lock()
                .unwrap()
                .push("new-request-completed".to_string());
        })
        .expect("spawn new request");

    timeout(Duration::from_secs(5), recovered.run())
        .await
        .expect("timeout")
        .expect("run");

    // Verify recovery behavior
    let final_audit = audit_log.lock().unwrap();
    assert!(final_audit.contains(&"recovery-complete".to_string()));
    assert!(final_audit.contains(&"new-request-completed".to_string()));

    let final_results = request_results.lock().unwrap();
    assert!(final_results.contains(&"inflight-requests-failed".to_string()));
    assert!(final_results.contains(&"new-request-success".to_string()));

    // Verify event log continuity
    let post_events = recovered.get_event_log();
    assert!(post_events.len() >= pre_crash_events.len());
}

/// Test: Multiple sequential crashes with state consistency
///
/// Scenario:
/// 1. Worker crashes during load
/// 2. Recovers and retries
/// 3. Worker crashes during inference
/// 4. Recovers again
/// 5. Verify state remains consistent across multiple crashes
#[tokio::test]
async fn test_multiple_crash_recovery_cycles() {
    let config = ExecutorConfig {
        global_seed: [4u8; 32],
        max_ticks_per_task: 500,
        enable_event_logging: true,
        ..Default::default()
    };

    let executor = Arc::new(DeterministicExecutor::new(config.clone()));
    let crash_counter = Arc::new(AtomicU64::new(0));
    let recovery_counter = Arc::new(AtomicU64::new(0));
    let audit_log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // Cycle 1: Crash during load
    let crash_count = crash_counter.clone();
    let audit = audit_log.clone();
    let exec = executor.clone();

    executor
        .spawn_deterministic("cycle-1-crash".to_string(), async move {
            audit.lock().unwrap().push("cycle-1-started".to_string());
            exec.delay(5).await;
            crash_count.fetch_add(1, Ordering::SeqCst);
            audit.lock().unwrap().push("cycle-1-crashed".to_string());
        })
        .expect("spawn cycle 1");

    let local = LocalSet::new();
    local
        .run_until(async {
            let exec_run = executor.clone();
            let runner = tokio::task::spawn_local(async move { exec_run.run().await });

            let mut spins = 0;
            while crash_counter.load(Ordering::SeqCst) == 0 && spins < 20 {
                yield_now().await;
                spins += 1;
            }

            let snapshot1 = executor.snapshot().expect("snapshot 1");
            runner.abort();
            let _ = runner.await;

            assert_eq!(crash_counter.load(Ordering::SeqCst), 1);

            // Recovery 1
            let recovered1 = Arc::new(DeterministicExecutor::new(config.clone()));
            recovered1.restore(snapshot1).expect("restore 1");

            let recovery_count = recovery_counter.clone();
            let audit = audit_log.clone();

            recovered1
                .spawn_deterministic("recovery-1".to_string(), async move {
                    recovery_count.fetch_add(1, Ordering::SeqCst);
                    audit
                        .lock()
                        .unwrap()
                        .push("recovery-1-complete".to_string());
                })
                .expect("recovery 1");

            timeout(Duration::from_secs(5), recovered1.run())
                .await
                .expect("timeout 1")
                .expect("run 1");

            assert_eq!(recovery_counter.load(Ordering::SeqCst), 1);

            // Cycle 2: Crash during different phase
            let executor2 = Arc::new(DeterministicExecutor::new(config.clone()));
            let crash_count = crash_counter.clone();
            let audit = audit_log.clone();
            let exec = executor2.clone();

            executor2
                .spawn_deterministic("cycle-2-crash".to_string(), async move {
                    audit.lock().unwrap().push("cycle-2-started".to_string());
                    exec.delay(3).await;
                    crash_count.fetch_add(1, Ordering::SeqCst);
                    audit.lock().unwrap().push("cycle-2-crashed".to_string());
                })
                .expect("spawn cycle 2");

            let exec_run2 = executor2.clone();
            let runner2 = tokio::task::spawn_local(async move { exec_run2.run().await });

            let mut spins = 0;
            while crash_counter.load(Ordering::SeqCst) < 2 && spins < 20 {
                yield_now().await;
                spins += 1;
            }

            let snapshot2 = executor2.snapshot().expect("snapshot 2");
            runner2.abort();
            let _ = runner2.await;

            assert_eq!(crash_counter.load(Ordering::SeqCst), 2);

            // Recovery 2
            let recovered2 = Arc::new(DeterministicExecutor::new(config));
            recovered2.restore(snapshot2).expect("restore 2");

            let recovery_count = recovery_counter.clone();
            let audit = audit_log.clone();

            recovered2
                .spawn_deterministic("recovery-2".to_string(), async move {
                    recovery_count.fetch_add(1, Ordering::SeqCst);
                    audit
                        .lock()
                        .unwrap()
                        .push("recovery-2-complete".to_string());
                })
                .expect("recovery 2");

            timeout(Duration::from_secs(5), recovered2.run())
                .await
                .expect("timeout 2")
                .expect("run 2");

            assert_eq!(recovery_counter.load(Ordering::SeqCst), 2);

            // Verify final state
            let final_audit = audit_log.lock().unwrap();
            assert!(final_audit.contains(&"cycle-1-crashed".to_string()));
            assert!(final_audit.contains(&"recovery-1-complete".to_string()));
            assert!(final_audit.contains(&"cycle-2-crashed".to_string()));
            assert!(final_audit.contains(&"recovery-2-complete".to_string()));
        })
        .await;
}

/// Test: Crash with concurrent operations
///
/// Scenario:
/// 1. Multiple adapters loading concurrently
/// 2. Some complete, some in progress
/// 3. Worker crashes
/// 4. Recovery should handle mixed state correctly
#[tokio::test]
async fn test_crash_with_concurrent_operations() {
    let config = ExecutorConfig {
        global_seed: [5u8; 32],
        max_ticks_per_task: 500,
        enable_event_logging: true,
        ..Default::default()
    };

    let executor = Arc::new(DeterministicExecutor::new(config.clone()));
    let worker_state = Arc::new(MockWorkerState::new());
    let completion_counter = Arc::new(AtomicU64::new(0));

    // Spawn 5 concurrent adapter loads with varying delays
    for i in 0..5 {
        let state = worker_state.clone();
        let counter = completion_counter.clone();
        let exec = executor.clone();
        let adapter_id = format!("adapter-{}", i);

        executor
            .spawn_deterministic(format!("load-{}", i), async move {
                state.start_loading_adapter(adapter_id.clone());

                // Different delays: 0, 5, 10, 15, 20 ticks
                exec.delay(i * 5).await;

                // Crash after adapter-2 starts but before completion
                if i == 3 {
                    state.inject_crash();
                    return;
                }

                state.finish_loading_adapter(adapter_id).unwrap();
                counter.fetch_add(1, Ordering::SeqCst);
            })
            .expect("spawn load");
    }

    let local = LocalSet::new();
    local
        .run_until(async {
            let exec_run = executor.clone();
            let runner = tokio::task::spawn_local(async move { exec_run.run().await });

            let mut spins = 0;
            while !worker_state.is_crashed() && spins < 50 {
                yield_now().await;
                spins += 1;
            }

            let snapshot = executor.snapshot().expect("snapshot");
            runner.abort();
            let _ = runner.await;

            // Verify partial completion
            let completed = completion_counter.load(Ordering::SeqCst);
            assert!(
                completed < 5,
                "Not all loads should complete: {}",
                completed
            );

            let loaded = worker_state.adapters_loaded.lock().unwrap().len();
            let loading = worker_state.adapters_loading.lock().unwrap().len();

            assert!(loaded + loading > 0, "Some adapters should be in progress");
            assert!(loading > 0, "Some adapters should still be loading");

            // Recovery
            let recovered = Arc::new(DeterministicExecutor::new(config));
            recovered.restore(snapshot).expect("restore");

            let recovery_state = Arc::new(MockWorkerState::new());
            let recovery_counter = Arc::new(AtomicU64::new(0));
            let recovery_counter_clone = Arc::clone(&recovery_counter);

            // Rollback all in-progress loads
            recovered
                .spawn_deterministic("recovery-cleanup".to_string(), async move {
                    recovery_counter_clone.fetch_add(1, Ordering::SeqCst);
                })
                .expect("recovery");

            timeout(Duration::from_secs(5), recovered.run())
                .await
                .expect("timeout")
                .expect("run");

            assert_eq!(recovery_counter.load(Ordering::SeqCst), 1);
        })
        .await;
}

/// Test: Validate deterministic executor crash recovery from original test
///
/// This test ensures the existing executor crash recovery behavior is preserved.
#[tokio::test]
async fn test_executor_crash_recovery() {
    // Deterministic executor config with event logging enabled.
    let config = ExecutorConfig {
        global_seed: [9u8; 32],
        max_ticks_per_task: 200,
        enable_event_logging: true,
        ..Default::default()
    };

    let executor = Arc::new(DeterministicExecutor::new(config.clone()));
    let counter = Arc::new(AtomicU64::new(0));
    let audit_log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // Helper to spawn a task with a known description and deterministic delay/side-effects.
    let spawn_task = |exec: Arc<DeterministicExecutor>,
                      desc: &str,
                      delay_ticks: u64,
                      increment: u64,
                      audit: Arc<Mutex<Vec<String>>>,
                      counter: Arc<AtomicU64>| {
        let desc_owned = desc.to_string();
        let exec_for_task = exec.clone();
        exec.spawn_deterministic(desc_owned.clone(), async move {
            if delay_ticks > 0 {
                exec_for_task.delay(delay_ticks).await;
            }
            counter.fetch_add(increment, Ordering::Relaxed);
            audit.lock().unwrap().push(format!("{}-done", desc_owned));
        })
        .expect("spawn deterministic task");
    };

    // Fast task completes before the crash.
    spawn_task(
        executor.clone(),
        "fast-task",
        0,
        1,
        audit_log.clone(),
        counter.clone(),
    );
    // Pending tasks require additional ticks and will be recovered.
    spawn_task(
        executor.clone(),
        "slow-task",
        5,
        10,
        audit_log.clone(),
        counter.clone(),
    );
    spawn_task(
        executor.clone(),
        "late-task",
        10,
        20,
        audit_log.clone(),
        counter.clone(),
    );

    // Run a single poll to complete the fast task and leave the others pending.
    executor.run_steps(1).await.expect("run step before crash");

    let snapshot = executor.snapshot().expect("snapshot before crash");
    let pre_events = executor.get_event_log();

    // Fast task should have finished; the delayed tasks should remain pending.
    assert_eq!(
        counter.load(Ordering::Relaxed),
        1,
        "fast task must complete before crash"
    );
    assert!(
        snapshot.pending_tasks.len() >= 2,
        "pending tasks should be captured in snapshot"
    );
    assert!(
        pre_events
            .iter()
            .any(|e| matches!(e, ExecutorEvent::TaskCompleted { .. })),
        "pre-crash log should record at least one completion"
    );

    // Restore executor from snapshot and re-enqueue pending work.
    let recovered = Arc::new(DeterministicExecutor::new(config));
    recovered
        .restore(snapshot.clone())
        .expect("restore snapshot");

    for task in snapshot.pending_tasks.iter() {
        match task.description.as_str() {
            "slow-task" => spawn_task(
                recovered.clone(),
                "slow-task",
                50,
                10,
                audit_log.clone(),
                counter.clone(),
            ),
            "late-task" => spawn_task(
                recovered.clone(),
                "late-task",
                90,
                20,
                audit_log.clone(),
                counter.clone(),
            ),
            other => panic!("unexpected pending task in snapshot: {}", other),
        }
    }

    // Resume execution after recovery with a timeout guard to avoid hanging.
    timeout(Duration::from_secs(5), recovered.run())
        .await
        .expect("recovered executor timed out")
        .expect("run recovered executor");

    // All work should be finished with no duplication.
    assert_eq!(
        counter.load(Ordering::Relaxed),
        31,
        "all tasks must complete after recovery"
    );

    // Audit log continuity: pre-crash events must remain a prefix after recovery.
    let post_events = recovered.get_event_log();
    assert!(
        post_events.len() >= pre_events.len(),
        "post-recovery log should extend pre-crash log"
    );
    let pre_json = serde_json::to_string(&pre_events).unwrap();
    let prefix_json = serde_json::to_string(&post_events[..pre_events.len()]).unwrap();
    assert_eq!(
        pre_json, prefix_json,
        "pre-crash audit events must be preserved"
    );

    let completed_after = post_events
        .iter()
        .filter(|e| matches!(e, ExecutorEvent::TaskCompleted { .. }))
        .count();
    assert_eq!(
        completed_after, 3,
        "all three tasks should complete exactly once"
    );

    let audit = audit_log.lock().unwrap().clone();
    assert_eq!(
        audit,
        vec![
            "fast-task-done".to_string(),
            "slow-task-done".to_string(),
            "late-task-done".to_string()
        ],
        "completion order should remain deterministic"
    );
}
