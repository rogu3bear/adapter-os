# AdapterOS Developer Guide

**Purpose:** Technical reference for developers. For contribution process, see [CONTRIBUTING.md](CONTRIBUTING.md)
**Last Updated:** 2025-01-16
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

## RBAC (5 Roles, 20+ Permissions)

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

---

## Architecture Patterns

### Core Patterns (Consolidated)

| Pattern | Location | Key Concept |
|---------|----------|-------------|
| **K-Sparse Routing** | `adapteros-lora-router` | Top-K adapters via Q15 gates |
| **Metal Kernels** | `adapteros-lora-kernel-mtl` | Precompiled deterministic Metal kernels |
| **Configuration** | `adapteros-config` | Precedence: CLI > Env > File > Defaults |
| **Memory Mgmt** | `adapteros-memory` | Auto-eviction maintains ≥15% headroom |
| **Hot-Swap** | `adapteros-lora-worker/adapter_hotswap.rs` | Two-phase atomic swap with rollback |
| **Content Addressing** | `adapteros-core/hash.rs` | BLAKE3 hashing for all artifacts |
| **Deterministic Exec** | `adapteros-deterministic-exec` | Serial FIFO task execution, no concurrency |
| **HKDF Seeding** | `adapteros-core/hash.rs` | Domain-separated seeds (router, dropout, sampling, etc.) |

### Adapter Lifecycle State Machine
```
Unloaded → Cold → Warm → Hot → Resident
    ↑                              ↓
    └──────── (eviction) ──────────┘
```

**Transitions:** Promotion (activation % ↑), Demotion (activation % ↓ + timeout), Eviction (memory pressure + lowest %), Pinning (→ Resident)

```rust
use adapteros_lora_lifecycle::LifecycleManager;
let manager = LifecycleManager::new_with_db(adapter_names, &policies, path, telemetry, k, db);
manager.record_router_decision(&selected).await?; // Auto-promote
manager.check_memory_pressure(total_mem, 0.85).await?; // Auto-evict
```

### Deterministic Executor Seeding
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

**Env var:** `AOS_MANIFEST_PATH` (CLI `--manifest-path` overrides)
**Production enforcement:** Requires manifest when `require_pf_deny=true`
**Why:** Identical manifest = identical execution, enables replay/verification

### .aos Archive Format
```
[0-3]   manifest_offset (u32 LE)
[4-7]   manifest_len (u32 LE)
[offset] manifest (JSON)
[offset] weights (safetensors)
```

Zero-copy loading with memory-mapped files → GPU VRAM direct transfer.

### HKDF Hierarchy
```
Global Seed (BLAKE3) → derive_seed(seed, label)
  ├─ "router" (K-sparse tie-breaking)
  ├─ "dropout" (LoRA dropout masks)
  ├─ "sampling" (token sampling)
  ├─ "lora_trainer" (weight init)
  └─ "gate_noise", "executor", etc.
```

```rust
let global = B3Hash::hash(b"seed_material");
let router_seed = derive_seed(&global, "router");
let mut rng = ChaCha20Rng::from_seed(router_seed.try_into().unwrap());
```

### Hot-Swap Protocol
1. **Preload** adapter into staging area
2. **Swap** atomic pointer flip (mutex-guarded)
3. **Verify** effective-stack hash recomputation
4. **Rollback** on failure to last verified state

```rust
use adapteros_lora_worker::adapter_hotswap::AdapterTable;
let table = AdapterTable::new();
table.preload("new".to_string(), hash, vram_mb)?;
table.swap(&["new"], &["old"]).or_else(|e| { table.rollback()?; Err(e) })?;
```

**Architecture:** `active` (current) | `staged` (preloaded) | `rollback_state` (recovery)

---

## Document Processing & Training

### Pipeline (5 Steps)
1. **Ingest:** `DocumentIngestor::new(opts, tokenizer).ingest_pdf_path(path)?`
2. **Generate:** `generate_training_data(&doc, &tokenizer, &config)?`
3. **Dataset:** `TrainingDatasetManager::new(db, path, tok).create_dataset_from_documents(req).await?`
4. **Train:** `MicroLoRATrainer::new(cfg)?.train(examples, adapter_id).await?`
5. **Package:** `AdapterPackager::new().package(weights, manifest)?` → `registry.register_adapter(...)?`

**Training strategies:** Identity (unsupervised), QuestionAnswer, MaskedLM

**Core modules:** `adapteros-ingest-docs` (ingestion), `adapteros-orchestrator/training_dataset_integration.rs` (dataset mgmt), `adapteros-lora-worker/training/` (trainer, quantizer, packager)

**Dataset schema:** `training_datasets`, `dataset_files`, `dataset_statistics` (BLAKE3 content-addressed, JSONL format)

### Training Templates
- `general-code`: rank=16, alpha=32 (multi-language)
- `framework-specific`: rank=12, alpha=24

**Job tracking:** Pending → Running → Completed/Failed/Cancelled (progress %, loss, tokens/sec)

---

## Workflow Execution

**Types:** Sequential (serial), Parallel (concurrent merge), UpstreamDownstream (2-phase)

```rust
use adapteros_lora_lifecycle::{WorkflowExecutor, WorkflowType, KernelAdapterBackend};
let backend = Arc::new(KernelAdapterBackend::new(kernels_arc, names, 152064));
let executor = WorkflowExecutor::new(WorkflowType::UpstreamDownstream, vec!["a", "b"], backend);
let result = executor.execute(WorkflowContext { input_tokens, model_state, metadata }).await?;
```

**Backends:** `KernelAdapterBackend` (real Metal), `MockAdapterBackend` (testing)

---

## Database Schema (Core Tables)

| Table | Purpose | Key Fields |
|-------|---------|------------|
| `adapters` | Adapter metadata | id, hash, tier, rank, acl, activation_% |
| `tenants` | Tenant isolation | id, uid, gid, isolation_metadata |
| `adapter_stacks` | Reusable combos | id, name, adapter_ids_json, workflow_type |
| `training_datasets` | Dataset metadata | id, hash_b3, validation_status |
| `dataset_files` | Individual files | path, size, hash, ingestion_metadata |
| `dataset_statistics` | Cached stats | num_examples, total_tokens, distributions |
| `training_jobs` | Job tracking | id, dataset_id, status, progress_pct, loss |
| `audit_logs` | Immutable audit trail | user_id, action, resource, status, timestamp |

**Registry usage:**
```rust
use adapteros_registry::Registry;
let registry = Registry::open("./registry.db")?;
registry.register_adapter("id", &hash, "tier_1", rank, &["tenant_a"])?;
let allowed = registry.check_acl("id", "tenant_a")?;
```

---

## Streaming Architecture

**Modes:**
1. Batch (complete response)
2. Streaming (OpenAI-compatible SSE, token-by-token)

```rust
use adapteros_api::streaming::{StreamingInferenceRequest, streaming_inference_handler};
let request = StreamingInferenceRequest { prompt, model, max_tokens, temperature, stream: true, adapter_stack, ..Default::default() };
let stream = streaming_inference_handler(State(api_state), Json(request)).await;
// Format: data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"content":"Hi"},...}]}
// Final: data: [DONE]
```

**Features:** Keep-alive, client disconnect detection, OpenAI SDK compatible

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
| Unsafe in app crates | Security risk | Isolate to FFI crates (Metal, PyO3) |
| Blocking in async | Blocks executor | Use `tokio::time::sleep`, not `std::thread::sleep` |
| Unlocked kernel refs | Won't compile | `self.kernels.lock().await` before use |
| Unvalidated datasets | Training fails | Check `validation_status = 'valid'` |

See `docs/DEPRECATED_PATTERNS.md` for historical examples.

---

## Key Subsystems (Locations)

| Subsystem | Crate | Purpose |
|-----------|-------|---------|
| Router | `adapteros-lora-router` | K-sparse adapter selection |
| Metal Kernels | `adapteros-lora-kernel-mtl` | Deterministic GPU kernels |
| Policy Engine | `adapteros-policy` | 23-pack policy enforcement |
| Memory Mgmt | `adapteros-memory` | Auto-eviction, headroom maintenance |
| Lifecycle | `adapteros-lora-lifecycle` | State machine (Unloaded→Resident) |
| Hot-Swap | `adapteros-lora-worker/adapter_hotswap.rs` | Live adapter replacement |
| Deterministic Exec | `adapteros-deterministic-exec` | Serial FIFO task execution |
| HKDF | `adapteros-core/hash.rs` | Domain-separated seeding |
| Training | `adapteros-lora-worker/training/` | Trainer, quantizer, packager |
| Registry | `adapteros-registry` | SQLite WAL mode, adapter/tenant mgmt |
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
| `/api/chat/completions` | POST | OpenAI inference (streaming/batch) |
| `/api/adapter-stacks` | GET/POST | List/create adapter stacks |
| `/v1/audit/logs` | GET | Query audit logs (Admin/SRE/Compliance) |

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

**Disabled crates:**
1. `adapteros-server-api` (62 errors) - REST handlers need refactor. Priority: High
2. `adapteros-system-metrics` (11 SQL errors) - sqlx validation failures. Priority: Medium
3. `adapteros-lora-mlx-ffi` (PyO3 linker) - Experimental. Use Metal backend. Priority: Low
4. `adapteros-codegraph` (SQLite conflict) - Version conflict. Priority: Low

**Impact:** Core inference pipeline (Worker, Router, Kernels, Lifecycle, Policy, Telemetry) fully functional. CLI (`aosctl`) operational.

---

## Integration Testing Patterns

```rust
// Streaming
#[tokio::test]
async fn test_streaming() {
    let worker = create_test_worker().await?;
    let request = ChatCompletionRequest { messages: vec![ChatMessage { role: "user".into(), content: "Hi".into() }], stream: Some(true), ..Default::default() };
    let mut stream = worker.infer_streaming(request).await?;
    let chunks: Vec<_> = stream.collect().await;
    assert!(!chunks.is_empty() && chunks[0].object == "chat.completion.chunk");
}

// Workflow
#[tokio::test]
async fn test_workflow() {
    let kernels = MetalKernels::new()?;
    let backend = Arc::new(KernelAdapterBackend::new(Arc::new(Mutex::new(kernels)), names, 152064));
    let executor = WorkflowExecutor::new(WorkflowType::UpstreamDownstream, names, backend);
    let result = executor.execute(context).await?;
    assert_eq!(result.stats.phases.len(), 2);
}

// Policy Evidence
#[test]
fn test_evidence() {
    let policy = EvidencePolicy::new(config);
    let span = EvidenceSpan { relevance: 0.9, confidence: 0.95, evidence_type: EvidenceType::CodeDoc, ..Default::default() };
    assert!(policy.validate_evidence_spans(&[span]).is_ok());
}
```

---

## Citations

Format: `[source: crates/adapteros-server/src/main.rs L173-L218]`

See [CITATIONS.md](CITATIONS.md) for standards.

---

## References

- [CONTRIBUTING.md](CONTRIBUTING.md) - PR guidelines
- [README.md](README.md) - Quick start
- [docs/DEPRECATED_PATTERNS.md](docs/DEPRECATED_PATTERNS.md) - Anti-patterns
- [docs/ARCHITECTURE_INDEX.md](docs/ARCHITECTURE_INDEX.md) - Full architecture
- `crates/adapteros-policy/` - Policy implementations
- `crates/adapteros-core/src/error.rs` - Error definitions

---

**Rule:** When in doubt, follow patterns in `crates/`. All documentation and code signed by **James KC Auchterlonie**.
