# adapteros-deterministic-exec

Deterministic async executor for adapterOS with serial task execution, tick-based timing, and event logging for replay.

## Purpose

This crate provides a custom async executor that guarantees **deterministic execution** across runs and hosts. Unlike standard async runtimes where task scheduling is non-deterministic, this executor:

1. Executes tasks in strict FIFO order (serial, never concurrent)
2. Uses a logical **tick counter** instead of wall-clock time for timeouts
3. Logs all events (spawns, completions, timeouts) with cryptographic hashes
4. Supports identical replay from event logs

This is critical for adapterOS's auditability and air-gapped deployment requirements.

## Key Concepts

### Tick Ledger System

The executor tracks time via logical "ticks" rather than wall-clock time:

- Each task poll that returns `Pending` advances the tick counter
- Timeouts are expressed as tick counts, not durations
- The `GlobalTickLedger` persists tick events across hosts for cross-host consistency verification

```rust
// Tick-based timeout (not wall-clock)
let timeout = TickTimeout::new(task_id, 100, executor.tick_counter.clone());
if timeout.is_timeout() { /* 100 ticks elapsed */ }
```

### Deterministic Task IDs

Task IDs are derived from a global seed using BLAKE3:

```rust
let task_id = TaskId::from_seed_and_seq(&global_seed, sequence_number);
// Same seed + same sequence = same task ID
```

### Merkle Chain Verification

Events are chained via `prev_entry_hash` fields, enabling:
- Tamper detection in audit logs
- Cross-host consistency verification
- Replay validation

## Key Types

| Type | Purpose |
|------|---------|
| `DeterministicExecutor` | Main executor with spawn/run methods |
| `TaskId` | BLAKE3-hashed deterministic task identifier |
| `ExecutorEvent` | Logged events (spawn, complete, timeout, tick) |
| `GlobalTickLedger` | Persistent cross-host tick tracking |
| `TickTimeout` / `TickDelay` | Tick-based timing primitives |
| `ExecutorSnapshot` | Serializable state for crash recovery |

## Usage

```rust
use adapteros_deterministic_exec::{
    DeterministicExecutor, ExecutorConfig, spawn_deterministic
};

// Create executor with deterministic seed
let config = ExecutorConfig {
    global_seed: [42u8; 32],
    max_ticks_per_task: 1000,
    enable_event_logging: true,
    ..Default::default()
};
let executor = DeterministicExecutor::new(config);

// Spawn a task
executor.spawn_deterministic("my-task".into(), async {
    // Task work here
})?;

// Run to completion
executor.run().await?;

// Get event log for replay/audit
let events = executor.get_event_log();
```

## Modules

- **`global_ledger`**: Persistent tick ledger with Merkle chain verification
- **`multi_agent`**: Coordination for multi-agent deterministic execution
- **`cpu_affinity`**: Thread pinning for consistent scheduling
- **`channel`**: Deterministic async channels
- **`select`**: Deterministic select! macro replacement
- **`seed`**: HKDF-based seed derivation

## Enforcement Modes

The `EnforcementMode` controls how tick ledger policy violations are handled:

- `AuditOnly` (default): Log violations, continue execution
- `Warn`: Log warnings, continue execution
- `Enforce`: Fail on policy violations

## Crash Recovery

The executor supports snapshotting and restoration:

```rust
// Save state
let snapshot = executor.snapshot()?;

// Later: restore from snapshot
executor.restore(snapshot)?;
```

Note: Futures cannot be serialized. After restore, tasks must be re-spawned based on snapshot metadata.
