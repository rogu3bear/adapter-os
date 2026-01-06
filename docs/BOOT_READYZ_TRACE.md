# AdapterOS Boot Sequence and Readiness Checks

> Comprehensive trace of boot phases and `/readyz` endpoint behavior.

## Boot Overview

AdapterOS implements a 12-phase deterministic boot sequence with a configurable timeout (default: 300s). Each phase emits metrics via `BootStateManager`.

**Entry Points:**
- `/healthz` - Quick boot state check (no dependency validation)
- `/readyz` - Full readiness validation (DB, workers, models)

---

## Boot Phases

### Phase 1: Configuration
**File:** `crates/adapteros-server/src/boot/config.rs`

**Actions:**
- Parse CLI arguments
- Load config file
- Validate runtime directory (`var/`)
- Harmonize env vars (precedence: `AOS_*` > `ADAPTEROS_*` > config)
- Initialize logging/tracing
- Set up panic hook (`var/logs/crash-*.json`)
- Validate CORS and JWT settings
- Initialize `BootStateManager`
- Extract boot timeout

**Failure Modes:**
| Error | Handling |
|-------|----------|
| Config not found | Exit code 1 |
| Runtime dir not writable | Fallback to `/tmp` |
| CORS validation fail (prod) | Exit code 1 |
| JWT secret missing (prod) | Exit code 1 |

---

### Phase 2: Security Initialization
**File:** `crates/adapteros-server/src/boot/security.rs`

**Actions:**
- Acquire PID lock (if `--single-writer`)
- Load/generate Ed25519 worker keypair (`var/keys/worker_signing.key`)
- Derive Key ID (KID) for CP→Worker auth
- Log effective config summary

**Failure Modes:**
| Error | Handling |
|-------|----------|
| PID lock held | Block startup |
| Keypair fail (strict) | Block startup |
| Keypair fail (dev) | Warn, continue without auth |

---

### Phase 3: Deterministic Executor
**File:** `crates/adapteros-server/src/boot/executor.rs`

**Actions:**
- Resolve manifest path (env > CLI > config > dev fallback)
- Create `ShutdownCoordinator` and `BackgroundTaskTracker`
- Load and validate manifest JSON
- Compute manifest hash (BLAKE3)
- Derive executor seed (HKDF-SHA256)
- Initialize global deterministic executor
- Initialize MLX C++ FFI (if `multi-backend` feature)

**Failure Modes:**
| Error | Handling |
|-------|----------|
| Manifest not found | Warn, use default seed |
| Invalid manifest | Warn, continue |
| MLX init fail | Continue with fallback |

---

### Phase 4: Security Preflight
**File:** `crates/adapteros-server/src/boot/security.rs`

**Actions:**
- PF (Packet Filter) rule validation (macOS/Linux)
  - Verify firewall blocks egress
  - Skip with `--skip-pf-check` (dev only)
- Environment fingerprint verification
  - Capture device fingerprint
  - Load baseline from `var/baseline_fingerprint.json`
  - Detect drift against policy
  - Auto-create baseline on first run

**Failure Modes:**
| Error | Handling |
|-------|----------|
| PF rule fail (prod) | Block startup |
| Critical drift (prod) | `PolicyViolation` error |
| Drift (dev) | Warn, continue |

---

### Phase 4b: Boot Invariants
**File:** `crates/adapteros-server/src/boot/invariants.rs`

**Actions:**
- Validate security invariants (auth bypass, attestation)
- Validate data integrity invariants (dual-write, storage)
- Validate lifecycle invariants (boot order, executor init)
- Produce `InvariantReport`

**Failure Modes:**
| Error | Handling |
|-------|----------|
| Fatal violation (prod) | Block startup |
| Non-fatal violation | Log warning, continue |

**Metrics:**
- `BOOT_INVARIANTS_CHECKED`
- `BOOT_INVARIANTS_VIOLATED`
- `BOOT_INVARIANTS_FATAL`
- `BOOT_INVARIANTS_SKIPPED`

---

### Phase 5: Database Connection
**File:** `crates/adapteros-server/src/boot/database.rs`

**Actions:**
- Determine storage backend (Sql | Dual | KvPrimary | KvOnly)
- Create `DbFactory` with connection pool
- Establish SQLite connection
- Validate atomic dual-write config
- Attach DB to `BootStateManager`
- Load effective config, detect drift
- Freeze config guards
- Initialize tick ledger

**Failure Modes:**
| Error | Handling |
|-------|----------|
| Connection fail | Block startup |
| Lock poisoned | Block startup |
| Config drift (prod) | Block startup |

**Readyz Check:** `SELECT 1` with 2000ms timeout

---

### Phase 6: Migrations
**File:** `crates/adapteros-server/src/boot/migrations.rs`

**Actions:**
- Run SQL schema migrations (if SQL backend)
- Verify migration signatures (`migrations/signatures.json`)
- Crash recovery (cleanup orphaned adapters)
- Seed dev data (if applicable)
- Seed base models from cache
- Bootstrap system tenant and core policies

**State Transitions:** `DbConnecting` → `Migrating` → `Seeding` → `LoadingPolicies`

**Failure Modes:**
| Error | Code | Handling |
|-------|------|----------|
| Migration fail | - | Block startup |
| Signature mismatch | `MIGRATION_SIG_FAILED` | Block startup |
| System tenant fail | `BOOT_BOOTSTRAP_FAILED` | Block startup |
| Model seeding fail | - | Warn, continue |

---

### Phase 9a: API Configuration
**File:** `crates/adapteros-server/src/boot/api_config.rs`

**Actions:**
- Build `ApiConfig` from server config
- Transform config types for API layer
- Set dev bypass flag (debug builds only)

---

### Phase 9b: Federation
**File:** `crates/adapteros-server/src/boot/federation.rs`

**Actions:**
- Create telemetry writer (bundles directory)
- Create policy hash watcher (60s interval)
- Load baseline policy hashes
- Start policy watcher background task
- Create federation manager
- Configure federation daemon (5min interval)
- Start federation daemon

**Dev Mode:** Skips background task spawning

---

### Phase 9c: Metrics
**File:** `crates/adapteros-server/src/boot/metrics.rs`

**Actions:**
- Initialize UDS metrics exporter (`var/run/metrics.sock`)
- Register default metrics
- Create `MetricsExporter` and UMA pressure monitor
- Prepare JWT secret (HMAC or EdDSA mode)

**Failure Modes:**
| Error | Handling |
|-------|----------|
| Directory creation fail | Block startup |
| JWT secret missing (prod HMAC) | Block startup |

---

### Phase 10a: Application State
**File:** `crates/adapteros-server/src/boot/app_state.rs`

**Actions:**
- Initialize worker health monitor (30s polling, 5000ms threshold)
- Create metrics collector and registry
- Set up dataset progress broadcast
- Initialize training service
- Build `AppState` with all dependencies
- Initialize manifest hash
- Initialize plugin registry
- Launch self-hosting agent (if enabled)
- Initialize adapter registry (`registry.db`)
- Initialize git subsystem (if enabled)

---

### Phase 10b: Background Tasks
**File:** `crates/adapteros-server/src/boot/background_tasks.rs`

**Tasks Spawned:**

| Task | Interval | Mode |
|------|----------|------|
| Status writer | 5s | All |
| WAL checkpoint | 5m | All |
| TTL cleanup | 5m | All |
| KV metrics alert | 5s | Prod only |
| Log cleanup | 24h | Prod only |
| Upload session cleanup | 1h | Prod only |
| Security cleanup | 1h | Prod only |
| Telemetry bundle GC | 6h | Prod only |

**Failure Handling:**
- Strict mode: spawn failure → boot fails
- Non-strict: `record_boot_warning()` for `/readyz` visibility

---

### Phase 11: Adapter Loading
**File:** `crates/adapteros-server/src/boot/finalization.rs`

**State Transitions:**
- `StartingBackend` → `LoadingBaseModels` → `LoadingAdapters` → `WorkerDiscovery`

---

### Phase 12: Finalization
**File:** `crates/adapteros-server/src/boot/finalization.rs`

**Actions:**
- Construct Axum router with API routes and UI assets
- Add root-level compat routes (`/readyz`, `/healthz`)
- Resolve server binding config
- Extract in-flight request counter
- Write boot report to `var/run/boot_report.json`

**Boot Report Contents:**
- `config_hash` (SHA-256)
- `bind_addr`, `port`
- `worker_key_id` (if available)

---

### Phase 13: Bind & Serve
**File:** `crates/adapteros-server/src/boot/server.rs`

**Actions:**
- Determine bind mode (UDS for prod, TCP for dev)
- Validate port availability (TCP mode)
- Bind socket, start accepting connections
- Set up graceful shutdown handler

**Failure Modes:**
| Error | Exit Code |
|-------|-----------|
| Port in use | 2 |
| Socket permission denied | 2 |
| Config error | 1 |

**Startup Banner:**
```
╔═══════════════════════════════════════════════════════════════╗
║             BOOT COMPLETE - AdapterOS Ready                   ║
╚═══════════════════════════════════════════════════════════════╝
```

---

## Boot State Machine

```
INITIAL: Stopped
         ↓
BOOT:    Starting → DbConnecting → Migrating → Seeding → LoadingPolicies
         → StartingBackend → LoadingBaseModels → LoadingAdapters
         → WorkerDiscovery → Ready
         ↓
OPERATIONAL: Ready → FullyReady
                   ↓
             Maintenance | Degraded | Draining
         ↓
SHUTDOWN: Draining → Stopping → Stopped
         ↓
TERMINAL: Failed (from any state)
```

---

## Health Endpoints

### `/healthz` (Liveness)

**Checks:** Boot state only (no dependencies)

**Response Codes:**
| State | Code |
|-------|------|
| Failed | 503 + failure code |
| Booting | 503 + current state |
| Ready/FullyReady | 200 |
| Degraded | 200 |
| Maintenance | 200 |
| Draining/Stopping | 200 |

**Response:**
```json
{
  "schema_version": "1.x.x",
  "status": "healthy|degraded|failed|booting|draining|maintenance",
  "version": "semver"
}
```

---

### `/readyz` (Readiness)

**Checks:**

1. **Boot State**
   - Failed → `worker_check.ok = false`
   - Degraded → ready with hint
   - Maintenance → `worker_check.ok = false`
   - Still booting → `worker_check.ok = false`
   - Ready/FullyReady → `worker_check.ok = true`

2. **Database Connectivity**
   - `SELECT 1` with timeout (default 2000ms)
   - Configurable: `server.health_check_db_timeout_ms`
   - Measures latency

3. **Worker Presence** (unless `skip_worker_check=true`)
   - `COUNT(*)` from workers table
   - Timeout: 2000ms (configurable)
   - `ok=false` if no workers

4. **Models Seeded**
   - `COUNT(*)` from models table
   - Timeout: 2000ms (configurable)
   - `ok=false` if no models
   - Checks active model mismatch

**Readiness Modes:**

| Mode | Behavior |
|------|----------|
| `Strict` | All checks required |
| `Relaxed` | Some checks skipped |
| `DevBypass` | Always returns 200 |

**Final Determination:**
```
Strict:    ready = db.ok && worker.ok && models.ok
Relaxed:   ready = db.ok && (relaxed checks)
DevBypass: ready = true
```

**Response:**
```json
{
  "ready": true,
  "checks": {
    "db": {"ok": true, "latency_ms": 5},
    "worker": {"ok": true, "latency_ms": 10},
    "models_seeded": {"ok": true, "latency_ms": 8}
  },
  "metrics": {
    "boot_phases_ms": [{"state": "Starting", "elapsed_ms": 100}],
    "db_latency_ms": 5
  },
  "boot_trace_id": "uuid",
  "phases": [...],
  "readiness_mode": {"mode": "strict"},
  "boot_warnings": [...]
}
```

---

## Boot Timeout

**Default:** 300 seconds (5 minutes)

**Configuration:** `server.boot_timeout_secs`

**Timeout Behavior:**
- Exit code: 10
- Error: "Boot sequence exceeded timeout"
- No graceful shutdown

---

## Configuration Reference

```toml
[server]
production_mode = false
boot_timeout_secs = 300
health_check_db_timeout_ms = 2000
health_check_worker_timeout_ms = 5000
health_check_models_timeout_ms = 15000
skip_worker_check = false
drain_timeout_secs = 30

[security]
require_pf_deny = false
jwt_secret = "REPLACE"
dev_bypass = false

[db]
path = "./aos.sqlite3"
storage_mode = "sql"
pool_size = 10
```

**Environment Variables:**
- `AOS_SERVER_PORT`
- `AOS_SERVER_HOST`
- `AOS_DATABASE_URL`
- `AOS_MANIFEST_PATH`
- `AOS_LOG_LEVEL`

---

## Boot to Ready Path (Summary)

1. **Config** → Logging ready
2. **Security** → PID lock, keypair
3. **Executor** → Determinism seeded
4. **Preflight** → Security gated
5. **Invariants** → Consistency verified
6. **Database** → Data access ready
7. **Migrations** → Schema initialized
8. **API Config** → Dependencies ready
9. **Federation/Metrics** → Background services
10. **AppState** → Full state assembled
11. **Background Tasks** → Services running
12. **Finalization** → Router ready
13. **Bind** → **READY FOR REQUESTS**

**Typical Time:** 5-30 seconds
