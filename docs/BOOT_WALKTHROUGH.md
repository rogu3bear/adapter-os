# adapterOS Boot Sequence Walkthrough

This document provides a narrative walkthrough of the adapterOS boot sequence, connecting the shell orchestration layer with the Rust control plane startup. It's designed to help new developers understand how the system initializes in under 30 minutes.

## Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           adapterOS Boot Flow                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ./start                                                                   │
│      │                                                                      │
│      ├─→ scripts/service-manager.sh start backend                           │
│      │      └─→ adapteros-server (port 8080)                                │
│      │            └─→ [BootState machine: 17 states]                        │
│      │                  │                                                   │
│      │                  ├─→ Init         → LoadingConfig                    │
│      │                  ├─→ InitDb       → ConnectingDb → Migrating         │
│      │                  ├─→ LoadPolicies → StartingBackend                  │
│      │                  ├─→ LoadModels   → DiscoveringWorkers               │
│      │                  ├─→ LoadAdapters → WarmingModels → Ready            │
│      │                  └─→ FullyReady   → [Serving]                        │
│      │                                                                      │
│      ├─→ UI served by backend static/ (no separate process)                │
│      │                                                                      │
│      └─→ Health check loop (/readyz)                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 1. Shell Orchestration Layer

### 1.1 The `./start` Script

**File:** `start` (project root, 1099 lines)

The canonical entry point for adapterOS. It:

1. **Displays banner** and version info
2. **Runs preflight checks** (disk space, memory, port availability)
3. **Delegates to service-manager.sh** for actual service startup
4. **Monitors health endpoints** until ready

Key environment variables:
| Variable | Default | Description |
|----------|---------|-------------|
| `AOS_SERVER_PORT` | 8080 | Control plane HTTP port (falls back to `server.port` in config) |
| `AOS_UI_PORT` | 3200 | UI dev server port |
| `AOS_HEALTH_TIMEOUT` | 120s | Max wait for `/healthz` |
| `AOS_READYZ_TIMEOUT` | 300s | Max wait for `/readyz` |
| `AOS_SKIP_PREFLIGHT` | false | Skip disk/memory checks |
| `AOS_VERIFY_CHAT` | 0 | Verify chat response after `/readyz` (requires auth or dev bypass) |

```bash
# Example startup
./start                          # Normal startup
AOS_DEV_NO_AUTH=1 ./start       # Dev mode without auth
```

### 1.2 Service Manager

**File:** `scripts/service-manager.sh` (800+ lines)

Manages individual service lifecycle:

```bash
# Start backend only
./scripts/service-manager.sh start backend

# UI: Backend serves static UI from static/. For dev with hot reload, run
#   cd crates/adapteros-ui && trunk serve
# (service-manager.sh start ui is a no-op; UI is served by backend)

# Stop all services
./scripts/service-manager.sh stop all

# Check status
./scripts/service-manager.sh status
```

**Preflight checks performed:**
- Disk space (≥10GB free)
- Available memory (≥8GB)
- Database integrity (`PRAGMA integrity_check`)
- Port availability

---

## 2. Control Plane Boot Sequence (main.rs)

**File:** `crates/adapteros-server/src/main.rs` (3,428 lines)

The Rust binary goes through 17 distinct boot states. Here's the timeline with line references:

### Phase 1: Configuration Loading (Lines 100-350)

**Boot State:** `Init` → `LoadingConfig`

```
[BOOT] Configuration loaded from configs/aos.toml
[BOOT] Environment overrides applied
[BOOT] Base model path validated
```

**What happens:**
1. **CLI args parsed** (line 100-180)
   - `--config`: Config file path (default: `configs/aos.toml`)
   - `--strict`: Enable strict mode (production requirements)
   - `--migrate-only`: Run migrations and exit

2. **Config file loaded** (line 181-256)
   - TOML configuration merged with environment variables
   - Environment variables take precedence (e.g., `AOS_SERVER_PORT`)

3. **Logging initialized** (line 268-283)
   - Tracing subscriber configured
   - Optional OpenTelemetry export enabled

4. **JWT mode derived** (line 285-298)
   - Debug builds: `dev_algo` (default: HS256)
   - Release builds: `prod_algo` (default: EdDSA)

**Expected timing:** ~0.1-0.3 seconds

---

### Phase 2: Boot State Manager Creation (Lines 359-380)

**Boot State:** `Booting`

```
[BOOT] Starting boot sequence with timeout: 300s
```

The `BootStateManager` is created without database connection:

```rust
let boot_state = BootStateManager::new();  // Line 360
boot_state.start().await;                  // Line 361
```

The entire boot sequence is wrapped in a timeout (default: 300 seconds):

```rust
let boot_result = tokio::time::timeout(boot_timeout, async { ... }).await;
```

---

### Phase 3: Security Initialization (Lines 389-435)

**Boot State:** `Booting` (continued)

```
[BOOT] Loading worker authentication keypair
[BOOT] Worker signing keypair loaded for CP->Worker authentication
```

**What happens:**
1. **PID lock acquired** (if `--single-writer` enabled)
2. **Ed25519 keypair loaded or generated** (line 396-435)
   - Stored at `var/keys/worker_signing.key`
   - Used for control plane → worker authentication
   - In strict mode, missing keypair is fatal

**Key log messages:**
```
INFO Worker signing keypair loaded (kid=abc123..., elapsed=50ms)
WARN Worker signing keypair not available, CP->Worker auth disabled (dev mode)
```

---

### Phase 4: Deterministic Executor Setup (Lines 525-673)

**Boot State:** `Booting` (continued)

```
[BOOT] Initializing deterministic executor
[BOOT] Derived deterministic executor seed from manifest
```

**What happens:**
1. **Manifest path resolved** (line 540-549)
   - Precedence: env (`AOS_MANIFEST_PATH`) → CLI → config → dev fallback

2. **Manifest loaded and validated** (line 558-615)
   - Hash computed using BLAKE3
   - In production mode, valid manifest is required

3. **Executor seed derived** (line 635-645)
   - HKDF-SHA256 with BLAKE3 manifest hash
   - Label: "executor"

4. **Global executor initialized** (line 657-659)

**Key invariant:** Same manifest → same executor seed → deterministic inference

---

### Phase 5: Security Preflight (Lines 674-829)

**Boot State:** `SecurityPreflight`

```
[BOOT] Running security preflight checks
[BOOT] Verifying environment fingerprint
```

**What happens:**
1. **PF firewall check** (line 697-731)
   - In production (`require_pf_deny=true`): Verifies egress is blocked
   - Dev bypass: `--skip-pf-check` flag

2. **Environment drift detection** (line 734-829)
   - Compares current device fingerprint to baseline
   - First run: Creates baseline at `var/baseline_fingerprint.json`
   - Subsequent runs: Checks for critical drift

**Failure modes:**
- PF not configured → Fatal in production
- Critical drift detected → Fatal in production, warning in dev

---

### Phase 6: Database Connection (Lines 831-998)

**Boot State:** `InitDb` → `ConnectingDb`

```
[BOOT] Connecting to database with storage backend
[BOOT] Database connected: var/aos-cp.sqlite3
```

**What happens:**
1. **Storage backend resolved** (line 839-846)
   - Options: `sql`, `dual`, `kv_primary`, `kv_only`

2. **Database connection established** (line 858-865)
   - SQLite with connection pool
   - KV store initialized if enabled

3. **Boot state upgraded with DB** (line 887)
   - Enables audit logging during boot

4. **Runtime session recorded** (line 966-996)
   - Config drift from previous session logged

**Expected timing:** ~0.5-1.0 seconds

---

### Phase 7: Migrations (Lines 1263-1301)

**Boot State:** `Migrating`

```
[BOOT] Running database migrations...
[BOOT] Running crash recovery checks...
```

**What happens:**
1. **Migrations executed** (line 1268)
   - Ed25519 signature verification on each migration
   - 220+ migrations applied on fresh database

2. **Crash recovery** (line 1282-1284)
   - Orphaned adapters cleaned up
   - Stale state reset

3. **Dev data seeded** (line 1287-1293)
   - Default tenant, admin user created

**Expected timing:** ~1-3 seconds (fresh), ~0.1s (incremental)

---

### Phase 8: Policy & Backend Initialization (Lines 1303-1400)

**Boot State:** `LoadingPolicies` → `StartingBackend` → `LoadingBaseModels`

```
[BOOT] Loading policies
[BOOT] Starting backend
[BOOT] Loading base models
```

**What happens:**
1. **Policy loading** (line 1304)
   - 30 policy packs loaded from DB

2. **Backend initialization** (line 1307)
   - MLX/CoreML/Metal backend prepared

3. **Base model loading** (line 1310-1313)
   - Priority models downloaded from HuggingFace (if enabled)

4. **API config created** (line 1316-1382)
   - Runtime configuration snapshot for handlers

---

### Phase 9: Background Task Spawning (Lines 1384-2432)

**Boot State:** `StartingBackend` (continued)

```
[BOOT] Spawning background tasks...
[BOOT] SIGHUP handler installed
[BOOT] Worker health monitor started
[BOOT] Status writer started (5s interval)
```

Background tasks spawned (with line references):
| Task | Line | Interval | Purpose |
|------|------|----------|---------|
| SIGHUP handler | 1391 | Signal | Config reload |
| UMA monitor | 1460 | 5s | Memory pressure detection |
| Federation daemon | 1540 | Continuous | Peer synchronization |
| Worker health monitor | 1801 | 30s | Latency/health checks |
| Status writer | 2094 | 5s | `var/run/status.json` |
| KV isolation scan | 2125 | 15min | Tenant isolation audit |
| KV alert monitor | 2161 | 5s | Drift/fallback alerts |
| WAL checkpoint | 2389 | 5min | SQLite WAL management |
| DB index monitor | 2437 | Background | Index health |
| Heartbeat recovery | 2456 | 5min | Stale adapter recovery |

**Expected timing:** ~0.5 seconds (spawn only, async execution)

---

### Phase 10: App State Assembly (Lines 1857-1960)

**Boot State:** `LoadingAdapters`

```
[BOOT] Loading adapters
[BOOT] Training service initialized
[BOOT] Registry initialized
```

**What happens:**
1. **AppState created** (line 1857-1873)
   - All services wired together

2. **Worker keypair attached** (line 1876-1878)

3. **Registry initialized** (line 1914-1957)
   - Adapter registry at `var/adapters/registry.db`

4. **Embedding model loaded** (line 1963-2055, if enabled)
   - RAG capabilities enabled

---

### Phase 11: Router Build & Boot Report (Lines 2516-2619)

**Boot State:** `LoadingAdapters` → preparing for `Ready`

```
[BOOT] Building router
[BOOT] Boot report written to var/run/boot_report.json
```

**What happens:**
1. **API routes built** (line 2522)
2. **UI routes merged** (line 2523-2529)
3. **Boot report generated** (line 2559-2619)
   - Config hash, bind address, port, worker key IDs
   - Written to `var/run/boot_report.json`

---

### Phase 12: Server Binding (Lines 2672-2799)

**Boot State:** `Ready` → `FullyReady`

```
[BOOT] Starting control plane on http://127.0.0.1:8080
[BOOT] UI available at http://127.0.0.1:8080/
[BOOT] API available at http://127.0.0.1:8080/api/
```

**Production mode (UDS):**
```rust
let listener = tokio::net::UnixListener::bind(&socket_path)?;
boot_state.ready().await;      // Line 2702
boot_state.fully_ready().await; // Line 2704
axum::serve(listener, app)...
```

**Development mode (TCP):**
```rust
let listener = tokio::net::TcpListener::bind(addr).await?;
boot_state.ready().await;      // Line 2780
boot_state.fully_ready().await; // Line 2782
axum::serve(listener, app)...
```

**Expected timing:** < 0.1 seconds (port bind)

---

## 3. Health Endpoints

During boot, health endpoints reflect current state:

| Endpoint | During Boot | After Ready |
|----------|-------------|-------------|
| `/healthz` | 200 OK | 200 OK |
| `/readyz` | 503 (state info) | 200 OK |

**Example `/readyz` during boot:**
```json
{
  "status": "not_ready",
  "boot_state": "Migrating",
  "checks": {
    "database": "ok",
    "worker": "pending"
  }
}
```

---

## 4. Expected Boot Timeline

| Phase | Typical Duration | Notes |
|-------|------------------|-------|
| Shell preflight | 1-2s | Disk/memory checks |
| Config loading | 0.1-0.3s | |
| Security init | 0.1-0.5s | CSPRNG may be slow |
| Executor setup | 0.1-0.2s | |
| Security preflight | 0.5-2s | PF check, drift detection |
| DB connection | 0.5-1s | Pool initialization |
| Migrations | 0.1-3s | Fresh vs incremental |
| Task spawning | 0.5s | Async spawn |
| Server binding | < 0.1s | |
| **Total** | **3-10s** | Depends on system state |

---

## 5. Log Messages to Expect

### Successful Boot (Abbreviated)
```
INFO  Configuration loaded from configs/aos.toml
INFO  Worker signing keypair loaded (kid=abc123)
INFO  Derived deterministic executor seed
INFO  Running security preflight checks
INFO  No environment drift detected
INFO  Connecting to database: var/aos-cp.sqlite3
INFO  Running database migrations...
INFO  Boot report written to var/run/boot_report.json
INFO  Starting control plane on http://127.0.0.1:8080
INFO  UI available at http://127.0.0.1:8080/
```

### Boot Failure Patterns

**DB connection failure:**
```
ERROR Database connection failed: unable to open database file
```

**Migration failure:**
```
ERROR [boot] code=E_MIG_INVALID: Database migrations failed
```

**Port in use:**
```
ERROR Port 8080 already in use. Stop existing process: lsof -ti:8080 | xargs kill
```

**Boot timeout:**
```
FATAL Boot timeout after 300 seconds. Boot was stuck in state: Migrating
```

---

## 6. Dev Mode Flags

| Flag | Effect | When to Use |
|------|--------|-------------|
| `AOS_DEV_NO_AUTH=1` | Bypass JWT auth | Local development |
| `--skip-pf-check` | Skip PF firewall check | Dev without PF configured |
| `--skip-drift-check` | Skip environment drift | Testing on multiple machines |
| `--migrate-only` | Run migrations and exit | CI/CD, maintenance |
| `--strict` | Enforce production requirements | Pre-deploy validation |

---

## 7. Related Documentation

- **Boot States:** See `docs/BOOT_PHASES.md` for state machine details
- **Troubleshooting:** See `docs/BOOT_TROUBLESHOOTING.md` for failure diagnosis
- **Configuration:** See `configs/aos.toml` for all config options
- **CLI:** Run `./aosctl --help` for command reference

---

## Quick Reference: Key Line Numbers in main.rs

| Component | Lines | Description |
|-----------|-------|-------------|
| CLI args | 100-180 | Command-line parsing |
| Config loading | 181-256 | TOML + env merge |
| Logging init | 268-283 | Tracing setup |
| Boot state | 359-380 | BootStateManager |
| Worker keypair | 389-435 | Ed25519 auth |
| Manifest/executor | 525-673 | Deterministic seeding |
| Security preflight | 674-829 | PF + drift checks |
| DB connection | 831-998 | SQLite + KV |
| Migrations | 1263-1301 | Schema updates |
| Background tasks | 1384-2432 | Async spawning |
| AppState | 1857-1960 | Service assembly |
| Router build | 2516-2529 | Axum routes |
| Boot report | 2559-2619 | JSON output |
| Server bind | 2672-2799 | TCP/UDS listen |
