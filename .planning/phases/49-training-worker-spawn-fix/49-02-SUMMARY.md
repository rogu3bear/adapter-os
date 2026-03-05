---
phase: 49-training-worker-spawn-fix
plan: 02
subsystem: server/boot, db
tags: [training-worker, circuit-breaker, crash-cleanup, supervisor]
dependency_graph:
  requires: [49-01]
  provides: [worker-circuit-breaker, crash-job-cleanup]
  affects: [adapteros-server, adapteros-db]
tech_stack:
  added: []
  patterns: [sliding-window-circuit-breaker, bulk-job-failover]
key_files:
  created: []
  modified:
    - crates/adapteros-server/src/boot/background_tasks.rs
    - crates/adapteros-db/src/training_jobs.rs
decisions:
  - 3 crashes in 5 minutes triggers permanent degradation (circuit breaker)
  - Graceful exits (exit code 0) do not count as crashes
  - In-flight jobs marked failed on any worker exit (not just crashes)
  - VecDeque<Instant> sliding window for O(1) amortized crash tracking
metrics:
  duration: 22m
  completed: 2026-03-05T04:23:00Z
  tasks_completed: 2
  tasks_total: 2
---

# Phase 49 Plan 02: Supervisor Circuit Breaker and Crash Job Cleanup Summary

Sliding-window circuit breaker (3 crashes in 5 minutes) stops crash-looping training worker restarts and marks in-flight training jobs as failed with actionable reason on worker crash.

## Changes

### WorkerCircuitBreaker Struct
Added `WorkerCircuitBreaker` in `background_tasks.rs` with:
- `VecDeque<tokio::time::Instant>` sliding window for crash timestamps
- `record_crash()` evicts old entries outside the window, returns whether tripped
- `is_tripped()` for spawn gate check
- Initialized with `new(3, Duration::from_secs(300))` (3 crashes in 5 minutes)

### Circuit Breaker Integration
Integrated into both crash branches of the supervisor loop:

1. **Ok(Some(status)) with !status.success()**: Records crash in circuit breaker. If tripped, writes degraded marker with circuit breaker reason and stops restart attempts.
2. **Ok(Some(status)) with status.success()**: Graceful exit. Logged at `info!`, does NOT record as crash. Worker will be restarted normally.
3. **Err(e)**: Status inspection failure treated as crash. Same circuit breaker integration.

When tripped, writes: `"circuit breaker tripped: 3 crashes in 300 seconds\n"` to `var/run/training-worker.degraded`.

### In-Flight Job Cleanup
- Cloned `state.db` (ProtectedDb, Arc-backed) into the supervisor closure
- On any worker exit (crash or graceful), calls `db.mark_running_jobs_failed_worker_crash()`
- Logs affected job count at `warn!` level, errors at `error!`
- Jobs marked failed before circuit breaker check to ensure cleanup happens even on the tripping crash

### mark_running_jobs_failed_worker_crash DB Method
Added to `Db` impl in `training_jobs.rs`:
- **KV path**: Lists running jobs via `list_jobs_by_status("running", 1000)`, updates each to failed
- **SQL path**: Bulk `UPDATE repository_training_jobs SET status = 'failed', completed_at = ?, metadata_json = ? WHERE status = 'running'`
- Metadata includes `failure_reason: "training_worker_crashed"`, `failure_type: "worker_crash"`, `marked_failed_at`
- Returns `rows_affected` count
- Audit log emitted at `target: "audit.training"` for observability

### Spawn Gate Replacement
Replaced `spawn_disabled_due_to_fallback_error` with `circuit_breaker.is_tripped()` in:
- Health probe section: logs "Circuit breaker was tripped but healthy training worker detected"
- Spawn gate: `if !circuit_breaker.is_tripped() && managed_child.is_none() && ...`

## Deviations from Plan

None - plan executed exactly as written.

## Verification

```
SQLX_OFFLINE=1 cargo check -p adapteros-db -p adapteros-server --lib  # clean (0 warnings, 0 errors)
grep -n 'WorkerCircuitBreaker' background_tasks.rs                     # struct, impl, new(3, 300s)
grep -n 'mark_running_jobs_failed_worker_crash' training_jobs.rs       # DB method exists
grep -n 'circuit_breaker.record_crash' background_tasks.rs             # integrated in crash branches
```
