# CLAUDE.md - AdapterOS Developer Guide

**Single source of truth** for AI assistants and developers. Last updated: 2025-11-28
**Copyright:** 2025 JKCA / James KC Auchterlonie | **Version:** v0.3-alpha

---

## Architecture Scale
| Metric | Count |
|--------|-------|
| Crates | 57 |
| Rust LOC | 422,000+ |
| Migrations | 119 (0001-0119) |
| Policies | 24 |
| Permissions | 56 (5 roles) |
| REST Endpoints | ~250+ |
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

**Note:** CircuitBreaker policy exists in packs/ but is not registered in PolicyId enum.

See `crates/adapteros-policy/src/packs/`

---

## RBAC (5 Roles, 56 Permissions)

**Roles:** Admin, Operator, SRE, Compliance, Viewer

| Scope | Permissions |
|-------|------------|
| All (Viewer+) | AdapterList/View, TrainingView, PolicyView, MetricsView, plus 18 other view permissions |
| Admin only | AdapterDelete, PolicyApply/Sign, TenantManage, NodeManage |
| Operator+Admin | AdapterRegister, Training*, Inference* |
| SRE+Admin | AdapterLoad/Unload |
| SRE+Compliance+Admin | AuditView |
| New (2025-11) | Activity*, Dashboard*, Notification*, Workspace*, DatasetList |

```rust
require_permission(&claims, Permission::AdapterRegister)?;
```

See [docs/RBAC.md](docs/RBAC.md)

---

## Configuration System

**Crate:** `adapteros-config` | **Categories:** 16 | **Variables:** ~60

### Precedence (highest to lowest)
1. CLI arguments
2. Environment variables (`.env` file supported)
3. Manifest file

### Key Categories

| Category | Variables | Description |
|----------|-----------|-------------|
| MODEL | AOS_MODEL_PATH, AOS_MODEL_BACKEND | Base model configuration |
| SERVER | AOS_SERVER_PORT, AOS_SERVER_HOST | Server binding |
| DATABASE | AOS_DATABASE_URL, AOS_DATABASE_POOL_SIZE | SQLite/Postgres |
| SECURITY | AOS_SECURITY_JWT_SECRET, AOS_SIGNING_KEY | Auth & signing |
| ROUTER | AOS_ROUTER_K_SPARSE, AOS_ROUTER_QUANTIZATION | K-sparse adapter selection |
| PATHS | AOS_VAR_DIR, AOS_MODEL_CACHE_DIR | Runtime directories |
| WORKER | AOS_WORKER_THREADS | Background workers |
| DEBUG | AOS_DEBUG_ENABLED | Development mode |
| LOGGING | AOS_LOG_LEVEL, AOS_LOG_FORMAT | Log verbosity and format |
| MEMORY | AOS_MEMORY_HEADROOM_PCT | Memory management |
| BACKEND | AOS_BACKEND_COREML_ENABLED, AOS_BACKEND_MLX_ENABLED | Backend selection |
| TELEMETRY | AOS_TELEMETRY_ENABLED | Telemetry collection |
| TRAINING | AOS_TRAINING_MAX_EPOCHS, AOS_TRAINING_CHECKPOINT_INTERVAL | Training config |
| FEDERATION | AOS_FEDERATION_ENABLED, AOS_FEDERATION_NODE_ID | Multi-node federation |
| EMBEDDINGS | AOS_EMBEDDING_DIMENSION | Embedding configuration |

### Usage

```rust
// Initialize at startup (validates all AOS_* vars)
adapteros_config::initialize_config()?;

// Access config anywhere
let cfg = adapteros_config::get_config()?;
let port = cfg.server_port();
let k_sparse = cfg.router_k_sparse();
```

```bash
# .env file
AOS_MODEL_PATH=./models/qwen2.5-7b-mlx
AOS_SERVER_PORT=8080
AOS_ROUTER_K_SPARSE=4
```

See `crates/adapteros-config/src/schema.rs` for full schema.

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

**Core Tables:** adapters, tenants, adapter_stacks, repository_training_jobs, training_datasets, pinned_adapters, audit_logs, chat_sessions, chat_messages, documents, document_chunks, document_collections, inference_evidence

**119 Migrations** (key recent):
- 0084-0097: Default stacks, chat sessions, evidence, documents/collections, policy packs, base models, crypto audit
- 0102-0116: Dashboard indexes, federation health, chat features (tags, FTS, sharing), admin access
- 0117: Training job category metadata (category, language, framework, post_actions)
- 0118: Training jobs denormalization
- 0119: Registry consolidation

```bash
touch migrations/NNNN_description.sql
./scripts/sign_migrations.sh
```

See [docs/DATABASE_REFERENCE.md](docs/DATABASE_REFERENCE.md)

---

## Storage Paths

All runtime data is stored under `var/` relative to the working directory. Use `PlatformUtils` from `adapteros-platform` for path resolution.

### Directory Structure

```text
var/
├── aos-cp.sqlite3         # SQLite database (metadata for all entities)
├── adapters/              # LoRA adapter weights (.aos files)
├── artifacts/             # Training artifacts, temp files
│   ├── temp/              # Temporary processing files
│   └── cache/             # Orchestration cache
├── datasets/              # Training datasets (NDJSON files)
├── documents/             # Uploaded document files (PDFs, etc.)
│   └── {tenant_id}/       # Tenant-isolated subdirectories
├── model-cache/           # Downloaded models from HuggingFace
│   ├── blobs/             # Content-addressed storage (BLAKE3)
│   ├── models/            # Model directories with symlinks
│   ├── downloads/         # In-progress downloads
│   └── locks/             # File locks for concurrent access
├── bundles/               # Telemetry bundles
└── alerts/                # Alert logs
```

### What Goes Where

| Data Type | File Storage | Database Table |
|-----------|--------------|----------------|
| **Documents** | `var/documents/{tenant_id}/{id}.pdf` | `documents` (metadata), `document_chunks` (embeddings) |
| **Adapters** | `var/adapters/{id}.aos` | `adapters` (metadata, lifecycle state) |
| **Datasets** | `var/datasets/{id}.ndjson` | `training_datasets` (metadata, validation) |
| **Training Jobs** | `var/artifacts/` (outputs) | `training_jobs` (status, metrics) |
| **Chat Sessions** | — | `chat_sessions`, `chat_messages` |
| **Collections** | — | `document_collections`, `collection_documents` |
| **Evidence** | — | `inference_evidence` (provenance tracking) |
| **Stacks** | — | `adapter_stacks` (configuration) |

**Pattern:** Large files go to filesystem, metadata/relationships go to SQLite.

### Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `AOS_VAR_DIR` | `var` | Base directory for all runtime data |
| `AOS_MODEL_CACHE_DIR` | `var/model-cache` | Downloaded model cache |
| `AOS_ADAPTERS_DIR` | `var/adapters` | LoRA adapter storage |
| `AOS_ARTIFACTS_DIR` | `var/artifacts` | Training artifacts |
| `AOS_DATASETS_DIR` | `var/datasets` | Training datasets |
| `AOS_DOCUMENTS_DIR` | `var/documents` | Uploaded documents |
| `AOS_MODEL_PATH` | `./models/qwen2.5-7b` | Active model directory |
| `AOS_EMBEDDING_MODEL_PATH` | `./models/bge-small-en-v1.5` | Embedding model |
| `HF_TOKEN` | (none) | HuggingFace authentication |
| `AOS_HF_REGISTRY_URL` | `https://huggingface.co` | Model registry URL |

### Path Resolution

```rust
use adapteros_platform::common::PlatformUtils;

// Runtime data (relative to cwd)
let var_dir = PlatformUtils::aos_var_dir();           // var/
let cache = PlatformUtils::aos_model_cache_dir();     // var/model-cache
let adapters = PlatformUtils::aos_adapters_dir();     // var/adapters
let artifacts = PlatformUtils::aos_artifacts_dir();   // var/artifacts

// Path expansion for user paths (tilde expansion)
let expanded = PlatformUtils::expand_path("~/custom/path")?;

// User-specific directories (optional, for cross-project data)
let user_cache = PlatformUtils::aos_user_cache_dir()?;   // ~/.cache/adapteros
let user_config = PlatformUtils::aos_user_config_dir()?; // ~/.config/adapteros
```

### Configuration (TOML)

Paths are also configurable via `configs/cp.toml`:

```toml
[paths]
artifacts_root = "var/artifacts"
bundles_root = "var/bundles"
adapters_root = "var/adapters"
datasets_root = "var/datasets"
documents_root = "var/documents"
plan_dir = "plan"

[db]
path = "var/aos-cp.sqlite3"

[alerting]
alert_dir = "var/alerts"
```

**Production paths** use `/var/lib/adapteros/` prefix (see `configs/production-multinode.toml`).

---

## Training Pipeline (6 Steps)

1. `DocumentIngestor::new(opts, tok).ingest_pdf_path(path)?`
2. `generate_training_data(&doc, &tok, &cfg)?`
3. `TrainingDatasetManager::new(db, path, tok).create_dataset_from_documents(req).await?`
4. `MicroLoRATrainer::new(cfg)?.train(examples, id).await?`
5. `AdapterPackager::new().package(weights, manifest)?`
6. Register adapter in database (conditional on `post_actions.register`)

**Templates:** `general-code` (rank=16, alpha=32), `framework-specific` (rank=12, alpha=24), `codebase-specific` (rank=24, alpha=48), `ephemeral-quick` (rank=8, alpha=16)

**Categories:** code, framework, codebase, ephemeral

**Post-Actions:** Configurable via `post_actions` in StartTrainingRequest:
- `package`: Package adapter after training (default: true)
- `register`: Register in database (default: true)
- `tier`: persistent | warm | ephemeral (default: warm)
- `adapters_root`: Custom output directory

**Advanced Config:** LR schedules (constant/linear/cosine), early stopping, checkpoints, multi-backend GPU support (CoreML/MLX/Metal)

---

## REST API (~250+ routes)

**Source:** `crates/adapteros-server-api/src/routes.rs` | **Auth:** JWT Ed25519

| Category | Base Path | Key Operations |
|----------|-----------|----------------|
| Health | `/healthz`, `/readyz` | Health, readiness |
| Auth | `/v1/auth/*` | login, logout, me, refresh, sessions |
| Adapters | `/v1/adapters/*` | CRUD, load/unload, pin, lifecycle, hot-swap, lineage |
| Stacks | `/v1/adapter-stacks/*` | Create, activate, deactivate |
| Training | `/v1/training/*` | Jobs, start, cancel, templates, artifacts |
| Datasets | `/v1/datasets/*` | Upload, validate, preview, chunked upload |
| Inference | `/v1/infer` | Batch, streaming |
| Policies | `/v1/policies/*` | List, validate, apply, sign |
| Metrics | `/v1/metrics/*` | System, adapters, time-series |
| Audit | `/v1/audit/*` | Logs, compliance |
| Collections | `/v1/collections/*` | Document grouping |
| Documents | `/v1/documents/*` | Upload, retrieval |
| Evidence | `/v1/evidence/*` | Provenance tracking |
| Chat | `/v1/chat/*` | Sessions, messages, tags, sharing |
| Models | `/v1/models/*` | List, import, load/unload |
| Tenants | `/v1/tenants/*` | CRUD, policies, adapters |
| Workspaces | `/v1/workspaces/*` | CRUD, members, resources |
| Notifications | `/v1/notifications/*` | List, mark read |
| Dashboard | `/v1/dashboard/*` | Config, widgets |
| Streams (SSE) | `/v1/stream/*` | Metrics, telemetry, adapters |

**Additional categories:** Activity, Code Intelligence, Contacts, Federation, Git, Golden Runs, Monitoring, Nodes, Plugins, Replay, Routing, Services, Settings, Workers. See `routes.rs` for complete list.

**OpenAPI:** `/swagger-ui`, `/api-docs/openapi.json`

---

## CLI (70+ command groups)

```bash
# Core
aosctl adapter [list|register|load|unload|pin|swap|info|health]
aosctl stack [list|create|activate|deactivate|delete]
aosctl node [list|verify|sync [verify|push|pull]]
aosctl train / aosctl infer

# Operations
aosctl init / init-tenant / doctor / preflight / status / diag
aosctl golden [init|create|list|verify|show]
aosctl router [calibrate|validate|show|safe-mode|decisions]
aosctl policy [list|explain|enforce|hash-status|hash-verify]

# Development
aosctl dev [up|down|status|logs] / chat / deploy / maintenance / bootstrap
aosctl audit / audit-determinism / verify / replay / rollback
aosctl telemetry [list|verify] / registry [sync|migrate]
aosctl federation verify / codegraph [stats|export] / secd [status|audit]

# Additional
aosctl model-import / build-plan / backend-status / determinism
aosctl check / completions / config / error-codes / quarantine
```

**Note:** Deprecated command aliases exist for backwards compatibility. See `main.rs` for full command list.

---

## UI Architecture (90% complete)

| Directory | Pages | Purpose |
|-----------|-------|---------|
| `/pages/OwnerHome/` | 2 | Owner dashboard, system health, CLI console |
| `/pages/DocumentLibrary/` | 4 | Document management, chat |
| `/pages/System/` | 12 | Memory, metrics, nodes, workers |
| `/pages/Training/` | 17 | Datasets, jobs, templates |
| `/pages/Admin/` | 16 | Stacks, tenants, users, plugins |
| `/pages/Adapters/` | 13 | Detail, lifecycle, lineage |
| `/pages/Security/` | 8 | Policies, audit, compliance |

**410+ components** in `/components/chat/`, `/collections/`, `/documents/`, `/dashboard/`, `/adapters/`, `/ui/`, `/audit/`, `/federation/`, `/golden/`, `/inference/`, `/monitoring/`, `/observability/`, `/policies/`, `/workflows/`, `/workspaces/`

---

## CI/CD (16 workflows)

ci, deploy, integration-tests, metal-build, migration-testing, multi-backend, infrastructure-health, schema-drift-detection, security-regression-tests, type-validation, check-merge-conflicts, **duplication**, **architectural-lint**, **e2e-ui-tests**, **stress-tests**, **performance-regression**

---

## Quick Commands

```bash
# Build CLI (creates ./aosctl symlink)
make cli
./aosctl --help

# Build & test
cargo build --release && cargo test --workspace
cargo fmt --all && cargo clippy --workspace -- -D warnings
make dup  # Check duplication

# Database
./aosctl db migrate
./aosctl init-tenant --id default --uid 1000 --gid 1000

# Server
make dev          # Start dev server (port 8080)
make dev-no-auth  # Start without auth (debug only)

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
