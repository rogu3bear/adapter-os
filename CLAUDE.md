# AdapterOS Developer Guide

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.

**Purpose:** Quick reference for developers. For detailed architecture, see [docs/ARCHITECTURE_INDEX.md](docs/ARCHITECTURE_INDEX.md)
**Last Updated:** 2025-11-22 (AOS format with 64-byte header documented)
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

64-byte header for optimal cache-line alignment:

```
+--------+--------+------------------------------------------+
| Offset | Size   | Field                                    |
+--------+--------+------------------------------------------+
| 0      | 4      | Magic bytes: "AOS\x00"                   |
| 4      | 4      | Flags (u32 LE, reserved)                 |
| 8      | 8      | Weights offset (u64 LE)                  |
| 16     | 8      | Weights size (u64 LE)                    |
| 24     | 8      | Manifest offset (u64 LE)                 |
| 32     | 8      | Manifest size (u64 LE)                   |
| 40     | 24     | Reserved (padding to 64 bytes)           |
+--------+--------+------------------------------------------+
| 64     | N      | Weights (SafeTensors or Q15)             |
| 64+N   | M      | Manifest (JSON metadata)                 |
+--------+--------+------------------------------------------+
```

Zero-copy loading with memory-mapped files, direct GPU VRAM transfer. See [docs/AOS_FORMAT.md](docs/AOS_FORMAT.md) for full specification.

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
| **CoreML** | **Implemented** (model loading, inference, Swift bridge with runtime dispatch) | **Guaranteed (ANE)** | ANE acceleration, production | `adapteros-lora-kernel-coreml` |
| **MLX** | **Implemented** (model loading, forward passes, hidden states, text generation) | **HKDF-seeded** | Research, training | `adapteros-lora-mlx-ffi` |
| **Metal** | **Implemented** (precompiled Metal kernels, GPU acceleration) | **Guaranteed** | Legacy, non-ANE systems | `adapteros-lora-kernel-mtl` |

**Implementation Status:**
- CoreML: Fully implemented and operational. Supports model loading, inference, ANE detection, memory pooling, and MLTensor bridge (macOS 15+). Guaranteed determinism with ANE, graceful fallback to GPU on older systems. See [docs/COREML_ACTIVATION.md](docs/COREML_ACTIVATION.md) for operational guide.
- MLX: Fully implemented. Supports model loading from file or buffer, text generation, health tracking with circuit breaker, and memory pool integration.
- Multi-adapter routing: Implemented with K-sparse selection and Q15 quantized gates (see MULTI_ADAPTER_ROUTING.md)

**Backend Selection:**
```rust
use adapteros_lora_worker::backend_factory::{BackendChoice, create_backend};

// Production: CoreML (ANE acceleration, guaranteed determinism)
let backend = create_backend(BackendChoice::CoreML { model_path: None })?;

// Research/Training: MLX (HKDF-seeded determinism)
// Note: Requires --features real-mlx and MLX C++ library for GPU acceleration
// Falls back to software implementation if MLX not available
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
- [docs/COREML_ACTIVATION.md](docs/COREML_ACTIVATION.md) - CoreML operational status & verification procedures
- [docs/COREML_INTEGRATION.md](docs/COREML_INTEGRATION.md) - CoreML setup & ANE optimization
- [docs/MLX_INTEGRATION.md](docs/MLX_INTEGRATION.md) - MLX complete integration guide
- [docs/MLX_QUICK_REFERENCE.md](docs/MLX_QUICK_REFERENCE.md) - MLX quick start and configuration patterns
- [docs/MLX_BACKEND_DEPLOYMENT_GUIDE.md](docs/MLX_BACKEND_DEPLOYMENT_GUIDE.md) - MLX production deployment steps
- [docs/MLX_ROUTER_HOTSWAP_INTEGRATION.md](docs/MLX_ROUTER_HOTSWAP_INTEGRATION.md) - MLX router and hot-swap integration
- [docs/ADDING_NEW_BACKEND.md](docs/ADDING_NEW_BACKEND.md) - Template for new backends

### MLX Backend Details

The MLX backend is fully implemented for research and training workloads with enterprise-grade resilience:

**Features:**
- Model loading from directory or pre-serialized buffer
- Forward passes, hidden state extraction, text generation
- HKDF-seeded deterministic execution (RNG operations)
- Circuit breaker with health monitoring and auto-recovery
- Hot-swap support: live adapter loading/unloading
- Multi-adapter routing via K-sparse selection with Q15 quantized gates
- Memory pool integration with GC hints
- Comprehensive error handling and FFI safety

**Build & Deployment:**
```bash
# Build with MLX backend enabled
cargo build -p adapteros-lora-mlx-ffi --features real-mlx --release

# Start server with MLX backend
export AOS_MLX_FFI_MODEL="./models/qwen2.5-7b-mlx"
./target/release/aosctl serve --backend mlx --model-path ./models/qwen2.5-7b-mlx
```

**Usage Example:**
```rust
use adapteros_lora_mlx_ffi::{MLXFFIModel, generation::GenerationConfig};
use adapteros_core::{derive_seed, B3Hash};

let model = MLXFFIModel::load("./models/qwen2.5-7b-mlx")?;

// Deterministic seeding
let base_seed = B3Hash::hash(b"production-model");
let seed = derive_seed(&base_seed, "text-generation:step-0");
adapteros_lora_mlx_ffi::mlx_set_seed_from_bytes(&seed)?;

// Generate text with reproducible results
let text = model.generate("Once upon a time", 100)?;
```

See [docs/MLX_QUICK_REFERENCE.md](docs/MLX_QUICK_REFERENCE.md) for quick start and configuration patterns.

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

## REST API Reference

> **Maintenance:** Update this section when adding/removing routes in `crates/adapteros-server-api/src/routes.rs`. Verify OpenAPI annotations match with `cargo doc`.

**Source:** `crates/adapteros-server-api/src/routes.rs` | **Total Endpoints:** ~189 | **Auth:** JWT (Ed25519) required except where noted

### Health & Auth (Public)
| Method | Path | Description |
|--------|------|-------------|
| GET | `/healthz` | Health check |
| GET | `/healthz/all` | All component health |
| GET | `/healthz/:component` | Specific component health |
| GET | `/readyz` | Readiness check |
| POST | `/v1/auth/login` | User login |
| POST | `/v1/auth/logout` | User logout |
| GET | `/v1/auth/me` | Current user info |
| GET | `/v1/meta` | API metadata |

### Tenants
| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/tenants` | List tenants |
| POST | `/v1/tenants` | Create tenant |
| PUT | `/v1/tenants/:tenant_id` | Update tenant |
| POST | `/v1/tenants/:tenant_id/pause` | Pause tenant |
| POST | `/v1/tenants/:tenant_id/archive` | Archive tenant |
| POST | `/v1/tenants/:tenant_id/policies` | Assign policies |
| POST | `/v1/tenants/:tenant_id/adapters` | Assign adapters |
| GET | `/v1/tenants/:tenant_id/usage` | Usage statistics |

### Adapters
| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/adapters` | List adapters |
| GET | `/v1/adapters/:adapter_id` | Get adapter details |
| POST | `/v1/adapters/register` | Register adapter |
| DELETE | `/v1/adapters/:adapter_id` | Delete adapter |
| POST | `/v1/adapters/:adapter_id/load` | Load adapter |
| POST | `/v1/adapters/:adapter_id/unload` | Unload adapter |
| GET | `/v1/adapters/verify-gpu` | Verify GPU integrity |
| GET | `/v1/adapters/:adapter_id/activations` | Get activations |
| POST | `/v1/adapters/:adapter_id/lifecycle/promote` | Promote lifecycle state |
| POST | `/v1/adapters/:adapter_id/lifecycle/demote` | Demote lifecycle state |
| GET | `/v1/adapters/:adapter_id/lineage` | Lineage tree |
| GET | `/v1/adapters/:adapter_id/detail` | Detail view |
| GET | `/v1/adapters/:adapter_id/manifest` | Download manifest |
| POST | `/v1/adapters/directory/upsert` | Upsert directory adapter |
| GET | `/v1/adapters/:adapter_id/health` | Adapter health |
| GET | `/v1/adapters/:adapter_id/pin` | Get pin status |
| POST | `/v1/adapters/:adapter_id/pin` | Pin adapter |
| DELETE | `/v1/adapters/:adapter_id/pin` | Unpin adapter |
| POST | `/v1/adapters/:adapter_id/state/promote` | Promote tier (persistent→warm→ephemeral) |
| POST | `/v1/adapters/validate-name` | Validate adapter name |
| GET | `/v1/adapters/next-revision/:tenant/:domain/:purpose` | Get next revision |

### Adapter Stacks
| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/adapter-stacks` | List stacks |
| POST | `/v1/adapter-stacks` | Create stack |
| GET | `/v1/adapter-stacks/:id` | Get stack |
| DELETE | `/v1/adapter-stacks/:id` | Delete stack |
| POST | `/v1/adapter-stacks/:id/activate` | Activate stack |
| POST | `/v1/adapter-stacks/deactivate` | Deactivate stack |
| POST | `/v1/stacks/validate-name` | Validate stack name |

### Domain Adapters
| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/domain-adapters` | List domain adapters |
| POST | `/v1/domain-adapters` | Create domain adapter |
| GET | `/v1/domain-adapters/:adapter_id` | Get domain adapter |
| DELETE | `/v1/domain-adapters/:adapter_id` | Delete domain adapter |
| POST | `/v1/domain-adapters/:adapter_id/load` | Load |
| POST | `/v1/domain-adapters/:adapter_id/unload` | Unload |
| POST | `/v1/domain-adapters/:adapter_id/test` | Test |
| GET | `/v1/domain-adapters/:adapter_id/manifest` | Get manifest |
| POST | `/v1/domain-adapters/:adapter_id/execute` | Execute |

### Inference
| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/infer` | Run inference |
| POST | `/v1/infer/batch` | Batch inference |
| POST | `/v1/patch/propose` | Propose patch |

### Training
| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/training/jobs` | List jobs |
| GET | `/v1/training/jobs/:job_id` | Get job |
| POST | `/v1/training/start` | Start training |
| POST | `/v1/training/jobs/:job_id/cancel` | Cancel job |
| POST | `/v1/training/sessions` | Create session |
| GET | `/v1/training/jobs/:job_id/logs` | Job logs |
| GET | `/v1/training/jobs/:job_id/metrics` | Job metrics |
| GET | `/v1/training/jobs/:job_id/artifacts` | Job artifacts |
| GET | `/v1/training/templates` | List templates |
| GET | `/v1/training/templates/:template_id` | Get template |

### Datasets
| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/datasets/upload` | Upload dataset |
| POST | `/v1/datasets/chunked-upload/initiate` | Chunked upload |
| GET | `/v1/datasets` | List datasets |
| GET | `/v1/datasets/:dataset_id` | Get dataset |
| DELETE | `/v1/datasets/:dataset_id` | Delete dataset |
| GET | `/v1/datasets/:dataset_id/files` | Dataset files |
| GET | `/v1/datasets/:dataset_id/statistics` | Statistics |
| POST | `/v1/datasets/:dataset_id/validate` | Validate |
| GET | `/v1/datasets/:dataset_id/preview` | Preview |
| GET | `/v1/datasets/upload/progress` | Upload progress |

### Nodes & Workers
| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/nodes` | List nodes |
| POST | `/v1/nodes/register` | Register node |
| POST | `/v1/nodes/:node_id/ping` | Ping node |
| POST | `/v1/nodes/:node_id/offline` | Mark offline |
| DELETE | `/v1/nodes/:node_id` | Evict node |
| GET | `/v1/nodes/:node_id/details` | Node details |
| GET | `/v1/workers` | List workers |
| POST | `/v1/workers/spawn` | Spawn worker |
| GET | `/v1/workers/:worker_id/logs` | Worker logs |
| GET | `/v1/workers/:worker_id/crashes` | Worker crashes |
| POST | `/v1/workers/:worker_id/debug` | Debug session |
| POST | `/v1/workers/:worker_id/troubleshoot` | Troubleshoot |

### Policies
| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/policies` | List policies |
| GET | `/v1/policies/:cpid` | Get policy |
| POST | `/v1/policies/validate` | Validate policy |
| POST | `/v1/policies/apply` | Apply policy |
| POST | `/v1/policies/:cpid/sign` | Sign policy |
| POST | `/v1/policies/compare` | Compare versions |
| GET | `/v1/policies/:cpid/export` | Export policy |

### Routing
| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/routing/debug` | Debug routing |
| GET | `/v1/routing/history` | Routing history |
| GET | `/v1/routing/decisions` | List decisions |
| GET | `/v1/routing/decisions/:id` | Get decision |
| POST | `/v1/telemetry/routing` | Ingest router decision |

### Metrics & Monitoring
| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/metrics` | Get metrics (custom auth) |
| GET | `/v1/metrics/quality` | Quality metrics |
| GET | `/v1/metrics/adapters` | Adapter metrics |
| GET | `/v1/metrics/system` | System metrics |
| GET | `/v1/metrics/snapshot` | Metrics snapshot |
| GET | `/v1/metrics/series` | Metrics series |
| GET | `/v1/monitoring/rules` | List rules |
| POST | `/v1/monitoring/rules` | Create rule |
| GET | `/v1/monitoring/alerts` | List alerts |
| POST | `/v1/monitoring/alerts/:alert_id/acknowledge` | Ack alert |
| GET | `/v1/monitoring/anomalies` | List anomalies |
| POST | `/v1/monitoring/anomalies/:anomaly_id/status` | Update status |
| GET | `/v1/monitoring/dashboards` | List dashboards |
| POST | `/v1/monitoring/dashboards` | Create dashboard |
| GET | `/v1/monitoring/health-metrics` | Health metrics |
| GET | `/v1/monitoring/reports` | List/create reports |

### Streaming (SSE)
| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/streams/training` | Training events |
| GET | `/v1/streams/discovery` | Discovery events |
| GET | `/v1/streams/contacts` | Contacts events |
| GET | `/v1/streams/file-changes` | File changes |
| GET | `/v1/stream/metrics` | System metrics stream |
| GET | `/v1/stream/telemetry` | Telemetry events |
| GET | `/v1/stream/adapters` | Adapter state stream |

### Audit & Compliance
| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/audit/logs` | Query audit logs |
| GET | `/v1/audit/federation` | Federation audit |
| GET | `/v1/audit/compliance` | Compliance audit |
| GET | `/v1/audits` | Extended audits |

### Promotions & Golden Runs
| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/cp/promote` | Request promotion |
| GET | `/v1/cp/promotion-gates/:cpid` | Promotion gates |
| POST | `/v1/cp/rollback` | Rollback |
| POST | `/v1/cp/promote/dry-run` | Dry-run promotion |
| GET | `/v1/cp/promotions` | Promotion history |
| GET | `/v1/promotions/:id` | Get promotion |
| GET | `/v1/golden/runs` | List golden runs |
| GET | `/v1/golden/runs/:name` | Get golden run |
| POST | `/v1/golden/compare` | Compare runs |
| POST | `/v1/golden/:run_id/promote` | Request promotion |
| GET | `/v1/golden/:run_id/promotion` | Promotion status |
| POST | `/v1/golden/:run_id/approve` | Approve/reject |
| GET | `/v1/golden/:run_id/gates` | Gate status |
| POST | `/v1/golden/:stage/rollback` | Stage rollback |

### Logs & Traces
| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/logs/query` | Query logs |
| GET | `/v1/logs/stream` | Stream logs (SSE) |
| GET | `/v1/traces/search` | Search traces |
| GET | `/v1/traces/:trace_id` | Get trace details |

### Service Supervisor (separate service, port 3301)
| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/services` | List all services |
| GET | `/v1/services/:service_id` | Get service details |
| POST | `/v1/services/start` | Start service |
| POST | `/v1/services/stop` | Stop service |
| POST | `/v1/services/restart` | Restart service |
| POST | `/v1/services/essential/start` | Start essential services |
| POST | `/v1/services/essential/stop` | Stop essential services |
| GET | `/v1/services/:service_id/logs` | Get service logs |

### Additional Endpoints
| Resource | Base Path | Key Operations |
|----------|-----------|----------------|
| Code Intelligence | `/v1/code/*` | Register repo, scan, commit delta |
| Contacts | `/v1/contacts/*` | CRUD, interactions |
| Federation | `/v1/federation/*` | Status, quarantine |
| Git | `/v1/git/*` | Status, sessions, branches |
| Models | `/v1/models/*` | Import, status |
| Plans | `/v1/plans/*` | Build, compare, manifest |
| Plugins | `/v1/plugins/*` | Enable/disable, status |
| Replay | `/v1/replay/*` | Sessions, verify |
| System | `/v1/system/memory` | UMA memory info |
| Telemetry | `/v1/telemetry/bundles/*` | Export, verify, purge |
| Jobs | `/v1/jobs` | List jobs |
| Commits | `/v1/commits/*` | List, details, diff |
| Repositories | `/v1/repositories` | List (deprecated → use `/v1/code/repositories`) |

### OpenAPI / Swagger
| Method | Path | Description |
|--------|------|-------------|
| GET | `/swagger-ui` | Swagger UI interface |
| GET | `/api-docs/openapi.json` | OpenAPI spec (JSON) |

### Unwired Handlers (Planned/Internal)
The following handler modules exist in `crates/adapteros-server-api/src/handlers/` but are not yet wired to routes:
- `activity.rs`, `dashboard.rs`, `messages.rs`, `notifications.rs`
- `workspaces.rs`, `tutorials.rs`, `journeys.rs`
- `git_repository.rs` (separate from `git.rs`)
- `models.rs`, `streaming.rs`, `chunked_upload.rs`

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

### Benchmarking
```bash
# Run MLX FFI benchmarks (updates target/criterion/)
cargo bench -p adapteros-lora-mlx-ffi --bench mlx_integration_benchmark

# Run with real MLX backend (requires mlx C++ library)
cargo bench -p adapteros-lora-mlx-ffi --bench mlx_integration_benchmark --features real-mlx

# Run integration verification tests with timing output
cargo test -p adapteros-lora-mlx-ffi --test integration_verification -- --nocapture

# View HTML benchmark reports
open target/criterion/report/index.html
```

### CLI Commands (Git-Style)
```bash
# Adapter management
aosctl adapter list                    # List adapters
aosctl adapter register <id> <hash>    # Register adapter
aosctl adapter pin <id>                # Pin adapter
aosctl adapter swap --add <id>         # Hot-swap adapters

# Node management
aosctl node list                       # List cluster nodes
aosctl node verify --all               # Verify cross-node determinism
aosctl node sync push --to <node>      # Push adapters to node

# Telemetry
aosctl telemetry list                  # List telemetry events
aosctl telemetry verify --bundle-dir   # Verify bundle chain

# Registry
aosctl registry sync --dir ./adapters # Sync adapters to registry
aosctl registry migrate                # Migrate legacy database

# Code intelligence
aosctl code init .                     # Initialize repository
aosctl code update <repo>              # Scan repository
aosctl code list                       # List repositories

# Other grouped commands
aosctl federation verify               # Verify federation signatures
aosctl codegraph stats                 # CodeGraph statistics
aosctl secd status                     # Security daemon status
```

**After running benchmarks, update [BENCHMARK_RESULTS.md](BENCHMARK_RESULTS.md) with new results.**

Key benchmark locations:
- `crates/adapteros-lora-mlx-ffi/benches/mlx_integration_benchmark.rs` - MLX FFI benchmarks
- `target/criterion/` - Criterion results and HTML reports

---

## Quick Start UX Flow

Complete workflow from startup to inference with trained adapters. For detailed steps, see [QUICKSTART.md](QUICKSTART.md) and [docs/QUICKSTART_COMPLETE_SYSTEM.md](docs/QUICKSTART_COMPLETE_SYSTEM.md).

### 1. Start the System

```bash
# Terminal 1: Start backend server
export AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-mlx
export DATABASE_URL=sqlite://var/aos-cp.sqlite3
cargo run --release -p adapteros-server-api

# Terminal 2: Start UI (optional)
cd ui && pnpm dev
# UI available at http://localhost:5173
```

### 2. Load a Model

```bash
# Download Qwen 2.5 7B MLX format (~3.8GB)
huggingface-cli download mlx-community/Qwen2.5-7B-Instruct \
    --include "*.safetensors" "*.json" \
    --local-dir models/qwen2.5-7b-mlx

# Verify model is detected
curl http://localhost:8080/healthz
```

### 3. Run Inference

**Standard (batch) inference:**
```bash
curl -X POST http://localhost:8080/v1/infer \
  -H "Content-Type: application/json" \
  -d '{"prompt": "Write hello world in Rust", "max_tokens": 100}'
```

**Streaming inference (SSE):**
```bash
curl -X POST http://localhost:8080/v1/infer \
  -H "Content-Type: application/json" \
  -d '{"prompt": "Explain async in Rust", "max_tokens": 200, "stream": true}'
```

**Via UI:** Navigate to `/inference`, enter prompt, click Generate.

### 4. Train an Adapter

**Prepare dataset (JSONL):**
```jsonl
{"input": "What is Rust?", "target": "Rust is a systems programming language..."}
{"input": "Explain ownership", "target": "Ownership is Rust's memory management..."}
```

**Upload and train:**
```bash
# Upload dataset
curl -X POST http://localhost:8080/v1/datasets/upload \
  -F "name=rust-qa" -F "format=jsonl" -F "file=@training.jsonl"

# Start training (save dataset_id from response)
./target/release/aosctl train \
  --dataset-id <dataset_id> \
  --output adapters/rust-expert.aos \
  --rank 16 --epochs 3
```

**Via UI:** Navigate to `/training`, upload dataset, configure hyperparameters, start job.

### 5. Use the Trained Adapter

```bash
# Load adapter
curl -X POST http://localhost:8080/v1/adapters/rust-expert/load

# Inference with adapter
curl -X POST http://localhost:8080/v1/infer \
  -H "Content-Type: application/json" \
  -d '{"prompt": "Explain borrowing in Rust", "max_tokens": 150, "adapters": ["rust-expert"]}'

# Hot-swap adapters (live, <100ms)
./target/release/aosctl adapter swap --tenant default --add rust-expert --remove code-assistant --commit
```

**Via UI:** Select adapter from dropdown in Inference page.

### Quick Verification

```bash
# Health check
curl http://localhost:8080/healthz

# List adapters
curl http://localhost:8080/v1/adapters

# System metrics
curl http://localhost:8080/v1/metrics/system
```

**Full guides:** [QUICKSTART.md](QUICKSTART.md) | [docs/QUICKSTART_COMPLETE_SYSTEM.md](docs/QUICKSTART_COMPLETE_SYSTEM.md) | [QUICKSTART_GPU_TRAINING.md](QUICKSTART_GPU_TRAINING.md)

---

## Known Build Issues (Alpha v0.01-1)

**Status:** 40+ crates building successfully

**Backend Implementation Status:**
- `adapteros-lora-kernel-coreml` - Fully implemented and operational. Supports model loading, inference, ANE detection, memory pool integration, and Swift bridge (macOS 15+). Guaranteed determinism with ANE, graceful GPU fallback. See [docs/COREML_ACTIVATION.md](docs/COREML_ACTIVATION.md). Priority: Operational
- `adapteros-lora-mlx-ffi` - Fully implemented. Supports model loading, text generation, health tracking, and memory pool integration. Priority: Operational
- `adapteros-lora-kernel-mtl` - In workspace, builds successfully. Priority: Low

**Workspace crates with issues:**
1. `adapteros-lora-worker` - In workspace, library compiles. 29 test errors (tests need fixes). Priority: High

**Excluded from workspace:**
1. `adapteros-server` - Excluded for stable main merge. REST API available in `adapteros-server-api`. Priority: Low

**Routing Implementation:**
- Multi-adapter routing fully implemented with K-sparse selection and Q15 quantized gates (see MULTI_ADAPTER_ROUTING.md)

**Note:** `adapteros-server-api`, `adapteros-system-metrics`, `adapteros-lora-mlx-ffi`, `adapteros-lora-kernel-mtl`, and `adapteros-codegraph` are workspace members and building successfully.

**Impact:** Core inference pipeline building. CLI (`aosctl`) operational. REST API in `adapteros-server-api`. Multi-backend support operational.

---

## Citations

Format: `[source: crates/adapteros-server/src/main.rs L173-L218]`

See [CITATIONS.md](CITATIONS.md) for standards.

---

## References

- [QUICKSTART.md](QUICKSTART.md) - Quick start guide
- [docs/QUICKSTART_COMPLETE_SYSTEM.md](docs/QUICKSTART_COMPLETE_SYSTEM.md) - Complete system setup
- [QUICKSTART_GPU_TRAINING.md](QUICKSTART_GPU_TRAINING.md) - GPU training quick start
- [CONTRIBUTING.md](CONTRIBUTING.md) - PR guidelines
- [README.md](README.md) - Project overview
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
- [docs/COREML_ACTIVATION.md](docs/COREML_ACTIVATION.md) - CoreML activation & operational status
- [docs/COREML_INTEGRATION.md](docs/COREML_INTEGRATION.md) - CoreML backend implementation guide
- [BENCHMARK_RESULTS.md](BENCHMARK_RESULTS.md) - **MLX FFI benchmark results** (update after running benchmarks)
- `crates/adapteros-lora-mlx-ffi/MLX_FFI_INTEGRATION_PROOF.md` - MLX FFI integration proof document
- `crates/adapteros-policy/` - Policy implementations
- `crates/adapteros-core/src/error.rs` - Error definitions

---

**Rule:** When in doubt, follow patterns in `crates/`. All documentation and code signed by **James KC Auchterlonie**.

---

## See Also

Key cross-references for daily development:

- [docs/ADR_MULTI_BACKEND_STRATEGY.md](docs/ADR_MULTI_BACKEND_STRATEGY.md) - Backend selection rationale and architecture
- [docs/FEATURE_FLAGS.md](docs/FEATURE_FLAGS.md) - Complete feature flag reference
- [docs/LOCAL_BUILD.md](docs/LOCAL_BUILD.md) - Build troubleshooting and environment setup
- [docs/MLX_INTEGRATION.md](docs/MLX_INTEGRATION.md) - MLX backend complete guide
- [docs/README.md](docs/README.md) - Documentation index and navigation

---

## Language Requirements
- **Rust only** - No Python should be used or created
- All tools, utilities, and scripts must be written in Rust or shell scripts
- For any code generation, testing, or benchmarking: use Rust exclusively
- Build automation: Rust (build.rs) or shell scripts only