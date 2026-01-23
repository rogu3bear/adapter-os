# AdapterOS Timeout Configuration

This document describes the timeout configurations used throughout AdapterOS for reference reliability and production hardening.

## Configuration Files

### cp.toml (Control Plane)

| Setting | Default | Description |
|---------|---------|-------------|
| `worker.safety.inference_timeout_secs` | 30 | Maximum time for a single inference request |
| `worker.safety.evidence_timeout_secs` | 5 | Timeout for evidence envelope generation |
| `worker.safety.router_timeout_ms` | 100 | Timeout for K-sparse router decisions |
| `worker.safety.policy_timeout_ms` | 50 | Timeout for policy pack evaluation |
| `worker.safety.circuit_breaker_timeout_secs` | 60 | Time before circuit breaker resets |
| `worker.safety.recovery_timeout_secs` | 10 | Timeout for deadlock recovery |

### TimeoutsConfig (ApiConfig.timeouts)

The `TimeoutsConfig` struct centralizes timeout settings for known hang points.
Configure via environment variables or TOML config file:

| Setting | Env Variable | Default | Description |
|---------|-------------|---------|-------------|
| `timeouts.adapter_load_timeout_secs` | `AOS_ADAPTER_LOAD_TIMEOUT_SECS` | 60s | LoadCoordinator adapter load timeout |
| `timeouts.training_job_timeout_secs` | `AOS_TRAINING_JOB_TIMEOUT_SECS` | 7200s (2h) | Maximum training job duration |
| `timeouts.db_acquire_timeout_secs` | `AOS_DB_ACQUIRE_TIMEOUT_SECS` | 30s | DB pool connection acquire timeout |

### StreamingConfig (ApiConfig.streaming)

| Setting | Default | Description |
|---------|---------|-------------|
| `streaming.inference_idle_timeout_secs` | 300s | SSE stream idle timeout (no tokens) |
| `streaming.inference_heartbeat_interval_secs` | 15s | Heartbeat interval for SSE keepalive |

### reference.env (Reference Mode Overrides)

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `AOS_DATABASE_TIMEOUT` | 30s | SQLite/Postgres connection timeout |
| `AOS_WORKER_SHUTDOWN_TIMEOUT` | 30s | Graceful shutdown timeout for workers |
| `AOS_DOWNLOAD_TIMEOUT_SECS` | 300 | Model/adapter download timeout |
| `AOS_TRAINING_JOB_TIMEOUT_SECS` | 7200s | Maximum training job duration (2 hours) |
| `AOS_INFERENCE_TIMEOUT_SECS` | 60 | Per-request inference timeout |
| `AOS_STREAMING_IDLE_TIMEOUT_SECS` | 300 | SSE stream idle timeout |
| `AOS_ADAPTER_LOAD_TIMEOUT_SECS` | 60 | LoadCoordinator adapter load timeout |
| `AOS_DB_ACQUIRE_TIMEOUT_SECS` | 30 | DB pool connection acquire timeout |

## Component-Specific Timeouts

### LoadCoordinator (Thundering Herd Protection)

The LoadCoordinator uses SingleFlight to prevent duplicate adapter loads. For reference reliability, use `load_or_wait_with_timeout()`:

```rust
// Read timeout from config
let load_timeout = state.config.read().unwrap().timeouts.adapter_load_timeout();

coordinator.load_or_wait_with_timeout(
    &adapter_id,
    load_timeout,
    load_fn,
).await
```

**Error Code**: `ADAPTER_LOAD_TIMEOUT`

### Training Job Timeout

Training jobs are wrapped with a configurable timeout to prevent indefinite execution:

```rust
// Timeout from environment variable
let timeout = training_job_timeout(); // Reads AOS_TRAINING_JOB_TIMEOUT_SECS

match tokio::time::timeout(timeout, training_future).await {
    Ok(result) => result,
    Err(_) => Err(AosError::Timeout { duration: timeout })
}
```

**Error Code**: `TRAINING_JOB_TIMEOUT`

### SSE Streaming Idle Timeout

Streaming inference connections are terminated if no tokens are produced within the idle timeout:

```rust
// In StreamState::next_event()
if self.is_idle() {
    return Some(self.error_event("STREAM_IDLE_TIMEOUT", "Stream idle timeout", true));
}
```

**Error Code**: `STREAM_IDLE_TIMEOUT` (retryable)

### Database Connection Pool

SQLite pool settings in `DbFactory`:
- `busy_timeout`: 30s (SQLite locking wait)
- `acquire_timeout`: Configurable via `AOS_DB_ACQUIRE_TIMEOUT_SECS` (default 30s)
- Statement cache: 50 statements per connection

PostgreSQL pool settings:
- `acquire_timeout`: 30s
- `idle_timeout`: 600s

**Error Code**: `DB_ACQUIRE_TIMEOUT`

## Error Codes Reference

| Error Code | Component | Retryable | Description |
|------------|-----------|-----------|-------------|
| `ADAPTER_LOAD_TIMEOUT` | LoadCoordinator | Yes | Adapter load exceeded timeout |
| `TRAINING_JOB_TIMEOUT` | TrainingService | No | Training job exceeded max duration |
| `STREAM_IDLE_TIMEOUT` | StreamingInfer | Yes | SSE stream idle for too long |
| `DB_ACQUIRE_TIMEOUT` | DbFactory | Yes | Pool connection acquire timed out |

## Error Surfacing

Timeout errors are surfaced to the UI via the toast notification system:

```rust
// In UI components, use error_with_details for detailed errors
notifications.error_with_details(
    "Request Timeout",
    "The operation timed out",
    &format!("Timeout after {} seconds", timeout.as_secs()),
);
```

Toast notifications for errors:
- Display for 15 seconds (vs 5 seconds for success/info)
- Include expandable details section
- "Copy Details" button for diagnostic info

## Debugging Timeouts

All timeout events are logged with structured fields for debugging:

```
WARN adapter_id="my-adapter" timeout_secs=60 Adapter load timed out - consider increasing AOS_ADAPTER_LOAD_TIMEOUT_SECS
WARN job_id="training-123" timeout_secs=7200 TRAINING_JOB_TIMEOUT: Training job exceeded maximum duration
WARN request_id="req-456" Stream idle timeout
WARN database_url="sqlite://..." acquire_timeout_secs=30 Database pool acquire timed out
```

Use `AOS_LOG_LEVEL=debug` to see more detailed timing information.
