//! Executor crash recovery tests

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig, ExecutorEvent};
use serde_json;
use tokio::task::{yield_now, LocalSet};
use tokio::time::{timeout, Duration};

/// Validate that a simulated crash preserves queued work and audit log,
/// and that recovery completes pending tasks without duplication.
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

    let local = LocalSet::new();
    local
        .run_until(async {
            // Run executor in background and crash it mid-flight.
            let exec_for_run = executor.clone();
            let runner = tokio::task::spawn_local(async move { exec_for_run.run().await });

            // Allow the fast task to finish but stop before long delays elapse.
            let mut spins = 0;
            while counter.load(Ordering::Relaxed) == 0 && spins < 16 {
                yield_now().await;
                spins += 1;
            }

            let snapshot = executor.snapshot().expect("snapshot before crash");
            let pre_events = executor.get_event_log();

            // Crash the executor (abort run loop).
            runner.abort();
            let _ = runner.await;

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
        })
        .await;
}
