# Worker System Architecture

## Overview

Distributed inference: separate `aos-worker` processes handle ML workloads, communicating with control plane via UDS and HTTP.

---

## Worker Lifecycle

```
Pending -> Created -> Registered -> Healthy -> Draining -> Stopped
                                          \-> Error (terminal)
```

| State | Description |
|-------|-------------|
| `Pending` | Process starting, pre-registered in DB |
| `Created` | Socket bound, not yet registered |
| `Registered` | CP accepted registration |
| `Healthy` | Ready for inference |
| `Draining` | Graceful shutdown |
| `Stopped/Error` | Terminal |

---

## Key Components

### Worker Process (`crates/adapteros-lora-worker/`)

**Entry**: `src/bin/worker_modules/init.rs` → `run_worker()`

**Core Struct** (`src/lib.rs:1259`):
```rust
pub struct Worker<K: FusedKernels + StrictnessControl + Send + Sync + 'static> {
    manifest: ManifestV3,
    kernels: Arc<Mutex<K>>,
    router: Router,
    kv_cache: ...,
    health_monitor: Arc<HealthMonitor>,
    hotswap: Arc<HotSwapManager<K>>,
    inference_cancellations: Arc<InferenceCancelRegistry>,
}
```

### UDS Server (`src/uds_server.rs`)

- Socket path: `/var/run/aos/{tenant_id}/worker.sock`
- HTTP-over-UDS protocol
- Backpressure gate (configurable max concurrent)
- Circuit breaker (threshold: 5 failures)

**Endpoints**:
- `/inference` - Run inference
- `/health` - Health check
- `/training/cancel` - Cancel training
- `/maintenance` - Drain mode
- `/adapter/command` - Hot-swap commands

---

## Registration Flow

1. Worker calls `POST /v1/workers/register`
2. CP validates: plan exists, manifest_hash matches
3. CP returns: `heartbeat_interval_secs`, `kv_quota_bytes`
4. Worker notifies status via `POST /v1/workers/status`

**Retry**: Exponential backoff, circuit breaker (5 failures, 10 min deadline)

---

## API Routes (`handlers/workers.rs`)

| Route | Purpose |
|-------|---------|
| `POST /v1/workers/register` | Worker registration (internal) |
| `POST /v1/workers/status` | Status notifications (internal) |
| `GET /v1/workers` | List workers (protected) |
| `POST /v1/workers/spawn` | Spawn via node agent |
| `POST /v1/workers/{id}/stop` | Graceful stop |
| `POST /v1/workers/{id}/drain` | Enter drain mode |
| `GET /v1/workers/{id}/history` | Status transition history |
| `POST /v1/workers/fatal` | Fatal error reporting |

---

## Communication Patterns

| Direction | Protocol | Purpose |
|-----------|----------|---------|
| Worker → CP | HTTP POST | Registration, status, fatal errors |
| CP → Worker | UDS HTTP | Inference requests, drain signals |
| Worker → CP | UDS SSE | Streaming tokens |

---

## Security

- UDS socket permissions: `0o600`
- Ed25519 token validation with JTI replay defense
- Tenant isolation enforcement
- Request size limits: 16MB max

---

## Key Types (`adapteros-api-types/src/workers.rs`)

- `WorkerRegistrationRequest` / `WorkerRegistrationResponse`
- `WorkerStatusNotification`
- `WorkerCapabilities` (backend_kind, supports_streaming, gpu_backward)
- `WorkerHeartbeatRequest` / `WorkerHeartbeatResponse`

---

## Health Monitoring (`src/health.rs`)

```rust
pub struct HealthConfig {
    check_interval: Duration,
    max_response_time: Duration,
    max_memory_growth: u64,
    max_consecutive_failures: u32,
}
```

- Memory growth tracking with adaptive baseline
- Circuit breaker for fatal errors
- Telemetry integration
