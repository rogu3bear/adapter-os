# AdapterOS Boot & Runtime System Overview

**PRD:** PRD-BOOT-01
**Status:** Implemented
**Last Updated:** 2025-11-25

## Purpose

This document provides a comprehensive overview of AdapterOS boot infrastructure, lifecycle states, runtime modes, and shutdown procedures.

---

## 1. Boot Infrastructure Map

### Current Boot Paths

**Primary Boot Script:**
- `scripts/run_complete_system.sh` - Complete system startup (API + UI)
  - System requirements check (Apple Silicon, memory, macOS version)
  - Model verification
  - Database initialization via `adapteros-orchestrator db migrate`
  - Port conflict detection
  - API server start (`cargo run -p adapteros-server-api`)
  - UI server start (`pnpm dev` in ui/)

**Secondary Scripts:**
- `bootstrap.sh` - Basic manual service start (deprecated pattern)
  - PostgreSQL start
  - Manual binary execution
  - Legacy PID file management

### Binary Entry Points

**Main Server Binaries:**
1. `adapteros-server/src/main.rs` (aos-cp binary)
   - Configuration loading
   - Deterministic executor initialization (HKDF-seeded from manifest)
   - Security preflight (PF/firewall checks)
   - Environment fingerprint drift detection
   - Database migration and recovery
   - ShutdownCoordinator initialization
   - Component registration (telemetry, federation, UDS metrics, git, policy watcher, alerts)
   - Background task spawning (status writer, TTL cleanup, heartbeat recovery)
   - Network binding (UDS or TCP based on `production_mode`)

2. `adapteros-cli/src/main.rs` (aosctl binary)
   - Unified CLI entry point
   - Git-style subcommands (adapter, node, registry, telemetry, etc.)
   - Model configuration precedence (CLI > ENV > defaults)
   - `serve` command for starting inference server
   - `preflight` command for pre-boot system checks

3. `adapteros-orchestrator/src/main.rs`
   - Database migration orchestration
   - Federation coordination
   - Cluster management

4. `adapteros-service-supervisor/src/main.rs`
   - Service lifecycle management (separate service on port 3301)
   - Start/stop/restart essential services
   - Service health monitoring

### Lifecycle Components

**ShutdownCoordinator** (`crates/adapteros-server-api/src/lifecycle.rs`, `crates/adapteros-server/src/shutdown.rs`)
- Graceful shutdown orchestration
- Component-specific timeouts (10s telemetry, 15s federation, 5s policy watcher)
- Dependency-based shutdown order
- Critical vs. partial failure classification
- Shutdown progress tracking with structured logging

**Lifecycle Hooks System** (`crates/adapteros-server-api/src/lifecycle.rs`)
- Phase-based callbacks: `BeforeStartup`, `AfterStartup`, `BeforeShutdown`, `AfterShutdown`
- Component registration and coordination
- Error recovery with panic handling

---

## 2. Lifecycle States (Boot Sequence)

### State Flow

```
stopped → booting → initializing-db → loading-policies → starting-backend →
loading-base-models → loading-adapters → ready → draining → stopping
```

### State Descriptions

| State | Description | Key Actions | Failure Handling |
|-------|-------------|-------------|------------------|
| **stopped** | Server not running | None | N/A |
| **booting** | Initial process startup | PID lock, config load, tracing init | Exit with error code |
| **initializing-db** | Database setup | Migration run, crash recovery, dev data seed | Exit if migrations fail |
| **loading-policies** | Policy verification | Hash watcher init, baseline load | Warn and continue |
| **starting-backend** | MLX/CoreML/Metal init | Runtime initialization, model loading | Fallback to next backend |
| **loading-base-models** | Base model loading | Manifest validation, executor seeding | Exit if manifest missing in prod mode |
| **loading-adapters** | Adapter warmup | Lifecycle manager init, heartbeat recovery | Continue with partial load |
| **ready** | Accepting requests | Handle HTTP/UDS requests | Per-request error handling |
| **draining** | Shutdown initiated | Reject new requests, track in-flight | Timeout after 30s |
| **stopping** | Component shutdown | Ordered component termination | Force abort after timeouts |

### Transition Events

**Startup Transitions:**
- `stopped → booting`: Process start (`cargo run` or binary exec)
- `booting → initializing-db`: Config loaded, executor seeded
- `initializing-db → loading-policies`: Migrations complete
- `loading-policies → starting-backend`: Policies validated
- `starting-backend → loading-base-models`: Backend initialized
- `loading-base-models → loading-adapters`: Base model loaded
- `loading-adapters → ready`: Network socket bound

**Shutdown Transitions:**
- `ready → draining`: SIGTERM/SIGINT received, `shutdown_signal()` triggered
- `draining → stopping`: All in-flight requests completed or timeout (30s)
- `stopping → stopped`: All components shut down, exit code returned

### Structured Logging

Each transition emits a structured log event with:
- `state`: Current lifecycle state
- `elapsed_ms`: Time since process start
- `component`: Component name (if applicable)
- `reason`: Transition reason (e.g., "manifest-validated", "shutdown-signal")

Example:
```rust
info!(
    state = "ready",
    elapsed_ms = 1234,
    bind_address = "127.0.0.1:8080",
    "Server ready to accept requests"
);
```

---

## 3. Runtime Modes

### Mode Definitions

| Mode | Egress | Auth | Telemetry | Event Signing | UDS/HTTP | Use Case |
|------|--------|------|-----------|---------------|----------|----------|
| **dev** | Allowed | Optional | Optional | No | HTTP + HTTPS | Local development |
| **staging** | Allowlist | Required | Required | No | HTTP + UDS | Pre-production testing |
| **prod** | Deny-all | Required | Required | Yes | UDS only | Production |

### Mode Configuration

**Precedence Order:** Env vars > DB settings > Config file > Default

**Environment Variables:**
- `AOS_RUNTIME_MODE` - Explicit mode override (dev/staging/prod)
- `AOS_SERVER_PORT` - HTTP port (dev/staging only)
- `AOS_PRODUCTION_MODE` - Boolean prod mode flag (backward compat)

**Config File:** (`configs/cp.toml`)
```toml
[server]
port = 8080
production_mode = true  # Maps to 'prod' mode
uds_socket = "/var/run/aos/aos.sock"  # Required in prod mode

[security]
require_pf_deny = true  # Firewall egress enforcement
jwt_mode = "eddsa"      # Required in prod mode
```

**Database Settings:** (`settings` table in aos-cp.sqlite3)
```sql
INSERT INTO settings (key, value) VALUES ('runtime_mode', 'prod');
```

### Policy Enforcement by Mode

**cp-egress-001 (Egress Policy):**
- **Dev:** No restrictions
- **Staging:** Check egress allowlist in DB (`egress_allowlist` table)
- **Prod:** Reject all network egress, enforce UDS-only

**cp-determ-002 (Determinism Policy):**
- **All modes:** Require HKDF-seeded executor
- **Prod:** Additional manifest validation (must exist and be valid)

**Event Signing:**
- **Dev/Staging:** Optional
- **Prod:** Required for all telemetry bundles

### Mode Resolution Logic

```rust
pub enum RuntimeMode {
    Dev,
    Staging,
    Prod,
}

impl RuntimeMode {
    pub fn resolve(config: &Config, db: &Db) -> Result<Self> {
        // 1. Check environment variable
        if let Ok(mode) = std::env::var("AOS_RUNTIME_MODE") {
            return mode.parse();
        }

        // 2. Check database setting
        if let Ok(Some(mode)) = db.get_setting("runtime_mode").await {
            return mode.parse();
        }

        // 3. Check config file (backward compat)
        if config.server.production_mode {
            return Ok(RuntimeMode::Prod);
        }

        // 4. Default to dev
        Ok(RuntimeMode::Dev)
    }
}
```

---

## 4. Shutdown & Drain Process

### Shutdown Triggers

1. **Signal-based:**
   - SIGTERM: Graceful shutdown (preferred)
   - SIGINT (Ctrl+C): Graceful shutdown
   - SIGHUP: Config reload (no shutdown)

2. **API-based:**
   - `POST /api/system/shutdown` (admin permission required)
   - `aosctl shutdown` CLI command

### Draining Phase

**Goal:** Complete in-flight requests before stopping components

**Duration:** 30 seconds (configurable via `drain_timeout_secs`)

**Actions:**
1. Set `draining` flag in `AppState`
2. Reject new HTTP requests with `503 Service Unavailable`
3. Track in-flight request count (`Arc<AtomicUsize>`)
4. Wait for count to reach 0 or timeout

**Middleware Implementation:**
```rust
async fn drain_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Check drain flag
    if state.is_draining() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // Increment in-flight counter
    let _guard = state.track_request();

    // Process request
    Ok(next.run(req).await)
}
```

### Component Shutdown Order

**Critical Path (must complete):**
1. **Telemetry** (10s timeout) - Flush buffers, sign bundles
2. **Federation** (15s timeout) - Complete cross-host signatures

**High Priority:**
3. **UDS Metrics** (5s timeout) - Close socket connections
4. **Git Daemon** (10s timeout) - Stop file watching

**Low Priority (abortable):**
5. **Policy Watcher** (5s timeout) - Stop hash validation
6. **Alert Watcher** (abort immediately)
7. **Background Tasks** (abort immediately) - Status writer, TTL cleanup, heartbeat recovery

### Audit Logging

**Shutdown Event:** Logged to `audit_logs` table
```json
{
  "user_id": "system",
  "action": "system.shutdown",
  "resource_type": "server",
  "status": "success",
  "metadata": {
    "trigger": "sigterm",
    "duration_ms": 1234,
    "components_stopped": 7,
    "failed_components": [],
    "in_flight_requests": 0
  }
}
```

**Component Events:** Each component logs its shutdown
```json
{
  "user_id": "system",
  "action": "component.shutdown",
  "resource_type": "telemetry",
  "status": "success",
  "metadata": {
    "duration_ms": 567,
    "buffers_flushed": 3,
    "events_written": 1234
  }
}
```

---

## 5. Boot Verification

### Pre-flight Checks (`aosctl preflight`)

**System Requirements:**
- Platform: macOS (Darwin)
- Architecture: Apple Silicon (M1/M2/M3/M4)
- Memory: 16GB minimum (48GB recommended for Qwen 2.5 7B)
- macOS Version: 14.0+ recommended

**Model Verification:**
- Directory exists: `$AOS_MLX_FFI_MODEL`
- Required files present: `config.json`, `tokenizer.json`, weight files
- Model size: >=3GB

**Database Verification:**
- SQLite file exists or can be created
- Schema version matches expected (via migrations)
- Write permissions in `var/` directory

**Backend Verification:**
- Metal: GPU availability check
- MLX: C++ library detection, model loading test
- CoreML: ANE detection, MLTensor availability (macOS 15+)

### Health Endpoints

**Readiness Probe:** `GET /readyz`
- Returns `200 OK` when server is `ready`
- Returns `503 Service Unavailable` during `booting`, `initializing-db`, or `draining`

**Liveness Probe:** `GET /healthz`
- Returns `200 OK` if server process is alive
- Includes component health checks

**Detailed Health:** `GET /healthz/all`
- Returns JSON with per-component status
```json
{
  "status": "healthy",
  "uptime_seconds": 1234,
  "lifecycle_state": "ready",
  "components": {
    "database": {"status": "healthy", "pool_active": 5},
    "telemetry": {"status": "healthy", "bundles_pending": 0},
    "backend": {"status": "healthy", "circuit_breaker": "closed"}
  }
}
```

---

## 6. CLI Commands

### Boot Commands

```bash
# Start full system (API + UI)
./scripts/run_complete_system.sh

# Start API only
./scripts/run_complete_system.sh --no-ui

# Pre-flight check
aosctl preflight

# Start with explicit mode
AOS_RUNTIME_MODE=prod cargo run -p adapteros-server-api

# Database migration
aosctl db migrate
```

### Shutdown Commands

```bash
# Graceful shutdown (via signal)
kill -TERM $(cat var/aos-cp.pid)

# Immediate shutdown (not recommended)
kill -KILL $(cat var/aos-cp.pid)

# CLI shutdown (API-based)
aosctl shutdown --wait  # Waits for completion

# Config reload (no shutdown)
kill -HUP $(cat var/aos-cp.pid)
```

### Status Commands

```bash
# Check server state
curl http://localhost:8080/readyz

# Detailed component health
curl http://localhost:8080/healthz/all | jq

# Check lifecycle state
aosctl status
```

---

## 7. UI Integration

### System Overview Page

**Displays:**
- Current runtime mode (dev/staging/prod badge)
- Lifecycle state (colored indicator)
- Uptime
- Component health (green/yellow/red)

**Actions:**
- Reload config (SIGHUP via API)
- Initiate graceful shutdown (requires confirmation)

**Implementation:** `ui/src/pages/System/Overview.tsx`

### Settings Page

**Mode Configuration:**
- Dropdown: dev/staging/prod
- Warning when switching modes
- Requires restart to take effect

**Shutdown Controls:**
- "Graceful Shutdown" button (admin only)
- Drain progress indicator
- In-flight request count

**Implementation:** `ui/src/pages/Settings/RuntimeSettings.tsx`

---

## 8. Duplicate Paths Analysis

### Boot Scripts

**Duplicates Found:**
- `scripts/run_complete_system.sh` vs. `bootstrap.sh`
  - **Resolution:** Use `run_complete_system.sh` (comprehensive checks)
  - **Action:** Mark `bootstrap.sh` as deprecated

### Configuration Loading

**Duplicates Found:**
- `adapteros-server/src/config.rs` vs. `adapteros-config` crate
  - **Resolution:** Server-specific config in `config.rs`, shared logic in `adapteros-config`
  - **Action:** Extract common patterns to `adapteros-config`

### Shutdown Logic

**Duplicates Found:**
- `adapteros-server/src/shutdown.rs` vs. `adapteros-server-api/src/lifecycle.rs`
  - **Resolution:** Keep both (server owns coordinator, API provides shared traits)
  - **Action:** Ensure `ShutdownCoordinator` is consistently used

---

## 9. Testing Strategy

### Unit Tests

**Lifecycle State Transitions:**
- `tests/lifecycle_transitions.rs`
- Test each state transition
- Verify structured log events

**Mode Resolution:**
- `tests/runtime_mode_resolution.rs`
- Test precedence order (env > db > config > default)
- Verify policy enforcement per mode

### Integration Tests

**Boot Sequence:**
- `tests/integration/boot_sequence.rs`
- Start server, verify lifecycle states
- Check health endpoints at each stage

**Graceful Shutdown:**
- `tests/integration/shutdown.rs`
- Send SIGTERM, verify drain phase
- Confirm components shut down in order
- Check audit logs for shutdown events

### End-to-End Tests

**Full Boot:**
- `tests/e2e/complete_boot.rs`
- Run `run_complete_system.sh`
- Verify API and UI are accessible
- Test inference request flow

---

## 10. References

- [CLAUDE.md](../CLAUDE.md) - Development standards
- [QUICKSTART.md](../../QUICKSTART.md) - Quick start guide
- [docs/LIFECYCLE.md](../LIFECYCLE.md) - Adapter lifecycle details
- [docs/RBAC.md](../RBAC.md) - Permission matrix
- [docs/TELEMETRY_EVENTS.md](../TELEMETRY_EVENTS.md) - Event catalog

---

**Last Reviewed:** 2025-11-25
**Next Review:** 2026-02-25 (quarterly)
