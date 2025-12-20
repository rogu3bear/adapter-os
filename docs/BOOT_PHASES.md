# AdapterOS Boot Phases

This document formalizes the boot lifecycle phases and their transitions.

## Overview

AdapterOS uses a phase-gated boot sequence where each phase must complete before the next can begin. This ensures orderly initialization and provides clear failure points for debugging.

## State Diagram

```text
stopped
   |
   v
starting ─────────────────────────────────────────────┐
   |                                                   |
   v                                                   |
db-connecting ─────────────────────────────────────────┤
   |                                                   |
   v                                                   |
migrating ─────────────────────────────────────────────┤
   |                                                   |
   v                                                   |
seeding ───────────────────────────────────────────────┤
   |                                                   |
   v                                                   v
loading-policies ──────────────────────────────────> failed
   |
   v
starting-backend
   |
   v
loading-base-models
   |
   v
loading-adapters
   |
   v
worker-discovery
   |
   v
ready <──────────────────┐
   |                     |
   ├──────> degraded ────┘
   |
   v
fully-ready
   |
   v
draining
   |
   v
stopping
```

## Phase Descriptions

| Phase | Description | Duration (typical) | Can Fail To |
|-------|-------------|-------------------|-------------|
| `stopped` | Initial state before boot begins | - | `starting` |
| `starting` | PID lock acquisition, config loading | <100ms | `failed` |
| `db-connecting` | SQLite/KV database connection | <1s | `failed` |
| `migrating` | Run database migrations | 1-30s | `failed` |
| `seeding` | Dev fixtures, cache warmup | <5s | `failed` |
| `loading-policies` | Policy pack verification | <1s | `failed` |
| `starting-backend` | MLX/CoreML/Metal backend initialization | 1-10s | `failed` |
| `loading-base-models` | Manifest validation, model loading | 10-60s | `failed` |
| `loading-adapters` | Adapter warmup, heartbeat recovery | 5-30s | `failed` |
| `worker-discovery` | UDS socket binding, worker registration | <5s | `failed` |
| `ready` | Accepting requests, models may still be loading | - | `degraded`, `draining` |
| `fully-ready` | All priority models loaded and health-checked | - | `degraded`, `draining` |
| `degraded` | Non-critical dependency failure (can recover) | - | `ready` |
| `maintenance` | No new work accepted, in-flight continues | - | `draining` |
| `draining` | Reject new requests, track in-flight | <60s | `stopping` |
| `stopping` | Component shutdown (ordered termination) | <30s | - |
| `failed` | Terminal failure state | - | - |

## Transition Rules

1. **Forward Only**: Normal boot progresses forward through phases monotonically
2. **Fail Anytime**: Any non-terminal phase can transition to `failed`
3. **Degraded Recovery**: `degraded` can recover to `ready`
4. **Terminal States**: `stopping` and `failed` are terminal (no further transitions)
5. **No Backwards**: Cannot return to earlier boot phases once passed

### Valid Transitions

```text
stopped -> starting
starting -> db-connecting
db-connecting -> migrating
migrating -> seeding
seeding -> loading-policies
loading-policies -> starting-backend
starting-backend -> loading-base-models
loading-base-models -> loading-adapters
loading-adapters -> worker-discovery
worker-discovery -> ready
ready -> fully-ready
ready -> maintenance
ready -> draining
ready -> degraded
fully-ready -> maintenance
fully-ready -> draining
fully-ready -> degraded
degraded -> ready (recovery)
degraded -> draining
maintenance -> ready
maintenance -> draining
draining -> stopping
* -> failed (from any non-terminal state)
```

## Boot Report

The boot report is emitted at the end of a successful boot sequence:

1. **Single-line JSON log** at INFO level with tag `BOOT_REPORT`
2. **File** at `var/run/boot_report.json` with 0600 permissions

### Report Schema

```json
{
  "config_hash": "a1b2c3d4e5f67890",
  "config_schema_version": "1.0",
  "boot_phase_durations_ms": {
    "starting": 45,
    "db-connecting": 120,
    "migrating": 2500,
    "seeding": 150,
    "loading-policies": 80,
    "starting-backend": 3200,
    "loading-base-models": 12000,
    "loading-adapters": 800,
    "worker-discovery": 50,
    "ready": 10
  },
  "total_boot_time_ms": 18955,
  "enabled_features": ["debug", "macos", "aarch64"],
  "bind_addr": "127.0.0.1",
  "port": 8080,
  "auth_key_kids": ["jwt-abc123"],
  "worker_key_kids": ["worker-def456"],
  "build": {
    "git_sha": "abc123def",
    "build_time": "2025-01-15T10:00:00Z",
    "version": "0.11.0"
  },
  "generated_at": "2025-01-15T10:00:18Z"
}
```

### Security Considerations

The boot report is designed to be safe for logging:

- **No secrets**: No key material, only key IDs (derived hashes)
- **No environment variables**: Raw env values are not included
- **No full paths**: Avoids leaking tenant directory structure
- **Config hash only**: Full config is hashed, not exposed

## Worker Authentication

Workers authenticate requests using Ed25519-signed JWTs with short TTL.

### Key Files

| File | Permissions | Purpose |
|------|-------------|---------|
| `var/keys/worker_signing.key` | 0600 | Ed25519 private key (32 bytes) |
| `var/keys/worker_signing.pub` | 0644 | Ed25519 public key (32 bytes) |

### Token Format

```json
{
  "header": {
    "alg": "EdDSA",
    "typ": "JWT",
    "kid": "worker-abc12345"
  },
  "payload": {
    "iss": "control-plane",
    "aud": "worker",
    "wid": "worker-1",
    "iat": 1234567890,
    "exp": 1234567935,
    "jti": "req-uuid-here"
  }
}
```

### Claims

| Claim | Description |
|-------|-------------|
| `iss` | Issuer - always "control-plane" |
| `aud` | Audience - always "worker" |
| `wid` | Worker ID that this token is valid for |
| `iat` | Issued at (Unix timestamp) |
| `exp` | Expiration (Unix timestamp), typically iat + 45s |
| `jti` | JWT ID - unique request ID for replay defense |

### Replay Defense

Workers maintain an LRU cache of recently seen `jti` values:

- Cache size: ~1000 entries
- Tokens with duplicate `jti` within the TTL window are rejected
- Expired entries are naturally evicted by the LRU

## Implementation

The boot infrastructure is implemented in `crates/adapteros-boot/`:

```
adapteros-boot/
├── src/
│   ├── lib.rs                 # Crate root and re-exports
│   ├── phase.rs               # BootPhase enum and transitions
│   ├── lifecycle_builder.rs   # LifecycleBuilder pattern
│   ├── boot_report.rs         # JSON boot report generation
│   ├── worker_auth.rs         # Ed25519 token generation/validation
│   ├── services.rs            # ServiceRegistry for boot-time services
│   └── error.rs               # BootError and WorkerAuthError types
└── Cargo.toml
```

### Design Principles

1. **NO Axum dependencies**: The boot crate must not depend on Axum, tower, or hyper
2. **Phase-gated execution**: Each phase validates its predecessor completed
3. **Observable**: Phase timings and boot reports enable debugging
4. **Secure**: Worker auth uses Ed25519 with replay defense

## Usage

### Basic Boot Sequence

```rust
use adapteros_boot::{LifecycleBuilder, LifecycleConfig};

let config = LifecycleConfig::default();
let artifacts = LifecycleBuilder::new(config)
    .start().await?
    .db_connecting().await?
    .migrating().await?
    .seeding().await?
    .loading_policies().await?
    .init_crypto().await?
    .starting_backend().await?
    .loading_base_models().await?
    .loading_adapters().await?
    .worker_discovery().await?
    .ready().await?
    .build().await?;

// Use artifacts.worker_signing_keypair to generate tokens
// Use artifacts.boot_report for logging
```

### Worker Token Generation

```rust
use adapteros_boot::worker_auth::generate_worker_token;

let token = generate_worker_token(
    &signing_key,    // Ed25519 SigningKey
    "worker-1",      // Worker ID
    "req-123",       // Request ID (becomes jti)
    45,              // TTL in seconds
)?;
```

### Worker Token Validation

```rust
use adapteros_boot::worker_auth::validate_worker_token;
use lru::LruCache;
use std::num::NonZeroUsize;

let mut jti_cache: LruCache<String, i64> =
    LruCache::new(NonZeroUsize::new(1000).unwrap());

let claims = validate_worker_token(
    &token,
    &verifying_key,
    Some("worker-1"),  // Expected worker ID (optional)
    &mut jti_cache,
)?;
```

## Strict Mode

Strict mode (`AOS_STRICT=1`) enforces fail-closed behavior for all boot features. When enabled:

### Control Plane (adapteros-server)

```bash
# Enable strict mode
AOS_STRICT=1 cargo run --bin adapteros-server

# Or via CLI flag
cargo run --bin adapteros-server -- --strict
```

**Behavior:**
- **Worker keypair required**: Boot fails if `var/keys/worker_signing.key` cannot be loaded or generated
- **Boot report required**: Boot fails if `var/run/boot_report.json` cannot be written
- **No silent fallbacks**: All warnings become errors

### Worker (aos-worker)

```bash
# Enable strict mode
AOS_STRICT=1 cargo run --bin aos-worker

# Or via CLI flag
cargo run --bin aos-worker -- --strict
```

**Behavior:**
- **Public key required**: Boot fails if `var/keys/worker_signing.pub` is not found
- **Token validation required**: All requests must have valid Bearer tokens
- **No anonymous requests**: Missing or invalid tokens are rejected (401)

### Verification Matrix

| Feature | Non-Strict | Strict |
|---------|-----------|--------|
| Missing worker keypair (CP) | Warning, generates new | **Boot fails** |
| Missing public key (worker) | Warning, allows anon | **Boot fails** |
| Boot report write fails | Warning, continues | **Boot fails** |
| Invalid/missing token | Depends on config | **Request rejected** |

### Prerequisites

Before enabling strict mode:

```bash
# 1. Ensure directories exist
mkdir -p var/keys var/run
chmod 700 var/keys var/run

# 2. Start CP once (non-strict) to generate keypair
cargo run --bin adapteros-server
# This creates var/keys/worker_signing.key and var/keys/worker_signing.pub

# 3. Copy public key to workers
scp var/keys/worker_signing.pub worker-host:/path/to/worker/var/keys/

# 4. Now enable strict mode
AOS_STRICT=1 cargo run --bin adapteros-server
AOS_STRICT=1 cargo run --bin aos-worker
```

### Verification

```bash
# Check keypair exists
ls -la var/keys/worker_signing.*

# Check boot report
cat var/run/boot_report.json | jq .worker_key_kids

# Check logs for success messages
grep "Worker signing keypair loaded" logs/
grep "Worker public key loaded" logs/
```

## Code File Mapping

This section maps each boot phase to its implementation location in the codebase.

### Phase to File Reference

| Phase | Primary File | Line Range | Key Functions |
|-------|--------------|------------|---------------|
| `stopped` → `starting` | `main.rs` | 100-350 | CLI parsing, config loading |
| `starting` → `db-connecting` | `main.rs` | 831-866 | `DbFactory::create()` |
| `db-connecting` → `migrating` | `main.rs` | 1263-1279 | `db.migrate()` |
| `migrating` → `seeding` | `main.rs` | 1280-1296 | `recover_from_crash()`, `seed_dev_data()` |
| `seeding` → `loading-policies` | `main.rs` | 1303-1305 | `boot_state.load_policies()` |
| `loading-policies` → `starting-backend` | `main.rs` | 1306-1308 | `boot_state.start_backend()` |
| `starting-backend` → `loading-base-models` | `main.rs` | 1309-1313 | `download_priority_models()` |
| `loading-base-models` → `loading-adapters` | `main.rs` | 2515-2517 | `boot_state.load_adapters()` |
| `loading-adapters` → `worker-discovery` | `main.rs` | 1800-1823 | `WorkerHealthMonitor::with_defaults()` |
| `worker-discovery` → `ready` | `main.rs` | 2702, 2780 | `boot_state.ready()` |
| `ready` → `fully-ready` | `main.rs` | 2704, 2782 | `boot_state.fully_ready()` |

### Key Implementation Files

```
crates/
├── adapteros-server/
│   └── src/
│       └── main.rs              # Primary boot orchestration (3400+ lines)
│
├── adapteros-boot/
│   └── src/
│       ├── lib.rs               # BootStateManager, BootState enum
│       ├── phase.rs             # Phase transition logic
│       ├── boot_report.rs       # Boot report generation
│       └── worker_auth.rs       # Ed25519 worker auth
│
├── adapteros-server-api/
│   └── src/
│       ├── boot_state.rs        # BootState extensions
│       └── handlers/
│           └── health.rs        # /healthz, /readyz handlers
│
├── adapteros-db/
│   └── src/
│       ├── factory.rs           # DbFactory::create()
│       └── lifecycle.rs         # migrate(), recover_from_crash()
│
└── adapteros-config/
    └── src/
        ├── loader.rs            # ConfigLoader
        └── effective.rs         # init_effective_config()
```

### BootStateManager Implementation

**File:** `crates/adapteros-boot/src/lib.rs`

```rust
pub struct BootStateManager {
    state: Arc<RwLock<BootState>>,
    db: Option<Arc<Db>>,
    timings: Arc<RwLock<HashMap<BootState, Duration>>>,
}

impl BootStateManager {
    pub fn new() -> Self;
    pub async fn boot(&self);
    pub async fn init_db(&self);
    pub async fn load_policies(&self);
    pub async fn start_backend(&self);
    pub async fn load_base_models(&self);
    pub async fn load_adapters(&self);
    pub async fn ready(&self);
    pub async fn fully_ready(&self);
    pub fn current_state(&self) -> BootState;
    pub fn attach_db(self, db: Arc<Db>) -> Self;
}
```

---

## Development Mode Flags Matrix

This section documents all development bypass flags and their effects.

### Environment Variables

| Variable | Values | Effect | Security Impact |
|----------|--------|--------|-----------------|
| `AOS_DEV_NO_AUTH` | `1`, `true` | Bypass JWT authentication | **HIGH**: All endpoints accessible without auth |
| `AOS_DEV_JWT_SECRET` | string | Use custom JWT secret | Medium: Allows testing with known secret |
| `AOS_SKIP_PREFLIGHT` | `1`, `true` | Skip shell preflight checks | Low: Skips disk/memory validation |
| `AOS_LOG_LEVEL` | `trace`, `debug`, `info`, `warn`, `error` | Set log verbosity | None |
| `AOS_LOG_FORMAT` | `text`, `json` | Set log format | None |

### CLI Flags

| Flag | Effect | Production Allowed |
|------|--------|-------------------|
| `--strict` | Enable fail-closed behavior | Yes (recommended) |
| `--skip-pf-check` | Skip PF firewall check | **No** (debug builds only) |
| `--skip-drift-check` | Skip environment drift detection | **No** (debug builds only) |
| `--migrate-only` | Run migrations and exit | Yes |
| `--single-writer` | Acquire PID lock | Yes |
| `--generate-openapi` | Generate OpenAPI spec and exit | Yes |

### Production Mode Guards

When `production_mode=true` or `require_pf_deny=true`:

| Bypass Attempt | Result |
|----------------|--------|
| `--skip-pf-check` | **Boot fails** with error |
| `--skip-drift-check` | **Boot fails** with error |
| `AOS_DEV_NO_AUTH=1` | Ignored (auth still required) |
| Missing manifest | **Boot fails** with error |
| JWT secret < 32 chars | **Boot fails** with error |

### Config File vs Environment Priority

Environment variables **always override** config file values:

```
Priority (highest to lowest):
1. Environment variable (AOS_SERVER_PORT)
2. CLI argument (--config)
3. Config file value (server.port)
4. Default value (8080)
```

---

## Health Endpoint Behavior Per State

The health endpoints (`/api/healthz` and `/api/readyz`) return different responses based on boot state.

### /api/healthz (Liveness Probe)

**Purpose:** Kubernetes liveness check. Returns 200 if process is alive.

| Boot State | HTTP Status | Response |
|------------|-------------|----------|
| Any non-failed state | 200 OK | `{"status": "ok"}` |
| `failed` | 503 | `{"status": "failed", "reason": "..."}` |

### /api/readyz (Readiness Probe)

**Purpose:** Kubernetes readiness check. Returns 200 only when ready for traffic.

| Boot State | HTTP Status | Response |
|------------|-------------|----------|
| `stopped` | 503 | `{"ready": false, "boot_state": "stopped"}` |
| `starting` | 503 | `{"ready": false, "boot_state": "starting"}` |
| `db-connecting` | 503 | `{"ready": false, "boot_state": "db-connecting", "checks": {"database": "connecting"}}` |
| `migrating` | 503 | `{"ready": false, "boot_state": "migrating", "checks": {"database": "migrating"}}` |
| `seeding` | 503 | `{"ready": false, "boot_state": "seeding"}` |
| `loading-policies` | 503 | `{"ready": false, "boot_state": "loading-policies"}` |
| `starting-backend` | 503 | `{"ready": false, "boot_state": "starting-backend"}` |
| `loading-base-models` | 503 | `{"ready": false, "boot_state": "loading-base-models"}` |
| `loading-adapters` | 503 | `{"ready": false, "boot_state": "loading-adapters"}` |
| `worker-discovery` | 503 | `{"ready": false, "boot_state": "worker-discovery"}` |
| `ready` | **200 OK** | `{"ready": true, "boot_state": "ready"}` |
| `fully-ready` | **200 OK** | `{"ready": true, "boot_state": "fully-ready"}` |
| `degraded` | **200 OK** | `{"ready": true, "boot_state": "degraded", "degraded_reason": "..."}` |
| `maintenance` | 503 | `{"ready": false, "boot_state": "maintenance"}` |
| `draining` | 503 | `{"ready": false, "boot_state": "draining", "in_flight": 5}` |
| `stopping` | 503 | `{"ready": false, "boot_state": "stopping"}` |
| `failed` | 503 | `{"ready": false, "boot_state": "failed", "failure_reason": "..."}` |

### Kubernetes Configuration Example

```yaml
apiVersion: v1
kind: Pod
spec:
  containers:
    - name: adapteros
      livenessProbe:
        httpGet:
          path: /api/healthz
          port: 8080
        initialDelaySeconds: 5
        periodSeconds: 10
        failureThreshold: 3
      readinessProbe:
        httpGet:
          path: /api/readyz
          port: 8080
        initialDelaySeconds: 10
        periodSeconds: 5
        failureThreshold: 6  # Allow ~30s for boot
      startupProbe:
        httpGet:
          path: /api/readyz
          port: 8080
        initialDelaySeconds: 5
        periodSeconds: 10
        failureThreshold: 30  # Allow up to 5 minutes for initial boot
```

### Detailed Check Information

When boot state includes pending checks, the response includes details:

```json
{
  "ready": false,
  "boot_state": "loading-base-models",
  "checks": {
    "database": "ok",
    "worker": "pending",
    "base_model": "loading",
    "adapters": "pending"
  },
  "progress": {
    "models_loaded": 1,
    "models_total": 3,
    "adapters_warmed": 0,
    "adapters_total": 5
  }
}
```

---

## Related Documentation

- [BOOT_WALKTHROUGH.md](./BOOT_WALKTHROUGH.md) - Narrative boot sequence guide
- [BOOT_TROUBLESHOOTING.md](./BOOT_TROUBLESHOOTING.md) - Troubleshooting guide
- [ARCHITECTURE.md](./ARCHITECTURE.md) - Overall system architecture
- [SECURITY.md](./SECURITY.md) - Security model and threat analysis
- [OPERATIONS.md](./OPERATIONS.md) - Operational procedures
