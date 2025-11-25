# CLAUDE.md - AdapterOS Developer Guide

**Single source of truth** for AI assistants and developers. Last updated: 2025-11-25
**Copyright:** 2025 JKCA / James KC Auchterlonie | **Version:** v0.3-alpha

---

## Architecture Scale
| Metric | Count |
|--------|-------|
| Crates | 70 (+xtask, fuzz) |
| Rust LOC | 422,000+ |
| Migrations | 97 (0001-0097) |
| Policies | 24 |
| Permissions | 55 (5 roles) |
| REST Endpoints | ~190 |
| CI Workflows | 16 |
| Docs | 340+ files |

---

## Critical Rules

**Compilation != Correctness** - Verify runtime behavior, not just compilation.

**Duplication Prevention:**
- Search existing patterns before writing code
- Extract if: appears 3+ times, literal >50 chars used 2x, logic >10 lines duplicated
- Run `make dup` before committing

**Error Handling:** Use `Result<T, AosError>`, never `Option<T>` for errors.

**Logging:** Use `tracing` macros (info!, warn!, error!), never `println!` in production.

---

## Core Standards

### Code Patterns
```rust
// Errors: Always use AosError with context
.map_err(|e| AosError::Database(format!("Failed: {}", e)))?;

// Logging: Structured tracing
info!(tenant_id = %id, "Loading adapter");

// Deterministic: Use spawn_deterministic for inference/training/routing
spawn_deterministic("task".into(), async { /* reproducible logic */ })?;

// Non-deterministic OK: Background tasks, CLI, tests
tokio::spawn(async { /* monitoring, signals */ });
```

### Database Access
```rust
// PREFERRED: Db trait methods
state.db.update_adapter_state_tx(&id, "warm", "reason").await?;

// ACCEPTABLE: Direct SQL for simple queries or inside existing transactions
sqlx::query("SELECT...").fetch_one(&db.pool()).await?;
```

---

## Policies (24 Canonical)

| Policy | Purpose |
|--------|---------|
| Egress | Zero network in production (UDS only) |
| Determinism | HKDF-seeded randomness |
| Router | K-sparse Q15 quantized gates |
| Evidence | Audit trail, quality thresholds |
| Telemetry | Canonical JSON with signatures |
| Naming | `{tenant}/{domain}/{purpose}/{revision}` |
| DependencySecurity | CVE validation, supply chain |

All 24: Egress, Determinism, Router, Evidence, Refusal, Numeric, Rag, Isolation, Telemetry, Retention, Performance, Memory, Artifacts, Secrets, BuildRelease, Compliance, Incident, Output, Adapters, DeterministicIo, Drift, Mplora, Naming, DependencySecurity

See `crates/adapteros-policy/src/packs/`

---

## RBAC (5 Roles, 55 Permissions)

**Roles:** Admin, Operator, SRE, Compliance, Viewer

| Scope | Permissions |
|-------|------------|
| All | AdapterList/View, TrainingView, PolicyView, MetricsView |
| Admin only | AdapterDelete, PolicyApply/Sign, TenantManage, NodeManage |
| Operator+Admin+SRE | AdapterRegister/Load/Unload, Training*, Inference* |
| SRE+Compliance+Admin | AuditView |
| New (2025-11) | Activity*, Dashboard*, Notification*, Workspace* |

```rust
require_permission(&claims, Permission::AdapterRegister)?;
```

See [docs/RBAC.md](docs/RBAC.md)

---

## Multi-Backend Architecture

| Backend | Status | Determinism | Crate |
|---------|--------|-------------|-------|
| CoreML | Operational | Guaranteed (ANE) | `adapteros-lora-kernel-coreml` |
| MLX | Operational | HKDF-seeded | `adapteros-lora-mlx-ffi` |
| Metal | Fallback | Guaranteed | `adapteros-lora-kernel-mtl` |

```rust
let backend = create_backend(BackendChoice::CoreML { model_path: None })?;
let backend = create_backend(BackendChoice::Mlx { model_path })?;
let backend = create_backend(BackendChoice::Metal)?;
```

**CoreML:** ANE acceleration, Swift bridge (macOS 15+), graceful GPU fallback
**MLX:** `--features real-mlx`, circuit breaker, hot-swap, memory pool
**Selection:** CoreML → Metal → MLX fallback chain

See [docs/COREML_INTEGRATION.md](docs/COREML_INTEGRATION.md), [docs/MLX_INTEGRATION.md](docs/MLX_INTEGRATION.md)

---

## Key Subsystems

| Subsystem | Location |
|-----------|----------|
| Router | `adapteros-lora-router` (K-sparse) |
| Backend Factory | `adapteros-lora-worker/backend_factory.rs` |
| Policy Engine | `adapteros-policy` (24 packs) |
| Memory | `adapteros-memory` (≥15% headroom) |
| Lifecycle | `adapteros-lora-lifecycle` (Unloaded→Cold→Warm→Hot→Resident) |
| Hot-Swap | `adapteros-lora-worker/adapter_hotswap.rs` |
| Deterministic Exec | `adapteros-deterministic-exec` (FIFO) |
| Training | `adapteros-lora-worker/training/` |
| Registry | `adapteros-registry` (SQLite WAL) |

---

## Database

**Core Tables:** adapters, tenants, adapter_stacks, training_datasets, training_jobs, pinned_adapters, audit_logs

**97 Migrations** (key recent):
- 0084-0097: Default stacks, chat sessions, evidence, documents/collections, policy packs, base models, crypto audit

```bash
touch migrations/NNNN_description.sql
./scripts/sign_migrations.sh
```

See [docs/DATABASE_REFERENCE.md](docs/DATABASE_REFERENCE.md)

---

## Training Pipeline (5 Steps)

1. `DocumentIngestor::new(opts, tok).ingest_pdf_path(path)?`
2. `generate_training_data(&doc, &tok, &cfg)?`
3. `TrainingDatasetManager::new(db, path, tok).create_dataset_from_documents(req).await?`
4. `MicroLoRATrainer::new(cfg)?.train(examples, id).await?`
5. `AdapterPackager::new().package(weights, manifest)?`

**Templates:** `general-code` (rank=16, alpha=32), `framework-specific` (rank=12, alpha=24)

---

## REST API (~190 endpoints)

**Source:** `crates/adapteros-server-api/src/routes.rs` | **Auth:** JWT Ed25519

| Category | Base Path | Key Operations |
|----------|-----------|----------------|
| Health | `/healthz`, `/readyz` | Health, readiness |
| Auth | `/v1/auth/*` | login, logout, me |
| Adapters | `/v1/adapters/*` | CRUD, load/unload, pin, lifecycle |
| Stacks | `/v1/adapter-stacks/*` | Create, activate, deactivate |
| Training | `/v1/training/*` | Jobs, start, cancel |
| Datasets | `/v1/datasets/*` | Upload, validate, preview |
| Inference | `/v1/infer` | Batch, streaming |
| Policies | `/v1/policies/*` | List, validate, apply, sign |
| Metrics | `/v1/metrics/*` | System, adapters, time-series |
| Audit | `/v1/audit/*` | Logs, compliance |
| Collections | `/v1/collections/*` | Document grouping |
| Documents | `/v1/documents/*` | Upload, retrieval |
| Evidence | `/v1/evidence/*` | Provenance tracking |
| Streams (SSE) | `/v1/stream/*` | Metrics, telemetry, adapters |

**OpenAPI:** `/swagger-ui`, `/api-docs/openapi.json`

---

## CLI (51 command groups)

```bash
# Core
aosctl adapter [list|register|load|unload|pin|swap|info|health]
aosctl stack [list|create|activate|deactivate|delete]
aosctl node [list|verify|sync]
aosctl train / aosctl infer

# Operations
aosctl init / doctor / preflight / status / diag
aosctl golden [init|create|list|verify]
aosctl router [calibrate|validate|show]
aosctl policy [list|explain|enforce]

# Development
aosctl dev [start|stop] / chat / deploy / maintenance / bootstrap
aosctl audit / audit-determinism / verify / replay / rollback
aosctl telemetry [list|verify] / registry [sync|migrate]
aosctl federation verify / codegraph stats / secd status
```

---

## UI Architecture (90% complete)

| Directory | Pages | Purpose |
|-----------|-------|---------|
| `/pages/OwnerHome/` | 2 | Owner dashboard, system health, CLI console |
| `/pages/DocumentLibrary/` | 4 | Document management, chat |
| `/pages/System/` | 11 | Memory, metrics, nodes, workers |
| `/pages/Training/` | 17 | Datasets, jobs, templates |
| `/pages/Admin/` | 16 | Stacks, tenants, users, plugins |
| `/pages/Adapters/` | 12 | Detail, lifecycle, lineage |
| `/pages/Security/` | 8 | Policies, audit, compliance |

**130+ components** in `/components/chat/`, `/collections/`, `/documents/`, `/dashboard/`, `/adapters/`, `/ui/`

---

## CI/CD (16 workflows)

ci, deploy, integration-tests, metal-build, migration-testing, multi-backend, infrastructure-health, schema-drift-detection, security-regression-tests, type-validation, check-merge-conflicts, **duplication**, **architectural-lint**, **e2e-ui-tests**, **stress-tests**, **performance-regression**

---

## Quick Commands

```bash
# Build & test
cargo build --release && cargo test --workspace
cargo fmt --all && cargo clippy --workspace -- -D warnings
make dup  # Check duplication

# Database
./target/release/aosctl db migrate
./target/release/aosctl init-tenant --id default --uid 1000 --gid 1000

# Server
export AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-mlx
cargo run --release -p adapteros-server-api

# Inference
curl -X POST http://localhost:8080/v1/infer -H "Content-Type: application/json" \
  -d '{"prompt": "Hello", "max_tokens": 100}'
```

---

## Anti-Patterns

| Avoid | Fix |
|-------|-----|
| TODO comments | Complete or error |
| Option for errors | Result<T, AosError> |
| println! logging | tracing macros |
| Blocking in async | tokio::time::sleep |
| Direct AdapterLoader | Use lifecycle manager |
| Unvalidated datasets | Check validation_status |

---

## .aos Format

64-byte header: Magic (4) + Flags (4) + Weights offset/size (16) + Manifest offset/size (16) + Reserved (24)
Zero-copy mmap loading. See [docs/AOS_FORMAT.md](docs/AOS_FORMAT.md)

---

## Key References

- [docs/ARCHITECTURE_INDEX.md](docs/ARCHITECTURE_INDEX.md) - Full architecture
- [docs/ARCHITECTURE_PATTERNS.md](docs/ARCHITECTURE_PATTERNS.md) - Patterns & diagrams
- [docs/DETERMINISTIC_EXECUTION.md](docs/DETERMINISTIC_EXECUTION.md) - HKDF, tick ledger
- [docs/LIFECYCLE.md](docs/LIFECYCLE.md) - Adapter state machine
- [docs/TRAINING_PIPELINE.md](docs/TRAINING_PIPELINE.md) - Training flow
- [docs/DATABASE_REFERENCE.md](docs/DATABASE_REFERENCE.md) - Schema
- [docs/RBAC.md](docs/RBAC.md) - Permissions
- [docs/COREML_INTEGRATION.md](docs/COREML_INTEGRATION.md) - CoreML backend
- [docs/MLX_INTEGRATION.md](docs/MLX_INTEGRATION.md) - MLX backend
- [QUICKSTART.md](QUICKSTART.md) - Getting started

---

## Language Requirements

**Rust only** - No Python. All tools in Rust or shell scripts.

---

**Rule:** Follow patterns in `crates/`. Signed by James KC Auchterlonie.
