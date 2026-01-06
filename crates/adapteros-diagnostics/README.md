# AdapterOS Diagnostics

This crate defines the diagnostics event schema and provides utilities to emit,
buffer, and persist diagnostic events in a deterministic way.

## Quick Start

```rust
use adapteros_db::diagnostics::SqliteDiagPersister;
use adapteros_diagnostics::{
    spawn_diagnostics_writer, DiagEnvelope, DiagEvent, DiagLevel, DiagRunId,
    DiagSeverity, DiagnosticsConfig, DiagnosticsService, WriterConfig, DIAG_SCHEMA_VERSION,
};
use std::sync::Arc;
use tokio::time::Duration;

// Configure diagnostics
let config = DiagnosticsConfig {
    enabled: true,
    level: DiagLevel::Tokens,
    channel_capacity: 1000,
    max_events_per_run: 10_000,
    batch_size: 100,
    batch_timeout_ms: 500,
};

let (service, receiver) = DiagnosticsService::new(config.clone());
let persister = SqliteDiagPersister::new_arc(db.pool().clone());
let writer_config = WriterConfig {
    batch_size: config.batch_size,
    batch_timeout: Duration::from_millis(config.batch_timeout_ms),
    max_events_per_run: config.max_events_per_run,
};

let (_shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
spawn_diagnostics_writer(
    persister,
    receiver,
    service.run_tracker(),
    writer_config,
    shutdown_rx,
);

// Emit events
let run_id = DiagRunId::new_random();
service.start_run(&run_id);

let envelope = DiagEnvelope {
    schema_version: DIAG_SCHEMA_VERSION,
    emitted_at_mono_us: 1000,
    trace_id: "trace-id".to_string(),
    span_id: "span-id".to_string(),
    tenant_id: "default".to_string(),
    run_id,
    severity: DiagSeverity::Info,
    payload: DiagEvent::RunStarted {
        request_id: "req-123".to_string(),
        is_replay: false,
    },
};

service.emit(envelope)?;
```

## Design Notes

- Events are serialized deterministically (JCS) for hashing.
- Payloads avoid floating-point fields to keep determinism.
- The writer batches events for efficient persistence.

## Configuration Keys

Diagnostics keys are defined in `crates/adapteros-config/src/effective.rs` under `diag.*`.
Use `diag.level` to control verbosity (`off`, `errors`, `stages`, `router`, `tokens`).
