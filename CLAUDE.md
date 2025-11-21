# AdapterOS Developer Guide

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.

**Purpose:** Quick reference for developers. For detailed architecture, see [docs/ARCHITECTURE_INDEX.md](docs/ARCHITECTURE_INDEX.md)
**Last Updated:** 2025-11-21
**Maintained by:** James KC Auchterlonie

---

## Standards & Conventions

### Code Style
```rust
// Standard Rust conventions: PascalCase (types), snake_case (functions/modules), SCREAMING_SNAKE_CASE (constants)
// Run: cargo fmt --all && cargo clippy --workspace -- -D warnings
```

### Documentation
```rust
/// Brief description. Args: `path` - description. Errors: `AosError::NotFound` if missing.
pub async fn load_from_path(path: &Path) -> Result<Adapter> { /* ... */ }
```

### Error Handling
```rust
use adapteros_core::{AosError, Result};

// Use Result<T>, never Option<T> for errors. Add context with map_err.
pub async fn load(&self, path: &Path) -> Result<Data> {
    std::fs::read(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => AosError::NotFound(format!("File not found: {}", path.display())),
        _ => AosError::Io(format!("Failed to read {}: {}", path.display(), e))
    })?;
    // ...
}
```

**Common `AosError` variants:** `PolicyViolation`, `DeterminismViolation`, `EgressViolation`, `IsolationViolation`, `Validation`, `Config`, `Io`, `Database`, `Crypto`, `Network`

### Logging (Use `tracing`, never `println!`)
```rust
use tracing::{info, warn, error, debug, trace};
info!(tenant_id = %tenant.id, adapter_id = %adapter.id, "Loading adapter");
```

**Log levels:** `trace!` (detailed debug) → `debug!` (dev info) → `info!` (general) → `warn!` (attention) → `error!` (action required)

**Telemetry:** See [docs/TELEMETRY_EVENTS.md](docs/TELEMETRY_EVENTS.md) for event catalog and metadata patterns

---

## Policy Packs

23 canonical policies enforced. Core policies:

| Policy | Purpose | Implementation |
|--------|---------|----------------|
| **Egress** | Zero network egress in production | `production_mode` requires `uds_socket` only |
| **Determinism** | Reproducible execution | All randomness seeded via HKDF (no `rand::thread_rng()`) |
| **Router** | K-sparse LoRA routing | Q15 quantized gates for adapter selection |
| **Evidence** | Audit trail with quality thresholds | Min relevance/confidence scores, source validation |
| **Telemetry** | Structured event logging | Canonical JSON events with signatures |
| **Naming** | Semantic adapter names | `{tenant}/{domain}/{purpose}/{revision}` format |

**Naming conventions:**
- Adapters: `tenant-a/engineering/code-review/r001`
- Stacks: `stack.production-env`
- Reserved: `system`, `admin`, `root`, `default`, `test` (tenants); `core`, `internal`, `deprecated` (domains)
- Max revision gap: 5

**Policy compliance checklist:**
- [ ] UDS-only in production, [ ] Seeded randomness, [ ] Q15 quantization, [ ] Evidence tracking, [ ] Canonical JSON telemetry, [ ] Semantic naming, [ ] Input validation, [ ] Tenant isolation, [ ] Typed errors

See `crates/adapteros-policy/src/packs/` for implementations.

---

## RBAC (5 Roles, 40 Permissions)

**Roles:** Admin (full), Operator (runtime ops), SRE (infra debug), Compliance (audit-only), Viewer (read-only)

**Permission matrix (condensed):**
- **All roles:** AdapterList, AdapterView, TrainingView, PolicyView, MetricsView
- **Admin only:** AdapterDelete, PolicyApply, PolicySign, TenantManage, NodeManage, AuditView
- **Operator+Admin:** AdapterRegister, AdapterLoad/Unload, TrainingStart/Cancel, InferenceExecute
- **SRE+Compliance+Admin:** AuditView
- **Compliance+Admin:** PolicyValidate

**Usage:**
```rust
use adapteros_server_api::permissions::{require_permission, Permission};
require_permission(&claims, Permission::AdapterRegister)?;
```

**Audit logging:**
```rust
use adapteros_server_api::audit_helper::{log_success, actions, resources};
log_success(&db, &claims, actions::ADAPTER_REGISTER, resources::ADAPTER, Some(&id)).await;
```

**Auth flow:** Login → JWT (Ed25519, 8hr TTL) → Middleware validation → Permission check → Audit log

**Query logs:** `GET /v1/audit/logs?action=adapter.register&status=success&limit=50`

**Detailed reference:** See [docs/RBAC.md](docs/RBAC.md) for complete permission matrix and audit logging

---

## Architecture Patterns (Quick Reference)

| Pattern | Location | Key Concept |
|---------|----------|-------------|
| **K-Sparse Routing** | `adapteros-lora-router` | Top-K adapters via Q15 gates |
| **Multi-Backend** | `adapteros-lora-worker/backend_factory.rs` | Metal/CoreML/MLX backends via `FusedKernels` trait |
| **CoreML Backend** | `adapteros-lora-kernel-coreml` | ANE acceleration (primary/production) |
| **MLX Backend** | `adapteros-lora-mlx-ffi` | Research, training (active) |
| **Metal Kernels** | `adapteros-lora-kernel-mtl` | Precompiled deterministic Metal kernels (fallback) |
| **Configuration** | `adapteros-config` | Precedence: CLI > Env > File > Defaults |
| **Memory Mgmt** | `adapteros-memory` | Auto-eviction maintains ≥15% headroom |
| **Hot-Swap** | `adapteros-lora-worker/adapter_hotswap.rs` | Live adapter replacement |
| **Content Addressing** | `adapteros-core/hash.rs` | BLAKE3 hashing for all artifacts |
| **Deterministic Exec** | `adapteros-deterministic-exec` | Serial FIFO task execution, no concurrency |
| **HKDF Seeding** | `adapteros-core/hash.rs` | Domain-separated seeds (router, dropout, sampling, etc.) |
| **Lifecycle Management** | `adapteros-lora-lifecycle` | State machine (Unloaded→Cold→Warm→Hot→Resident) |
| **Heartbeat Recovery** | `adapteros-lora-lifecycle`, `adapteros-db` | 5-min timeout, auto-reset stale adapters |
| **Determinism Attestation** | `adapteros-lora-kernel-api/attestation.rs` | Backend validation before serving |

**Detailed architecture:** See [docs/ARCHITECTURE_PATTERNS.md](docs/ARCHITECTURE_PATTERNS.md)

### Adapter Lifecycle States

```
Unloaded → Cold → Warm → Hot → Resident
    ↑                          ↓
    └──── (eviction) ──────────┘
```

**Transitions:** Promotion (activation % ↑), Demotion (activation % ↓ + timeout), Eviction (memory pressure + lowest %), Pinning (→ Resident)

**Usage:**
```rust
use adapteros_lora_lifecycle::LifecycleManager;
let manager = LifecycleManager::new_with_db(adapter_names, &policies, path, telemetry, k, db);
manager.record_router_decision(&selected).await?; // Auto-promote
manager.check_memory_pressure(total_mem, 0.85).await?; // Auto-evict
```

**Full state machine diagram:** See [docs/LIFECYCLE.md](docs/LIFECYCLE.md)

### Deterministic Execution

**Critical:** Seed derived from base model manifest hash via HKDF

```rust
use adapteros_core::{B3Hash, derive_seed};
use adapteros_manifest::ManifestV3;

let manifest = serde_json::from_str::<ManifestV3>(&std::fs::read_to_string(&cli.manifest_path)?)?;
manifest.validate()?;
let manifest_hash = manifest.compute_hash()?;
let global_seed = derive_seed(&manifest_hash, "executor");
init_global_executor(ExecutorConfig { global_seed, enable_event_logging: true, ..Default::default() })?;
```

**Details:** See [docs/DETERMINISTIC_EXECUTION.md](docs/DETERMINISTIC_EXECUTION.md) for HKDF hierarchy, global tick ledger, and multi-agent coordination

### .aos Archive Format
```
[0-3]   manifest_offset (u32 LE)
[4-7]   manifest_len (u32 LE)
[offset] manifest (JSON)
[offset] weights (safetensors)
```

Zero-copy loading with memory-mapped files → GPU VRAM direct transfer.

---

## Document Processing & Training

### Pipeline (5 Steps)

1. **Ingest:** `DocumentIngestor::new(opts, tokenizer).ingest_pdf_path(path)?`
2. **Generate:** `generate_training_data(&doc, &tokenizer, &config)?`
3. **Dataset:** `TrainingDatasetManager::new(db, path, tok).create_dataset_from_documents(req).await?`
4. **Train:** `MicroLoRATrainer::new(cfg)?.train(examples, adapter_id).await?`
5. **Package:** `AdapterPackager::new().package(weights, manifest)?` → `registry.register_adapter(...)?`

**Training strategies:** Identity (unsupervised), QuestionAnswer, MaskedLM

**Templates:**
- `general-code`: rank=16, alpha=32 (multi-language)
- `framework-specific`: rank=12, alpha=24

**Job tracking:** Pending → Running → Completed/Failed/Cancelled (progress %, loss, tokens/sec)

**Full pipeline diagram:** See [docs/TRAINING_PIPELINE.md](docs/TRAINING_PIPELINE.md)

---

## Database Schema (Core Tables)

| Table | Purpose | Key Fields |
|-------|---------|------------|
| `adapters` | Adapter metadata | id, hash, tier, rank, acl, activation_%, expires_at |
| `tenants` | Tenant isolation | id, uid, gid, isolation_metadata |
| `adapter_stacks` | Reusable combos | id, name, adapter_ids_json, workflow_type |
| `training_datasets` | Dataset metadata | id, hash_b3, validation_status |
| `training_jobs` | Job tracking | id, dataset_id, status, progress_pct, loss |
| `pinned_adapters` | Pin enforcement | tenant_id, adapter_id, pinned_until, reason, pinned_by |
| `audit_logs` | Immutable audit trail | user_id, action, resource, status, timestamp |

**Registry usage:**
```rust
use adapteros_registry::Registry;
let registry = Registry::open("./registry.db")?;
registry.register_adapter("id", &hash, "tier_1", rank, &["tenant_a"])?;
let allowed = registry.check_acl("id", "tenant_a")?;
```

### Migration Management

**Canonical Migration Directory:** `/migrations/` (root)
**Migration Count:** 80 migrations (0001-0080, complete sequence)
**Signing:** All migrations signed with Ed25519 (`migrations/signatures.json`)
**Status:** PRD-01 conflict resolution completed (2025-11-19)

**Key Migrations:**
- **0035** - Tick ledger federation columns
- **0045** - .aos file support
- **0055** - Model backend fields (merged: metal + last_error)
- **0060** - Pinned adapters table with TTL support
- **0061** - Semantic naming taxonomy
- **0062** - RBAC audit logs
- **0063** - Dashboard configuration
- **0064** - Adapter stacks
- **0065** - Heartbeat mechanism
- **0066** - Stack versioning (telemetry correlation)
- **0067** - Multi-tenancy for adapter stacks
- **0068** - Metadata normalization (version, lifecycle_state)
- **0069** - Plugin tenant enables
- **0070** - Routing decisions telemetry
- **0071** - Lifecycle version history (adapter/stack audit trail)
- **0072** - Tenant snapshots (renumbered from crate 0066)
- **0073** - Index hash tracking (renumbered from crate 0067)
- **0074** - Legacy index migration (renumbered from crate 0068)
- **0075** - Lifecycle state transition triggers
- **0076** - Golden run promotions
- **0077** - Adapter performance tracking
- **0078** - Federation consensus ledger
- **0079** - Stack versioning extensions
- **0080** - Tenant adapter stack isolation

**Creating New Migrations:**
```bash
touch migrations/NNNN_description.sql
# Write SQL (use SQLite-compatible types: TEXT, INTEGER, REAL, BOOLEAN)
./scripts/sign_migrations.sh
cargo test -p adapteros-db schema_consistency_tests
```

**Full schema reference:** See [docs/DATABASE_REFERENCE.md](docs/DATABASE_REFERENCE.md)

---

## Adapter Pinning & TTL

### Pinning System

**Purpose:** Prevent critical adapters from being evicted or deleted

**Quick usage:**
```rust
use adapteros_db::Db;

// Pin adapter (optional TTL)
db.pin_adapter(tenant_id, adapter_id, Some("2025-12-31 23:59:59"), "production-critical", "ops@example.com").await?;

// Unpin adapter
db.unpin_adapter(tenant_id, adapter_id).await?;

// Check pin status
let is_pinned = db.is_pinned(tenant_id, adapter_id).await?;
```

### TTL (Time-To-Live) Enforcement

**Purpose:** Automatic cleanup of ephemeral/temporary adapters

**Quick usage:**
```rust
use adapteros_db::AdapterRegistrationParams;

let params = AdapterRegistrationParams {
    adapter_id: "temp-adapter".to_string(),
    expires_at: Some("2025-02-15 23:59:59".to_string()),
    ..Default::default()
};
db.register_adapter_with_params(&params).await?;
```

**Three-Tier Enforcement:**
1. Database query (`find_expired_adapters()`)
2. Background cleanup loop (5-min interval)
3. Lifecycle manager integration (evict expired first)

**Full reference:** See [docs/PINNING_TTL.md](docs/PINNING_TTL.md) for pinning system, TTL enforcement, and lifecycle integration

---

## Streaming Architecture

**Modes:**
1. Batch (complete response)
2. Streaming (chat-style SSE, token-by-token)

```rust
use adapteros_api::streaming::{StreamingInferenceRequest, streaming_inference_handler};
let request = StreamingInferenceRequest {
    prompt, model, max_tokens, temperature, stream: true, adapter_stack,
    ..Default::default()
};
let stream = streaming_inference_handler(State(api_state), Json(request)).await;
// Format: data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"content":"Hi"},...}]}
// Final: data: [DONE]
```

**Features:** Keep-alive, client disconnect detection

---

## Common Patterns

### Database Access
```rust
query("SELECT * FROM adapters WHERE tenant_id = ?").bind(&tenant_id).fetch_all(&db.pool).await
    .map_err(|e| AosError::Database(format!("Query failed: {}", e)))?;
```

### Async Task Spawning
```rust
spawn(async move { if let Err(e) = do_work().await { error!(error = %e, "Task failed"); } });
```

### Production Mode Enforcement
```rust
if config.server.production_mode {
    if config.server.uds_socket.is_none() { return Err(AosError::Config("Requires uds_socket".into())); }
    if config.security.jwt_mode.as_deref() != Some("eddsa") { return Err(AosError::Config("Requires jwt_mode='eddsa'".into())); }
    if !config.security.require_pf_deny { return Err(AosError::Config("Requires require_pf_deny=true".into())); }
}
```

---

## Anti-Patterns (Avoid)

| Anti-Pattern | Issue | Fix |
|--------------|-------|-----|
| TODO comments | No completion plan | Complete implementation or explicit error |
| Placeholder logic | Fake functionality | Real implementation |
| `Option<T>` for errors | Loses error context | Use `Result<T, AosError>` |
| `println!` logging | Not queryable | Use `tracing` macros |
| Unsafe in app crates | Security risk | Isolate to FFI crates (Metal) |
| Blocking in async | Blocks executor | Use `tokio::time::sleep`, not `std::thread::sleep` |
| Unlocked kernel refs | Won't compile | `self.kernels.lock().await` before use |
| Unvalidated datasets | Training fails | Check `validation_status = 'valid'` |

See `docs/DEPRECATED_PATTERNS.md` for historical examples.

---

## Multi-Backend Architecture

**Strategy:** CoreML-first (ANE production), MLX-active (research/training), Metal-fallback (legacy)

| Backend | Status | Determinism | Use Case | Crate |
|---------|--------|-------------|----------|-------|
| **CoreML** | **Primary** (MLTensor API implemented, Swift bridge complete) | **Guaranteed (ANE)** | ANE acceleration, production | `adapteros-lora-kernel-coreml` |
| **MLX** | **Active** (95% test pass rate) | **HKDF-seeded** | Research, training | `adapteros-lora-mlx-ffi` |
| **Metal** | Fallback | Guaranteed | Legacy, non-ANE systems | `adapteros-lora-kernel-mtl` |

**Note:** macOS 26 compatibility investigation needed for CoreML backend runtime behavior.

**Backend Selection:**
```rust
use adapteros_lora_worker::backend_factory::{BackendChoice, create_backend};

// Production: CoreML (ANE acceleration, guaranteed determinism)
let backend = create_backend(BackendChoice::CoreML { model_path: None })?;

// Research/Training: MLX (HKDF-seeded determinism)
let backend = create_backend(BackendChoice::Mlx { model_path })?;

// Fallback: Metal (legacy, non-ANE systems)
let backend = create_backend(BackendChoice::Metal)?;
```

### Swift Bridge (MLTensor)

The CoreML backend includes a Swift bridge for MLTensor operations (macOS 15+):

- **Location:** `crates/adapteros-lora-kernel-coreml/swift/CoreMLBridge.swift`
- **Purpose:** Access modern MLTensor API for GPU-accelerated tensor operations
- **Requirements:** Xcode 15+ with `swiftc` in PATH

**Runtime Dispatch Behavior:**
- macOS 15+: Uses MLTensor path (2x speedup, GPU/ANE tensor operations)
- macOS 14: Falls back to MLMultiArray path (CPU-based)
- Detection: `swift_coreml_supports_mltensor()` returns availability at runtime

**Build Requirements:**
```bash
# Swift bridge compiles automatically during cargo build
# Requires: swiftc (from Xcode Command Line Tools)
xcode-select --install  # If swiftc not found
```

**FFI Patterns:** See [docs/OBJECTIVE_CPP_FFI_PATTERNS.md](docs/OBJECTIVE_CPP_FFI_PATTERNS.md) for memory-safe Rust ↔ Objective-C++/Swift patterns.

**Full details:**
- [docs/ADR_MULTI_BACKEND_STRATEGY.md](docs/ADR_MULTI_BACKEND_STRATEGY.md) - Backend selection rationale
- [docs/COREML_INTEGRATION.md](docs/COREML_INTEGRATION.md) - CoreML setup & ANE optimization
- [docs/ADDING_NEW_BACKEND.md](docs/ADDING_NEW_BACKEND.md) - Template for new backends

---

## Key Subsystems (Locations)

| Subsystem | Crate | Purpose |
|-----------|-------|---------|
| Router | `adapteros-lora-router` | K-sparse adapter selection |
| Backend Factory | `adapteros-lora-worker/backend_factory.rs` | Multi-backend creation & attestation |
| CoreML Backend | `adapteros-lora-kernel-coreml` | ANE acceleration (primary/production) |
| MLX Backend | `adapteros-lora-mlx-ffi` | Research, training (active) |
| Metal Kernels | `adapteros-lora-kernel-mtl` | Deterministic GPU kernels (fallback) |
| Policy Engine | `adapteros-policy` | 23-pack policy enforcement |
| Memory Mgmt | `adapteros-memory` | Auto-eviction, headroom maintenance |
| Lifecycle | `adapteros-lora-lifecycle` | State machine (Unloaded→Resident) |
| Hot-Swap | `adapteros-lora-worker/adapter_hotswap.rs` | Live adapter replacement |
| Deterministic Exec | `adapteros-deterministic-exec` | Serial FIFO task execution |
| HKDF | `adapteros-core/hash.rs` | Domain-separated seeding |
| Training | `adapteros-lora-worker/training/` | Trainer, quantizer, packager |
| Registry | `adapteros-registry` | SQLite WAL mode, adapter/tenant mgmt |
| Pinning & TTL | `adapteros-db/src/pinned_adapters.rs` | Pin/unpin API, TTL enforcement |
| Web UI | `adapteros-server-api`, `ui/` | React/TypeScript REST API |

---

## REST API Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/adapters` | GET | List adapters with lifecycle state |
| `/api/adapters/load` | POST | Load adapter into lifecycle |
| `/api/adapters/swap` | POST | Hot-swap adapters |
| `/api/router/config` | GET | Router configuration |
| `/api/training/start` | POST | Start training job |
| `/api/training/datasets` | POST | Create dataset from documents |
| `/api/training/jobs/:id` | GET | Get job status |
| `/api/chat/completions` | POST | Chat inference (streaming/batch) |
| `/api/adapter-stacks` | GET/POST | List/create adapter stacks |
| `/v1/audit/logs` | GET | Query audit logs (Admin/SRE/Compliance) |

**UI integration:** See [docs/UI_INTEGRATION.md](docs/UI_INTEGRATION.md)

---

## Quick Reference

### Build Commands
```bash
cargo build --release              # Production build
cargo test --workspace             # All tests
cargo fmt --all && cargo clippy --workspace -- -D warnings  # Lint
make build test check clean        # Makefile shortcuts
make metal                         # Compile Metal shaders
make ui / ui-dev                   # React build / dev server
make installer                     # macOS installer
```

### Database
```bash
./target/release/aosctl db migrate
./target/release/aosctl init-tenant --id default --uid 1000 --gid 1000
sqlite3 var/aos-cp.sqlite3 "SELECT * FROM tenants;"
# Reset (dev only): rm var/aos-cp.sqlite3 && ./target/release/aosctl db migrate
```

### Testing
```bash
cargo test test_adapter_loading    # Specific test
cargo test --workspace -- --nocapture  # With output
cargo test --test integration_tests    # Integration tests only
```

### Debugging
```bash
cargo check --workspace --message-format=short
cargo clippy --workspace -- -W dead_code
cargo udeps                        # Unused dependencies
```

---

## Known Build Issues (Alpha v0.01-1)

**Status:** 40+ crates building successfully

**Backend Implementation Status:**
- `adapteros-lora-kernel-coreml` - MLTensor API implemented, Swift bridge complete. macOS 26 compatibility investigation needed. Priority: High
- `adapteros-lora-mlx-ffi` - Active development, 95% test pass rate. Building successfully. Priority: High

**Disabled crates (workspace excluded):**
1. `adapteros-lora-kernel-mtl` - Excluded for stable main merge. Use Metal backend support in `adapteros-lora-worker`. Priority: Low
2. `adapteros-lora-worker` - Temporarily disabled due to compilation errors. Core inference pipeline tests passing. Priority: High
3. `adapteros-server` - Excluded for stable main merge. REST API available in `adapteros-server-api`. Priority: Low

**Note:** `adapteros-server-api`, `adapteros-system-metrics`, `adapteros-lora-mlx-ffi`, and `adapteros-codegraph` are workspace members and building successfully.

**Impact:** Core inference pipeline building (prebuilt Metal kernels). CLI (`aosctl`) operational. REST API in `adapteros-server-api`.

---

## Citations

Format: `[source: crates/adapteros-server/src/main.rs L173-L218]`

See [CITATIONS.md](CITATIONS.md) for standards.

---

## References

- [CONTRIBUTING.md](CONTRIBUTING.md) - PR guidelines
- [README.md](README.md) - Quick start
- [docs/ARCHITECTURE_INDEX.md](docs/ARCHITECTURE_INDEX.md) - Full architecture
- [docs/ARCHITECTURE_PATTERNS.md](docs/ARCHITECTURE_PATTERNS.md) - Detailed patterns & diagrams
- [docs/TELEMETRY_EVENTS.md](docs/TELEMETRY_EVENTS.md) - Event catalog
- [docs/DETERMINISTIC_EXECUTION.md](docs/DETERMINISTIC_EXECUTION.md) - HKDF, tick ledger, multi-agent
- [docs/LIFECYCLE.md](docs/LIFECYCLE.md) - State machine details
- [docs/TRAINING_PIPELINE.md](docs/TRAINING_PIPELINE.md) - Training flow
- [docs/DATABASE_REFERENCE.md](docs/DATABASE_REFERENCE.md) - Schema reference
- [docs/PINNING_TTL.md](docs/PINNING_TTL.md) - Pinning & TTL details
- [docs/UI_INTEGRATION.md](docs/UI_INTEGRATION.md) - UI integration
- [docs/RBAC.md](docs/RBAC.md) - RBAC permission matrix
- [docs/DEPRECATED_PATTERNS.md](docs/DEPRECATED_PATTERNS.md) - Anti-patterns
- `crates/adapteros-policy/` - Policy implementations
- `crates/adapteros-core/src/error.rs` - Error definitions

---

**Rule:** When in doubt, follow patterns in `crates/`. All documentation and code signed by **James KC Auchterlonie**.
- no python, only rust