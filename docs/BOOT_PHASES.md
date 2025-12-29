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

The health endpoints (`/healthz` and `/readyz`) return different responses based on boot state.

### /healthz (Liveness Probe)

**Purpose:** Kubernetes liveness check. Returns 200 if process is alive.

| Boot State | HTTP Status | Response |
|------------|-------------|----------|
| Any non-failed state | 200 OK | `{"status": "ok"}` |
| `failed` | 503 | `{"status": "failed", "reason": "..."}` |

### /readyz (Readiness Probe)

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
          path: /healthz
          port: 8080
        initialDelaySeconds: 5
        periodSeconds: 10
        failureThreshold: 3
      readinessProbe:
        httpGet:
          path: /readyz
          port: 8080
        initialDelaySeconds: 10
        periodSeconds: 5
        failureThreshold: 6  # Allow ~30s for boot
      startupProbe:
        httpGet:
          path: /readyz
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

## Codebase Ingestion Pipeline

The codebase ingestion pipeline provides deterministic adapter training from source repositories.
This integrates with the boot lifecycle for automated adapter registration and management.

### Pipeline Phases

```text
source-resolution ──> codegraph-extraction ──> dataset-generation ──> training ──> packaging ──> preflight ──> alias-swap ──> registration
       │                                              │                              │           │                            │
       ▼                                              ▼                              ▼           ▼                            ▼
  [CommitMetadata]                            [Canonical Storage]              [.aos Manifest]  [Gating]              [AdapterRegistration]
  branch/commit                               content-addressable             scope metadata   readiness              repo_path tracking
```

| Phase | Description | Determinism Control | Set 31 Enhancement |
|-------|-------------|---------------------|---------------------|
| Source Resolution | Local path discovery or remote git clone | Git commit SHA captured | Full CommitMetadata with author, date, message |
| CodeGraph Extraction | Symbol parsing via adapteros-codegraph | Sorted symbol ordering | - |
| Dataset Generation | Q&A training pairs from symbols | Content hash computed | Canonical storage layout |
| Training | LoRA adapter training with seed control | Deterministic seed from content | - |
| Packaging | .aos file creation with metadata | BLAKE3 hash for reproducibility | scope_* and scan_roots fields |
| Preflight | Adapter validation before promotion | Gated alias swap | Full readiness validation suite |
| Alias Swap | Atomic promotion to production alias | Copy-then-rename atomicity | gate_alias_swap() enforcement |
| Registration | Database registration with provenance | Linked to commit SHA | repo_path for scan root tracking |

### Repository Slug Generation

Repository slugs provide URL-safe, normalized identifiers for tracking codebases across the ingestion pipeline.

#### Slug Normalization (`normalize_repo_slug`)

The `normalize_repo_slug` function in `crates/adapteros-orchestrator/src/code_ingestion.rs` converts repository names into canonical slugs:

1. **Trim whitespace**: Leading/trailing whitespace removed
2. **Lowercase**: All characters converted to lowercase
3. **Character replacement**: Non-alphanumeric characters become underscores
4. **Collapse underscores**: Consecutive underscores collapsed to single underscore
5. **Trim underscores**: Leading/trailing underscores removed
6. **Length limit**: Truncated to 64 characters maximum
7. **Fallback**: Empty/invalid inputs return `"repo"`

**Examples:**

| Input | Slug |
|-------|------|
| `AdapterOS-Core` | `adapteros_core` |
| `My Awesome Repo!` | `my_awesome_repo` |
| `__weird__` | `weird` |
| `""` (empty) | `repo` |

#### Repository Identifier Normalization (`normalize_repo_id`)

The `normalize_repo_id` function normalizes full repository identifiers (URLs or paths):

1. **Trim whitespace**: Leading/trailing whitespace removed
2. **Lowercase**: Case-insensitive matching
3. **Strip URL schemes**: Removes `https://`, `http://`, `git://`, `ssh://`
4. **Git SSH format**: Converts `git@host:path` to `host/path`
5. **Remove `.git` suffix**: Strips trailing `.git` extension
6. **Collapse slashes**: Multiple slashes become single slash
7. **Remove trailing slashes**: Trailing `/` removed
8. **Preserve `repo:` prefix**: Local identifiers keep their prefix

**Examples:**

| Input | Normalized |
|-------|------------|
| `https://github.com/org/repo.git` | `github.com/org/repo` |
| `git@github.com:org/repo` | `github.com/org/repo` |
| `GitHub.com/Org/Repo` | `github.com/org/repo` |
| `repo:my-project` | `repo:my-project` |

#### Usage in Codebase Ingestion

Repo slugs flow through the pipeline as follows:

```text
Repository Path/URL
       │
       ▼
┌──────────────────────┐
│ Extract repo_name    │  ← Directory name or URL path component
│ (e.g., "adapter-os") │
└──────────────────────┘
       │
       ▼
┌──────────────────────┐
│ normalize_repo_slug  │  ← Produces "adapter_os"
└──────────────────────┘
       │
       ├──────────────────────────────────────────────┐
       │                                              │
       ▼                                              ▼
┌──────────────────────┐                    ┌──────────────────────┐
│ Adapter ID           │                    │ Dataset Row Records  │
│ code.{slug}.{sha}    │                    │ repo_slug column     │
│ e.g., code.adapter_  │                    │                      │
│ os.a1b2c3d           │                    │                      │
└──────────────────────┘                    └──────────────────────┘
       │
       ▼
┌──────────────────────┐
│ Adapter Metadata     │
│ - repo_name (raw)    │
│ - repo_slug (norm)   │
│ - repo_commit        │
└──────────────────────┘
```

#### Database Storage

The `repo_slug` is stored in multiple locations:

| Table | Column | Purpose |
|-------|--------|---------|
| `codebase_dataset_rows` | `repo_slug` | Training sample provenance |
| `codebase_dataset_rows` | `repo_identifier` | Full normalized repo ID |
| Adapter metadata JSON | `repo_slug` | Adapter origin tracking |

This enables queries like:
- Find all training samples from a repository
- Locate adapters trained on specific codebases
- Track dataset lineage across repository updates

### Determinism Controls

The pipeline ensures reproducible training through multiple layers:

#### Content-Based Seeding

Training seeds are derived deterministically from:
- Git commit SHA
- Dataset content hash (BLAKE3)
- Training configuration parameters (rank, alpha, learning rate, batch size, epochs, hidden_dim)

```rust
fn derive_seed(commit_sha: &str, dataset_hash: &str, config: &TrainingConfig) -> u64 {
    let mut hasher = Hasher::new();
    hasher.update(commit_sha.as_bytes());
    hasher.update(dataset_hash.as_bytes());
    hasher.update(&config.rank.to_le_bytes());
    hasher.update(&config.alpha.to_le_bytes());
    // ... additional config fields
    u64::from_le_bytes(digest.as_bytes()[..8])
}
```

#### Seed Override

Explicit seed can be provided via `--seed` flag:

```bash
aosctl adapter ensure-codebase --seed 42
```

When set, overrides the derived seed for controlled reproducibility.

#### Dataset Hash Computation

Dataset hashes include all training sample components:
- Prompt text
- Response text
- Sample weight (positive/negative)
- Metadata key-value pairs

This ensures any change to training data results in a different hash.

### Dataset Records for Codebase Runs

Codebase ingestion creates dataset records for provenance tracking:

#### Dataset Lineage

Track relationships between datasets using CLI flags:

```bash
# Create a derived dataset from a parent
aosctl adapter ensure-codebase --parent-dataset-id ds-abc123

# Link to multiple source datasets
aosctl adapter ensure-codebase --derived-from ds-001,ds-002

# Add version and annotation
aosctl adapter ensure-codebase --version 2.1.0 --lineage-label "production-ready"

# Add custom lineage metadata
aosctl adapter ensure-codebase --lineage-metadata origin=github \
                               --lineage-metadata reviewed_by=team-a
```

#### Session Tracking

Correlate multiple operations within a workflow:

```bash
# Create adapter with session tracking
aosctl adapter ensure-codebase --session-name my-feature-branch

# Tag the session for filtering
aosctl adapter ensure-codebase --session-tags dev,experiment

# Use explicit session ID for correlation
aosctl adapter ensure-codebase --session-id 550e8400-e29b-41d4-a716-446655440000
```

Session IDs must be valid UUIDs when explicitly provided.

### .aos Metadata Registration

The `.aos` file format embeds comprehensive metadata for adapter registration:

#### Manifest Structure

```json
{
  "adapter_id": "tenant/domain/purpose/revision",
  "name": "Human-readable name",
  "version": "1.0.0",
  "rank": 16,
  "alpha": 32.0,
  "base_model": "meta-llama/Llama-3.1-8B",
  "target_modules": ["q_proj", "v_proj"],
  "category": "code",
  "tier": "warm",
  "weights_hash": "abc123...",
  "training_config": {
    "learning_rate": 0.0001,
    "batch_size": 4,
    "epochs": 3
  },
  "metadata": {
    "repo_name": "adapter-os",
    "repo_commit": "abc123def",
    "dataset_hash": "...",
    "scope_path": "code/rust/completion/v1"
  }
}
```

#### Metadata Extraction

The `AosMetadata` type extracts registration data from `.aos` files:

```rust
let metadata = AosMetadata::from_file("/path/to/adapter.aos").await?;
let params = AdapterRegistrationBuilder::new()
    .adapter_id(&metadata.adapter_id)
    .name(metadata.name.as_deref().unwrap_or("unnamed"))
    .hash_b3(&metadata.weights_hash.unwrap_or_default())
    .rank(metadata.rank as i32)
    .with_aos_metadata(metadata)
    .build()?;
```

#### MoE Support

For Mixture of Experts models, additional metadata tracks expert configuration:

```json
{
  "moe_config": {
    "num_experts": 8,
    "num_experts_per_token": 2,
    "lora_strategy": "routing_weighted_shared",
    "use_routing_weights": true
  }
}
```

### Branch and Commit Recording

The code ingestion pipeline captures comprehensive git metadata for traceability and reproducibility:

#### CommitMetadata Structure

```rust
struct CommitMetadata {
    sha: String,              // Full 40-character SHA
    short_sha: String,        // First 8 characters for display
    author_name: Option<String>,
    author_email: Option<String>,
    commit_date: Option<String>,      // ISO 8601 UTC
    commit_timestamp: Option<i64>,    // Unix epoch seconds
    message_summary: Option<String>,  // First line of commit message
    message_body: Option<String>,
    committer_name: Option<String>,
    committer_email: Option<String>,
    parent_shas: Vec<String>,
}
```

#### Recorded Fields

| Field | Source | Purpose |
|-------|--------|---------|
| `sha` | git HEAD | Exact commit for reproducibility |
| `short_sha` | Derived | Human-readable display |
| `author_name` | git commit | Attribution tracking |
| `commit_date` | git commit | Temporal ordering |
| `message_summary` | git commit | Context for training run |
| `parent_shas` | git commit | Lineage tracking |

#### Scope Metadata Overrides

CLI and CI/CD pipelines can override auto-detected values:

```bash
aosctl adapter ensure-codebase \
    --scope-repo my-project \
    --scope-branch feature/xyz \
    --scope-commit abc123def \
    --scope-scan-root src/lib \
    --scope-remote-url git@github.com:org/repo
```

### Dataset Storage Layout

The storage system uses a canonical content-addressable layout for datasets:

#### Path Scheme

```text
{datasets_root}/canonical/{category}/{hash_prefix}/{content_hash}/{version?}/{file_name}
```

Where:
- `category`: Dataset type (`codebase`, `metrics`, `synthetic`, `upload`, or custom)
- `hash_prefix`: First 2 characters of content hash (for directory sharding)
- `content_hash`: Full BLAKE3 hash (hex string)
- `version`: Optional version subdirectory
- `file_name`: The actual file name

#### Example Paths

```text
# Codebase dataset (unversioned)
var/datasets/canonical/codebase/a1/a1b2c3d4.../train.jsonl

# Metrics dataset (versioned)
var/datasets/canonical/metrics/f8/f8e7d6c5.../v2/samples.jsonl

# With tenant isolation
var/datasets/canonical/codebase/a1/a1b2c3d4.../tenants/tenant-123/train.jsonl
```

#### Dataset Categories

| Category | Description |
|----------|-------------|
| `codebase` | Derived from code ingestion |
| `metrics` | System metrics datasets |
| `synthetic` | Generated/synthetic datasets |
| `upload` | User-uploaded datasets |
| `Custom(name)` | Custom category |

#### Legacy Layout (Deprecated)

The legacy layout remains supported for backwards compatibility:

```text
{datasets_root}/files/{workspace_id?}/{dataset_id}/{versions/{version_id}?}/{file_name}
```

New datasets should use the canonical content-addressable layout.

### Adapter Registration with .aos Metadata

The `.aos` file format embeds comprehensive metadata for adapter registration:

#### Manifest Structure

```json
{
  "adapter_id": "tenant/domain/purpose/revision",
  "name": "Human-readable name",
  "version": "1.0.0",
  "rank": 16,
  "alpha": 32.0,
  "base_model": "meta-llama/Llama-3.1-8B",
  "target_modules": ["q_proj", "v_proj"],
  "category": "code",
  "tier": "warm",
  "weights_hash": "abc123...",
  "training_config": {
    "learning_rate": 0.0001,
    "batch_size": 4,
    "epochs": 3
  },
  "scope_repo": "adapter-os",
  "scope_branch": "main",
  "scope_commit": "abc123def",
  "scope_scan_root": "crates/",
  "scope_remote_url": "git@github.com:org/adapter-os",
  "scan_roots": [
    {"path": "crates/", "label": "main", "file_count": 150},
    {"path": "libs/", "label": "deps", "file_count": 30}
  ],
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "metadata": {
    "repo_name": "adapter-os",
    "repo_slug": "adapter_os",
    "dataset_hash": "...",
    "generator": "code_ingestion_pipeline"
  }
}
```

#### Scan Root Metadata

When training combines content from multiple directories, each scan root is recorded:

```rust
struct ScanRootMetadata {
    path: String,                    // Absolute or relative path
    label: Option<String>,           // Role (e.g., "main", "lib", "tests")
    file_count: Option<u64>,         // Files processed
    byte_count: Option<u64>,         // Total bytes ingested
    content_hash: Option<String>,    // BLAKE3 of scan root content
    scanned_at: Option<String>,      // ISO 8601 timestamp
}
```

#### Registration Builder

```rust
let params = AdapterRegistrationBuilder::new()
    .adapter_id(&metadata.adapter_id)
    .name(metadata.name.as_deref().unwrap_or("unnamed"))
    .hash_b3(&metadata.weights_hash.unwrap_or_default())
    .rank(metadata.rank as i32)
    .repo_id(Some(repo_id.to_string()))
    .commit_sha(Some(commit_sha.to_string()))
    .repo_path(Some(scan_root_path.to_string()))  // New: scan root tracking
    .with_aos_metadata(metadata)
    .build()?;
```

#### MoE Support

For Mixture of Experts models, additional metadata tracks expert configuration:

```json
{
  "moe_config": {
    "num_experts": 8,
    "num_experts_per_token": 2,
    "lora_strategy": "routing_weighted_shared",
    "use_routing_weights": true
  }
}
```

### Alias Change Gating (Preflight)

Before promoting an adapter to a production alias, preflight checks validate integrity.
This is enforced via the `gate_alias_swap()` function which must pass before any alias operation.

#### Gating Architecture

```text
                       ┌─────────────────────────────────────┐
                       │          Alias Swap Request          │
                       └────────────────┬────────────────────┘
                                        │
                                        ▼
                       ┌─────────────────────────────────────┐
                       │       Maintenance Mode Check         │
                       │   (var/.maintenance or env flag)     │
                       └────────────────┬────────────────────┘
                                        │
                        ┌───────────────┴───────────────┐
                        ▼                               ▼
                   [No Maintenance]               [In Maintenance]
                        │                               │
                        ▼                               ▼
               ┌────────────────┐              ┌────────────────┐
               │ Readiness Check │              │    BLOCKED     │
               └────────────────┘              └────────────────┘
                        │
                        ▼
         ┌──────────────────────────────────────────────┐
         │            Validation Checks                   │
         │  - Adapter exists in registry                  │
         │  - .aos file path set                          │
         │  - .aos file hash set                          │
         │  - Content hash (BLAKE3) set                   │
         │  - Lifecycle state allows activation           │
         │  - No conflicting active adapters              │
         │  - .aos file exists on disk                    │
         │  - File integrity (readable, valid size)       │
         │  - Tenant isolation respected                  │
         └──────────────────────────────────────────────┘
                        │
            ┌───────────┴───────────┐
            ▼                       ▼
       [All Pass]            [Any Fail]
            │                       │
            ▼                       ▼
    ┌───────────────┐      ┌───────────────┐
    │ Alias Swap OK │      │   BLOCKED     │
    └───────────────┘      │ with reasons  │
                           └───────────────┘
```

#### Validation Checks

| Check | Status | Failure Behavior |
|-------|--------|------------------|
| Adapter Exists | Required | Abort swap |
| AOS File Path | Required | Abort swap |
| AOS File Hash | Required | Abort swap |
| Content Hash (BLAKE3) | Warning | Continue with warning |
| Lifecycle State | Required | Abort swap |
| Repo/Branch Uniqueness | Required | Abort swap |
| System Mode | Required | Abort swap |
| Tenant Isolation | Required | Abort swap |
| File Exists | Required | Abort swap |
| File Size | Warning if < 256 bytes | Continue with warning |
| Header Valid | Required | Abort swap |

#### Block Reasons

The system provides specific block reasons for debugging:

| Reason | Description |
|--------|-------------|
| `AdapterNotFound` | Target adapter not in registry |
| `AdapterFileNotFound` | .aos file not on disk |
| `AdapterFileCorrupted` | File unreadable or malformed |
| `InvalidManifest` | Missing or invalid manifest |
| `MissingHash` | Required hash not set |
| `InvalidLifecycleState` | State doesn't permit activation |
| `ConflictingAdapters` | Another adapter active for same repo/branch |
| `MaintenanceMode` | System in maintenance mode |
| `TenantIsolationViolation` | Would cross tenant boundaries |

#### Preflight Workflow

```text
Build Adapter ──> Run Preflight ──┬──> [PASS] ──> Atomic Swap ──> Success
                                  │
                                  └──> [FAIL] ──> Abort ──> Error + Remediation
```

#### API Usage

```rust
use adapteros_cli::commands::preflight::{gate_alias_swap, gate_alias_swap_with_config};

// Simple gating (default config)
gate_alias_swap("my-adapter", &db).await?;

// Custom configuration
let config = AliasSwapGateConfig {
    force: false,
    skip_maintenance_check: false,
    skip_conflict_check: false,
    tenant_id: Some("tenant-123".to_string()),
    allow_training_state: true,
};
gate_alias_swap_with_config("my-adapter", &db, &config).await?;
```

#### Adapter File Readiness

For real-time gating during inference, use the lightweight file check:

```rust
use adapteros_cli::commands::preflight::require_adapter_file_ready;

// Quick validation before hot-swap
require_adapter_file_ready(Path::new("/path/to/adapter.aos"))?;
```

This performs:
1. File existence check
2. File readability check
3. Minimum size validation (256 bytes)

#### Force Override

In emergency situations, preflight can be skipped:

```bash
aosctl adapter ensure-codebase --skip-preflight
```

**Warning**: This bypasses validation and may promote invalid adapters.

#### Atomic Swap Mechanics

Alias swaps use copy-then-rename for atomicity:

1. Copy source `.aos` to `<target>.aos.tmp`
2. Atomic rename from `.aos.tmp` to final target
3. On POSIX systems, rename overwrites existing file atomically
4. On failure, cleanup temp file and abort

---

## Lifecycle Rules

Lifecycle rules provide automated policies for adapter and dataset management.

### Rule Types

| Type | Description | Example Action |
|------|-------------|----------------|
| `retention` | How long to keep entities | Keep 5 versions |
| `ttl` | Time-to-live expiration | Evict after 24 hours |
| `promotion` | Tier upgrade triggers | Promote to persistent |
| `demotion` | Tier downgrade triggers | Demote to cold |
| `garbage_collection` | Cleanup unused entities | Delete orphaned |
| `validation` | Integrity checks | Verify hash |
| `memory` | Memory pressure response | Evict on pressure |
| `version_retention` | Version limits | Keep last N versions |

### Rule Scopes

Rules apply at different hierarchy levels:

```text
System (global) ──> Tenant ──> Category ──> Adapter/Dataset (specific)
```

| Scope | Target Required | Example |
|-------|-----------------|---------|
| `system` | No | Global TTL policy |
| `tenant` | Yes (tenant_id) | Per-tenant retention |
| `category` | Yes (category) | Ephemeral adapter rules |
| `adapter` | Yes (adapter_id) | Specific adapter policy |
| `dataset` | Yes (dataset_id) | Specific dataset policy |

### Condition Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `equals` | Exact match | `lifecycle_state = retired` |
| `not_equals` | Inverse match | `tier != ephemeral` |
| `greater_than` | Numeric comparison | `hours_since_use > 24` |
| `greater_than_or_equal` | Inclusive comparison | `age_days >= 30` |
| `less_than` | Numeric comparison | `memory_bytes < 1000000` |
| `less_than_or_equal` | Inclusive comparison | `version <= 5` |
| `in` | Set membership | `category in [code, framework]` |
| `not_in` | Set exclusion | `tier not_in [persistent]` |
| `contains` | String contains | `name contains "test"` |
| `starts_with` | Prefix match | `adapter_id starts_with "dev/"` |

### Action Types

| Action | Description | Parameters |
|--------|-------------|------------|
| `evict` | Remove from hot tier | `reason` |
| `delete` | Permanent removal | `soft_delete` |
| `transition_state` | Change lifecycle state | `target_state` |
| `archive` | Move to archive | `archive_reason` |
| `notify` | Send notification | `channel`, `message` |
| `promote` | Upgrade tier | `target_tier` |
| `demote` | Downgrade tier | `target_tier` |

### Priority Evaluation

Rules are evaluated in priority order (highest first):

1. Higher priority rules checked first
2. First matching rule's actions are executed
3. Execution is recorded in audit log

### Default Rules

The system seeds default lifecycle rules on first boot:

```rust
db.seed_default_lifecycle_rules().await?;
```

Running again is idempotent - duplicates are not created.

---

## Related Documentation

- [BOOT_WALKTHROUGH.md](./BOOT_WALKTHROUGH.md) - Narrative boot sequence guide
- [BOOT_TROUBLESHOOTING.md](./BOOT_TROUBLESHOOTING.md) - Troubleshooting guide
- [ARCHITECTURE.md](./ARCHITECTURE.md) - Overall system architecture
- [SECURITY.md](./SECURITY.md) - Security model and threat analysis
- [OPERATIONS.md](./OPERATIONS.md) - Operational procedures
