# AdapterOS Boot Process PRD

**Reverse-Engineered Product Requirements Document**
**Version:** 2.0 | **Date:** 2025-12-01
**Copyright:** 2025 JKCA / James KC Auchterlonie

---

## Executive Summary

AdapterOS implements a **two-process architecture** with security isolation between the **Control Plane** (HTTP/API server) and **Worker** (inference engine). The boot sequence is designed around three harmonious principles:

1. **Determinism First** - HKDF-seeded execution ensures reproducible inference
2. **Security by Default** - Egress blocking, drift detection, signed migrations
3. **Graceful Degradation** - Circuit breakers, crash recovery, partial failures

---

## Architecture Overview

```
                      CONTROL PLANE (Port 8080)
                    ┌─────────────────────────────┐
                    │  adapteros-server           │
                    │  ├─ REST API (~250 routes)  │
                    │  ├─ JWT Auth + RBAC         │
                    │  ├─ Middleware Stack        │
                    │  └─ Background Workers      │
                    └───────────┬─────────────────┘
                                │
                           UDS Socket
                      var/run/worker.sock
                                │
                    ┌───────────▼─────────────────┐
                    │   WORKER PROCESS            │
                    │  adapteros-lora-worker      │
                    │  ├─ Model Weights (GPU)     │
                    │  ├─ K-Sparse Router         │
                    │  ├─ Inference Engine        │
                    │  └─ Zero Network Access     │
                    └─────────────────────────────┘
```

**Security Benefits:**
- Worker process has zero network access (UDS only)
- Control plane handles all external communication
- Privilege dropping to tenant-specific UID/GID
- Egress policy enforcement via macOS PF

---

## Boot State Machine

The control plane transitions through **10 discrete states**:

```
┌──────────┐    ┌─────────┐    ┌─────────────────┐    ┌────────────────┐
│ Stopped  │───►│ Booting │───►│ StartingBackend │───►│ LoadingModels  │
└──────────┘    └─────────┘    └─────────────────┘    └────────────────┘
                                                              │
┌───────────┐    ┌───────┐    ┌─────────────────┐    ┌───────▼────────┐
│ FullReady │◄───│ Ready │◄───│ LoadingAdapters │◄───│ LoadingPolicies│
└───────────┘    └───────┘    └─────────────────┘    └────────────────┘
      │                                                       ▲
      │          ┌──────────┐    ┌──────────┐                 │
      └─────────►│ Draining │───►│ Stopping │─────────────────┘
                 └──────────┘    └──────────┘            (crash recovery)
```

**Health Endpoint Behavior:**

| State | `/healthz` | `/readyz` |
|-------|------------|-----------|
| Stopped → LoadingPolicies | 200 OK | 503 Not Ready |
| Ready → FullReady | 200 OK | 200 Ready |
| Draining | 200 OK | 503 Draining |
| Stopping | 503 Stopping | 503 Stopping |

---

## Phase 1: Process Initialization

**Duration:** ~50ms | **State:** `Stopped → Booting`

### 1.1 Tracing Setup
```rust
// Entry: main.rs:140-146
tracing_subscriber::registry()
    .with(EnvFilter::try_from_default_env()
        .unwrap_or("aos_cp=info,aos_cp_api=info,tower_http=debug"))
    .init();
```

### 1.2 CLI Argument Parsing
| Argument | Default | Purpose |
|----------|---------|---------|
| `--config` | `configs/cp.toml` | Server configuration |
| `--skip-pf-check` | false | Bypass egress firewall check (not needed when `require_pf_deny=false` in dev config) |
| `--skip-drift-check` | false | Bypass environment fingerprint (dev auto-baselines; avoid in production) |
| `--manifest-path` | auto-discover | Model manifest for HKDF seeding |
| `--migrate-only` | false | Run migrations and exit |
| `--single-writer` | true | PID lock for single instance |

### 1.3 Configuration Validation
```rust
// Validates ALL AOS_* environment variables
adapteros_config::init_runtime_config()?;
```

**Precedence (highest → lowest):**
1. CLI arguments
2. `.env` file
3. Environment variables
4. Config file (`configs/cp.toml`)
5. Compiled defaults

### 1.4 PID Lock Acquisition
- **Production:** `/var/run/aos/cp.pid`
- **Development:** `var/aos-cp.pid`
- **Behavior:** Fails if another instance is running

---

## Phase 2: Security Preflight

**Duration:** ~200ms | **State:** `Booting`

### 2.1 Egress Policy Enforcement (if `require_pf_deny=true`)

**macOS:**
```bash
pfctl -s info  → Status: Enabled
pfctl -s rules → count "block out" (TCP + UDP required)
```

**Linux:**
```bash
iptables -L OUTPUT → DROP policy required
```

**Bypass:** `--skip-pf-check` (only for debugging; dev config sets `require_pf_deny=false` so this is not needed)

### 2.2 Environment Drift Detection

| Check | First Run | Subsequent Runs |
|-------|-----------|-----------------|
| Fingerprint | Create baseline, sign with Ed25519 | Compare to baseline |
| Hardware ID | Capture MAC, CPU, memory | Detect changes |
| Kernel Version | Record | Warn on drift |
| Critical Libraries | Hash | Block on tampering |

**Baseline Location:** `var/baseline_fingerprint.json`

**Development Behavior:** First run auto-creates the baseline; non-critical drift only warns. Production still blocks on critical drift—do not bypass checks in production.

---

## Phase 3: Deterministic Executor

**Duration:** ~100ms | **State:** `Booting → StartingBackend`

### 3.1 Manifest Loading
```yaml
# manifests/qwen7b-mlx.yaml
model_id: "qwen2.5-7b-instruct"
k_sparse: 4
quantization: "bf16"
```

### 3.2 HKDF Seed Derivation
```rust
let manifest_hash = blake3::hash(&manifest_bytes);
let global_seed = hkdf_expand(manifest_hash, b"executor", 32);
```

### 3.3 Executor Configuration
```rust
ExecutorConfig {
    global_seed,                           // HKDF-derived
    enable_event_logging: true,            // Audit trail
    max_ticks_per_task: 10_000,           // Bounded execution
    enforcement_mode: EnforcementMode::AuditOnly,
}
```

**Purpose:** Ensures identical inputs produce identical outputs across runs.

---

## Phase 4: Backend Initialization

**Duration:** ~500ms | **State:** `StartingBackend → LoadingModels`

### 4.1 MLX Runtime (if `multi-backend` feature)
```rust
adapteros_lora_mlx_ffi::mlx_runtime_init()?;
```

### 4.2 Backend Selection Chain
```
CoreML (ANE) → MLX (GPU) → Metal (fallback)
```

### 4.3 Priority Model Download (if `AOS_HF_HUB_ENABLED=true`)
- Downloads models listed in `AOS_PRIORITY_MODELS`
- Non-blocking with timeout
- Stores in `var/model-cache/`

---

## Phase 5: Database Bootstrap

**Duration:** ~300ms | **State:** `LoadingModels → InitializingDb → LoadingPolicies`

### 5.1 Connection Pool
```rust
let db = Db::connect("sqlite://var/aos-cp.sqlite3").await?;
// Default pool size: 5 connections
```

### 5.2 Migration Execution
- **119 migrations** (0001-0119)
- Each migration Ed25519 signed
- Signatures verified from `migrations/signatures.json`

### 5.3 Crash Recovery
```rust
db.recover_from_crash().await?;
// - Reset orphaned adapters to Unloaded/Cold
// - Clear stale heartbeats
// - Recover interrupted training jobs
```

### 5.4 Runtime Mode Resolution
```
AOS_RUNTIME_MODE > db.settings > config.production_mode > "dev"
```

| Mode | HTTP | Egress | Telemetry |
|------|------|--------|-----------|
| `dev` | Allowed | Allowed | Optional |
| `staging` | HTTP + UDS | Allowlist | Required |
| `prod` | UDS only | Deny all | Required + Signed |

---

## Phase 6: Service Registration

**Duration:** ~200ms | **State:** `LoadingPolicies`

### 6.1 Core Services

| Service | Initialization | Purpose |
|---------|----------------|---------|
| `PolicyHashWatcher` | Load cache, start 60s monitor | Tamper detection |
| `FederationDaemon` | Generate keypair, start 5m sync | Multi-node consensus |
| `UdsMetricsExporter` | Bind `var/run/metrics.sock` | Zero-network metrics |
| `TelemetryWriter` | Buffer (10K events, 50MB max) | Audit logging |

### 6.2 Background Workers

| Worker | Interval | Circuit Breaker |
|--------|----------|-----------------|
| Status Writer | 5s | No |
| TTL Cleanup | 5m | Yes (30m pause after 5 errors) |
| Heartbeat Recovery | 5m | Yes (30m pause after 5 errors) |
| SIGHUP Handler | On signal | No |

### 6.3 Optional Subsystems

| Subsystem | Condition | Purpose |
|-----------|-----------|---------|
| Git Integration | `git.enabled=true` | File change detection |
| Embedding Model | `embeddings` feature | RAG functionality |
| Event Bus | Plugins registered | Plugin communication |

---

## Phase 7: AppState Construction

**Duration:** ~50ms | **State:** `LoadingPolicies → LoadingAdapters`

### 7.1 Core Components (30+ fields)

```rust
AppState::new(db, jwt_secret, config, metrics_*, uma_monitor)
    .with_boot_state(boot_state)
    .with_runtime_mode(runtime_mode)
    .with_tick_ledger(tick_ledger)
    .with_plugin_registry(registry)
    .with_federation(federation_daemon)
    .with_policy_manager(policy_manager)
    // ... 10+ more optional components
```

### 7.2 SSE Signal Channels
- `training_signal_tx` (capacity: 1000)
- `discovery_signal_tx` (capacity: 1000)
- `contact_signal_tx` (capacity: 1000)

---

## Phase 8: Router & Middleware

**Duration:** ~100ms | **State:** `LoadingAdapters → Ready`

### 8.1 Route Categories (~250 endpoints)

| Category | Base Path | Auth |
|----------|-----------|------|
| Health | `/healthz`, `/readyz` | None |
| Auth | `/v1/auth/*` | Partial |
| Adapters | `/v1/adapters/*` | JWT |
| Training | `/v1/training/*` | JWT |
| Inference | `/v1/infer` | JWT |
| Chat | `/v1/chat/*` | JWT |
| Models | `/v1/models/*` | JWT |
| Policies | `/v1/policies/*` | JWT |
| Stacks | `/v1/adapter-stacks/*` | JWT |

### 8.2 Middleware Stack (outer → inner)

```
Request →
  api_logger_middleware        # Log request start
  drain_middleware             # Reject if draining
  request_tracking_middleware  # Count in-flight
  client_ip_middleware         # Extract IP
  request_id_middleware        # X-Request-ID
  versioning_middleware        # API version
  caching_middleware           # ETag/Cache-Control
  security_headers_middleware  # CSP, HSTS
  request_size_limit           # Body size
  rate_limiting_middleware     # RPM limits
  cors_layer                   # CORS
  compression_layer            # gzip/brotli
  trace_layer                  # Distributed tracing
    → Handler
  auth_middleware              # JWT validation
  context_middleware           # RequestContext
  audit_middleware             # Audit logging
  policy_enforcement           # RBAC check
← Response
```

---

## Phase 9: Network Binding

**Duration:** ~50ms | **State:** `Ready`

### 9.1 Production Mode (UDS Only)
```rust
UnixListener::bind("/var/run/aos/{tenant}/cp.sock")
```

### 9.2 Development Mode (TCP)
```rust
TcpListener::bind("127.0.0.1:8080")
// UI available at http://127.0.0.1:8080/
```

### 9.3 Ready Signal
```rust
boot_state.ready().await;
// /readyz now returns 200 OK
```

---

## Phase 10: Graceful Shutdown

**Trigger:** SIGINT (Ctrl+C) or SIGTERM

### 10.1 Drain Phase
```rust
boot_state.drain().await;
// drain_middleware rejects new requests
```

**Drain Monitoring:**
- Sample every 100ms
- Track: current, peak, average in-flight
- Default timeout: 30s

### 10.2 Shutdown Coordinator

| Component | Timeout | Priority |
|-----------|---------|----------|
| Telemetry | 10s | CRITICAL |
| Federation | 15s | High |
| UDS Exporter | 5s | Medium |
| Git Daemon | 10s | Medium |
| Policy Watcher | 5s | Low |
| Alert Watcher | Immediate | Low |
| Background Tasks | Immediate | Low |

### 10.3 Exit Codes
| Code | Meaning |
|------|---------|
| 0 | Clean shutdown |
| 1 | Critical failure (telemetry) |
| 2 | Partial failure (non-critical) |

---

## Worker Boot Sequence

**Separate Process** - Started independently or via supervisor

### Entry Point
```bash
aos-worker --manifest manifests/qwen7b-mlx.yaml \
           --model-path ./var/model-cache/models/qwen2.5-7b \
           --uds-path ./var/run/worker.sock
```

### Initialization Order
1. Tracing setup
2. CLI parsing
3. UDS path resolution (prod → dev fallback)
4. Manifest validation
5. Model/tokenizer path resolution
6. Backend creation (`create_backend_with_model`)
7. Telemetry writer initialization
8. Worker construction (loads model into GPU)
9. UDS server start (blocking)

### Environment Variables
| Variable | Default | Purpose |
|----------|---------|---------|
| `AOS_MODEL_PATH` | auto-discover | Model directory |
| `AOS_TOKENIZER_PATH` | `{model}/tokenizer.json` | Tokenizer |
| `AOS_WORKER_SOCKET` | `/var/run/aos/{tenant}/worker.sock` | UDS path |
| `AOS_TELEMETRY_DIR` | `./var/telemetry` | Telemetry output |

---

## Dependency Constraints

### Must Complete Before `Ready`

```
 1. Tracing ────────────────────────────────────────┐
 2. PID Lock ───────────────────────────────────────┤
 3. Config Validation ──────────────────────────────┤
 4. Security Preflight (PF, Drift) ─────────────────┤
 5. Deterministic Executor Seeding ─────────────────┤
 6. MLX Runtime Init ───────────────────────────────┤
 7. Database Connect + Migrate ─────────────────────┼──► Ready
 8. Crash Recovery ─────────────────────────────────┤
 9. Runtime Mode Resolution ────────────────────────┤
10. Policy Hash Watcher Start ──────────────────────┤
11. Federation Daemon Start ────────────────────────┤
12. UDS Metrics Exporter Bind ──────────────────────┤
13. AppState Construction ──────────────────────────┤
14. Router Build ───────────────────────────────────┘
```

### Runs After `Ready` (Concurrent)

- Status writer (5s loop)
- TTL cleanup (5m loop)
- Heartbeat recovery (5m loop)
- Git daemon (if enabled)
- Training job execution
- Inference requests

---

## Error Handling

### Boot Failures

| Phase | Error | Recovery |
|-------|-------|----------|
| PID Lock | Another instance running | Exit with message |
| PF Check | Egress not blocked | Exit (prod) or warn (dev) |
| Drift | Critical drift detected | Exit with fingerprint diff |
| Database | Connection failed | Retry 3x, then exit |
| Migration | Signature invalid | Exit (tamper detected) |

### Runtime Failures

| Component | Error | Recovery |
|-----------|-------|----------|
| Policy Watcher | Hash mismatch | Quarantine + alert |
| Federation | Peer unreachable | Skip peer, continue sweep |
| TTL Cleanup | 5 consecutive errors | Circuit breaker (30m pause) |
| Heartbeat | 5 consecutive errors | Circuit breaker (30m pause) |

### Graceful Degradation

- **MLX Runtime Failure:** Falls back to CoreML/Metal
- **RAG System Failure:** Continues without evidence retrieval
- **Telemetry Failure:** Logs warning, continues operation
- **Git Subsystem Failure:** Continues without file watching

---

## Configuration Reference

### Key Environment Variables

| Variable | Default | Phase |
|----------|---------|-------|
| `AOS_SERVER_PORT` | 8080 | Network Binding |
| `AOS_DATABASE_URL` | `sqlite://var/aos-cp.sqlite3` | Database |
| `AOS_MANIFEST_PATH` | auto-discover | Executor Seeding |
| `AOS_RUNTIME_MODE` | (resolved) | Runtime Mode |
| `AOS_MEMORY_HEADROOM_PCT` | 15 | Memory Management |
| `AOS_WORKER_SOCKET` | `/var/run/adapteros.sock` | UDS |

### TOML Configuration Sections

```toml
[server]
port = 8080
production_mode = false

[security]
require_pf_deny = true
jwt_secret = "..."

[paths]
artifacts_root = "var/artifacts"
adapters_root = "var/adapters/repo"
datasets_root = "var/datasets"
documents_root = "var/documents"

[alerting]
enabled = true
alert_dir = "var/alerts"

[git]
enabled = false
```

---

## Performance Characteristics

### Boot Time Breakdown

| Phase | Duration | Parallelizable |
|-------|----------|----------------|
| Config loading | <100ms | No |
| Security preflight | ~200ms | No |
| Executor seeding | ~100ms | No |
| Backend init | ~500ms | No |
| Database + migrations | ~300ms | No |
| Service registration | ~200ms | Partial |
| Router build | ~100ms | No |
| **Total cold boot** | **~1.5s** | |

### Memory Usage

**Control Plane:**
- Base: 50-100MB
- With telemetry: +20MB
- With federation: +10MB

**Worker Process:**
- Base model: 4-8GB (Qwen-7B)
- Per adapter: 100-500MB
- Total: 8-16GB for production

---

## Operational Procedures

### Standard Startup

```bash
# Unified entrypoint (delegates to scripts/service-manager.sh)
./start           # starts backend + UI, waits for health, worker optional

# Backend only
./start backend   # same backend path as service-manager, drift checks enforced

# Status / shutdown
./start status
./start down
```

Notes:
- Worker startup is optional; `./start` attempts it if binaries/manifests are present.
- Health waits are performed for backend/UI before reporting ready.
- No orchestrator migrate invocation; control plane handles migrations internally.

### Legacy Scripts (opt-in only; default is NO)
- `scripts/run_complete_system.sh` — deprecated shim; prompts (15s timeout, default No) before delegating to `./start`
- `scripts/bootstrap_integration_test.sh` — deprecated harness; guarded by the same prompt
- `scripts/bootstrap_with_checkpoints.sh` — deprecated resumable bootstrap; guarded by the same prompt
- Use these only when `./start` cannot run and you explicitly need the legacy flow; they bypass modern guardrails and health waits.

### Production Deployment

```bash
# Systemd service files
/etc/systemd/system/aos-cp.service
/etc/systemd/system/aos-worker@.service

# Start services
sudo systemctl start aos-worker@default
sudo systemctl start aos-cp
```

---

## Key Files

| Component | File | Purpose |
|-----------|------|---------|
| CLI Entry | `crates/adapteros-cli/src/main.rs` | Command parsing |
| Control Plane | `crates/adapteros-server/src/main.rs` | Boot orchestration |
| Worker | `crates/adapteros-lora-worker/src/bin/aos_worker.rs` | Inference engine |
| Boot States | `crates/adapteros-server-api/src/boot_state.rs` | State machine |
| Routes | `crates/adapteros-server-api/src/routes.rs` | API registration |
| Middleware | `crates/adapteros-server-api/src/middleware/` | Request pipeline |
| Config | `crates/adapteros-config/src/schema.rs` | Configuration |
| Database | `crates/adapteros-db/src/lib.rs` | Persistence |

---

## Harmony Principles

1. **Sequential Dependencies** - Each phase completes before the next begins
2. **Parallel Background** - Non-blocking tasks run concurrently after Ready
3. **Graceful Degradation** - Circuit breakers prevent cascade failures
4. **Observable State** - Boot state machine enables health monitoring
5. **Deterministic Core** - HKDF seeding ensures reproducibility
6. **Security Boundaries** - UDS isolation, egress blocking, signed artifacts

---

*Copyright JKCA | 2025 James KC Auchterlonie*

MLNavigator Inc 2025-12-06.
