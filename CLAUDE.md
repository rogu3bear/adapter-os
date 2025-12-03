# AdapterOS Assistant Guide

## 1. Purpose & Scope

### What is AdapterOS?

AdapterOS is an offline-capable, UMA-optimized orchestration layer for multi-LoRA systems, targeting Mac/Metal environments and unified memory machines. It provides:

- **Control Plane** (`adapteros-server`): HTTP API with SQLite, JWT auth, and policy enforcement
- **Worker Processes** (`aos-worker`): LoRA-based inference and training over Unix Domain Sockets (UDS)
- **K-Sparse Router**: Multi-adapter mixing with Q15 quantization
- **Multi-Backend**: Metal, CoreML/ANE, and MLX support

**Key Characteristics:**
- Single-node by default (future: "bunch of Macs" topology)
- Multi-tenant at logical level: tenants own datasets, documents, adapters, stacks, and policies
- Base models and workers are shared resources
- Strongly focused on determinism, replay, tenant isolation, and auditable policies
- Adapters hot-swap as an alternative to stuffing tokens into context

### Scope of This Assistant

This assistant can:
- Read and edit Rust, TypeScript, SQL, and configuration code
- Run tests and build commands
- Reason about architecture, invariants, and design decisions
- Help implement features, fix bugs, and refactor code

---

## 2. Core Principles & Invariants

### 2.1 Determinism and Replay

**Seed Derivation:**
- Uses HKDF-SHA256 with BLAKE3 global seed
- Full entropy isolation: `manifest_hash + adapter_dir_hash + worker_id + label + nonce`
- Location: `crates/adapteros-core/src/seed.rs`

**Router Determinism:**
- IEEE 754 deterministic softmax (f64 intermediate precision intended)
- Sorted by score DESC, then index ASC for tie-breaking
- Q15 gate quantization: `gate_f32 = q15 / 32767.0` (denominator is 32767, NOT 32768)
- BLAKE3 decision hashing for audit trail
- Location: `crates/adapteros-lora-router/src/lib.rs`

**Replay Metadata (Stored per inference):**
- `manifest_hash`, `router_seed`, `sampling_params_json`, `backend`
- `rag_snapshot_hash` (BLAKE3 of sorted doc hashes), `adapter_ids_json`
- `prompt_text`, `response_text` (64KB limit with truncation flag)
- Location: `crates/adapteros-db/src/replay_metadata.rs`

**Critical: `router_seed` is stored for AUDIT onlyrouting is deterministic by algorithm, not RNG.**

### 2.2 Tenant Isolation

**Handler-Level (security/mod.rs):**
```rust
validate_tenant_isolation(claims, resource_tenant_id)
```
- Same tenant: always allowed
- Admin cross-tenant: requires explicit `admin_tenants` claim
- Dev mode bypass: debug builds only (`AOS_DEV_NO_AUTH`)

**Database-Level (migration 0131):**
- Composite FKs: `FOREIGN KEY (tenant_id, document_id) REFERENCES documents(tenant_id, id)`
- 15+ triggers prevent cross-tenant references
- Orphan detection fails migration if data integrity is broken

**CRITICAL: All queries involving tenant-scoped resources MUST include `tenant_id` filter.**

### 2.3 Policy & Audit

**Hook Points:**
- `OnRequestBeforeRouting`  before adapter selection
- `OnBeforeInference`  after routing, before inference
- `OnAfterInference`  after inference completes

**24 Canonical Policy Packs:**
Core (always enabled): Egress, Determinism, Isolation, Evidence

Additional: Router, Refusal, Numeric, RAG, Telemetry, Retention, Performance, Memory, Artifacts, Secrets, Build_Release, Compliance, Incident, Output, Adapters, Deterministic_IO, Drift, MPLoRA, Naming, Dependency_Security

**Merkle Chain Audit:**
- BLAKE3 hash chaining: `entry_hash = BLAKE3(entry_data + previous_hash)`
- Ed25519 signatures per audit entry
- Chain verification detects tampering
- Location: `crates/adapteros-db/src/policy_audit.rs`

### 2.4 Egress & Offline Posture

**Default Egress Config:**
```rust
mode: DenyAll
serve_requires_pf: true  // PF firewall required in production
allow_tcp: false
allow_udp: false
uds_paths: ["/var/run/aos"]
```

**Runtime Modes:**
- Dev: `allows_egress: true`
- Staging: `allows_egress: false`, requires allowlist
- Prod: `denies_egress: true` (zero egress)

**Workers have zero network egress (UDS only).**

### 2.5 Critical Invariants (DO NOT BREAK)

| Invariant | Location | Risk |
|-----------|----------|------|
| All inference routes through `InferenceCore` | `inference_core.rs:83` (`route_and_infer`) | Bypassing breaks auditability |
| Routing guard must fail hard | `inference_core.rs:106` (`enter_routed_context`) | Silent failures break audit trail |
| Merkle chain write lock BEFORE read | `deterministic-exec/src/global_ledger.rs:85` (`GlobalTickLedger`) | Race conditions break determinism |
| Q15 denominator is 32767.0 | `lora-router/src/lib.rs:1022,1397` | Precision-critical for gate values |
| FK constraints enabled | `db/lib.rs:466` | `foreign_keys=true` required |
| Tenant FK triggers | Migration 0131 | Cross-tenant leakage risk |

---

## 3. Repository Map

### 3.1 Binaries

| Binary | Crate | Source | Purpose |
|--------|-------|--------|---------|
| `adapteros-server` | adapteros-server | `crates/adapteros-server/src/main.rs` | Control plane HTTP API server |
| `aos-worker` | adapteros-lora-worker | `crates/adapteros-lora-worker/src/bin/aos_worker.rs` | Inference worker process (UDS) |
| `aosctl` | adapteros-cli | `crates/adapteros-cli/src/main.rs` | CLI tool for all operations |

### 3.2 Control Plane Crates

| Crate | Purpose |
|-------|---------|
| `adapteros-server` | HTTP server bootstrap, shutdown coordination |
| `adapteros-server-api` | 300+ REST endpoints, handlers, middleware |
| `adapteros-orchestrator` | Training job orchestration |
| `adapteros-registry` | Adapter/model registry |
| `adapteros-federation` | Cross-node communication (future) |

### 3.3 Worker/Data Plane Crates

| Crate | Purpose |
|-------|---------|
| `adapteros-lora-worker` | Core worker: UDS server, inference pipeline |
| `adapteros-lora-router` | K-sparse routing with Q15 gates |
| `adapteros-lora-kernel-mtl` | Metal GPU backend |
| `adapteros-lora-kernel-coreml` | CoreML/ANE backend |
| `adapteros-lora-mlx-ffi` | MLX backend (experimental) |
| `adapteros-lora-lifecycle` | Adapter state machine |

### 3.4 Shared Libraries

| Crate | Purpose |
|-------|---------|
| `adapteros-core` | Domain types, errors, BLAKE3/HKDF, seed derivation |
| `adapteros-types` | Base type definitions |
| `adapteros-api-types` | Request/response types |
| `adapteros-db` | SQLite abstraction, 137 migrations |
| `adapteros-config` | Config loading with precedence (CLI > env > manifest) |
| `adapteros-crypto` | Ed25519, BLAKE3, HKDF, AES-GCM |
| `adapteros-policy` | 24 policy packs, enforcement hooks |
| `adapteros-telemetry` | Event collection, Merkle bundles |
| `adapteros-manifest` | Adapter manifest V3 format |

### 3.5 UI

| Location | Purpose |
|----------|---------|
| `ui/` | React 18 + TypeScript + Vite + TanStack Query |
| `ui/src/api/client.ts` | API client (5,395 lines) |
| `ui/src/components/` | 139+ component directories |
| `ui/src/hooks/` | 108 custom hooks |

---

## 4. Working Process for Changes

### 4.1 Before Making Changes

1. **Identify the subsystem**: inference, replay, RAG, training, worker health, policies, config, UI
2. **Read relevant code**: Don't propose changes to code you haven't read
3. **Check for invariants**: Look for `CRITICAL`, `INVARIANT`, `SECURITY` comments
4. **Understand test coverage**: Find related tests in `tests/` and `crates/*/tests/`

### 4.2 Making Changes

1. **Make minimal, targeted changes** that preserve invariants
2. **Avoid over-engineering**: Don't add features, refactor, or "improve" beyond what's asked
3. **Security-first**: Be careful not to introduce OWASP Top 10 vulnerabilities
4. **Preserve determinism**: Any change touching inference/routing must maintain reproducibility

### 4.3 After Making Changes

1. **Run appropriate tests**:
   ```bash
   cargo test -p <crate>           # Crate-specific
   cargo test --workspace          # All tests
   make determinism-check          # Determinism suite
   ```

2. **Check formatting and lints**:
   ```bash
   cargo fmt --all
   cargo clippy --workspace
   ```

3. **Verify migrations** (if schema changed):
   ```bash
   cargo sqlx migrate run
   # Verify signatures if needed
   ```

4. **Summarize impact**: Document effects on determinism, isolation, policy auditability, egress posture

---

## 5. Tools & Commands

### 5.1 Development

```bash
make dev               # Start control plane (port 8080, NO_AUTH=1)
make ui-dev            # Start UI (port 3200)
make test              # Run all tests (excluding MLX)
make determinism-check # Determinism test suite
make check             # fmt + clippy + test
make metal             # Build Metal shaders
make security-audit    # Vulnerabilities, licenses, SBOM
```

### 5.2 Database

```bash
cargo sqlx migrate run    # Apply migrations
./scripts/migrate.sh      # Migration helper script
```

### 5.3 Testing

```bash
cargo test -p adapteros-lora-router          # Router tests
cargo test -p adapteros-db                   # DB tests
cargo test -p adapteros-server-api           # API tests
cargo test -- --test-threads=1               # Sequential (debugging)
cargo test -- --nocapture                    # With output
LOOM_MAX_PREEMPTIONS=3 cargo test test_name  # Concurrency tests
```

### 5.4 Environment Assumptions

- **Platform**: macOS with Metal GPU (Linux supported without GPU acceleration)
- **Memory**: Unified Memory Architecture (UMA) assumed
- **Database**: SQLite at `var/aos-cp.sqlite3`
- **Default Model**: Qwen2.5-7B

---

## 6. Change Safety Guidelines

### 6.1 NEVER Do These

- **Bypass `InferenceCore` routing guard**  all inference must go through it
- **Change Q15 denominator** from 32767.0  precision-critical
- **Use `unwrap()`/`expect()` in crypto code**  handle errors properly
- **Skip `tenant_id` validation** in DB queries  tenant isolation is critical
- **Move types across crate boundaries** without handling migrations/API impact
- **Modify Merkle chain logic** without proper locking
- **Store secrets in code or config files**
- **Renumber migrations**  `migration_conflicts.rs` enforces this
- **Use `-ffast-math`** or similar compiler flags  breaks determinism

### 6.2 ALWAYS Do These

- **Run `cargo fmt --all` and `cargo clippy --workspace`**
- **Include `tenant_id` filters** in all multi-tenant queries
- **Use `Result`-based error handling** in security code
- **Verify migration signatures** after schema changes
- **Add tests** for new functionality, especially determinism-related
- **Document impact** on determinism, isolation, and auditability

### 6.3 When Uncertain

- **Fail loudly** rather than silently
- **Add tests** to verify assumptions
- **Document limitations** clearly
- **Ask for clarification** before making risky changes

---

## 7. Style & Conventions

### 7.1 Error Handling

```rust
// Use thiserror for error enums
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MyError {
    #[error("description: {0}")]
    Variant(String),
}

// Use Result alias
pub type Result<T> = std::result::Result<T, AosError>;
```

### 7.2 Logging

```rust
// Use tracing, not println!
use tracing::{info, warn, error, debug, trace};

info!(request_id = %id, latency_ms = ms, "message");
warn!(tenant_id = %tid, "potential issue");
error!(error = ?e, "operation failed");
```

### 7.3 Feature Flags

```toml
[features]
default = ["deterministic-only", "coreml-backend"]
```

Feature flags affect determinism and available backends. Check feature configuration before assuming capabilities.

### 7.4 Commit Messages

When committing:
- Summarize the "why" not the "what"
- Note impact on: determinism, tenant isolation, policy enforcement, egress
- Reference PRD numbers if applicable (e.g., PRD-02, PRD-03, PRD-09)

---

## 8. Key Workflows

### 8.1 Inference Flow

```
HTTP Request � InferenceCore � Worker Selection � UDS Request
                    �
              Policy Hooks (OnBeforeInference)
                    �
              Router Decision (K-sparse)
                    �
              Kernel Execution (Metal/CoreML/MLX)
                    �
              Response + Evidence + Replay Metadata
```

### 8.2 Training Flow

```
API Request � TrainingService � Validation (tenant, dataset, evidence)
                    �
              Orchestrator (spawn job, cancel token)
                    �
              Worker (MicroLoRATrainer)
                    �
              Epoch Loop (metrics emission, cancel check)
                    �
              Adapter Packaging (Q15) � Registry � Stack Creation
```

### 8.3 RAG Flow

```
Query � Embedding � Vector Retrieval (tenant-scoped)
                    �
              Collection Filtering
                    �
              Deterministic Ordering (score DESC, doc_id ASC)
                    �
              Evidence Storage � Context Injection
```

### 8.4 RAG vs Adapter Positioning

**Core Principle:** RAG augments prompts with retrieved context. Adapters encode persistent behavior via learned weights. Both can run together in a single inference.

**Two Canonical Pipelines:**

```
Pipeline A: Adapter Path (Persistent Behavior)
document → dataset → adapter → stack → router
├── Training creates permanent specialization
├── Deterministic, auditable via policy packs
└── Best for: repeated queries, core domain behavior, compliance

Pipeline B: RAG Path (Query-Time Augmentation)
document → collection → retrieval → context injection
├── Query-time retrieval with evidence tracking
├── Deterministic ordering (score DESC, doc_id ASC)
└── Best for: ad-hoc queries, evolving knowledge bases
```

**Positioning:**

| Aspect | Adapters | RAG |
|--------|----------|-----|
| Persistence | Permanent (trained weights) | Transient (per-query) |
| Determinism | Router algorithm + Q15 gates | Ordering rules + snapshot hash |
| Use case | Core behavior, repeated patterns | Ad-hoc queries, dynamic docs |
| Audit | Manifest hash + adapter IDs | RAG snapshot hash + doc IDs |
| When to prefer | Policy-auditable, stable behavior | Frequently changing knowledge |

**At Query Time (InferenceCore):**

1. RAG retrieval runs on control plane (before worker call)
2. Prompt augmented with RAG context
3. Worker receives augmented prompt
4. Router selects adapters via K-sparse gating
5. Replay stores both: `rag_snapshot_hash` + `adapter_ids_json`

**Guideline:** For long-term, policy-auditable behavior, train an adapter from key datasets. Use RAG for supplementary context that changes frequently.

---

## 9. Known Gaps and Partial Implementations

### Current Behavior vs Intended Invariant

| Area | Current | Intended |
|------|---------|----------|
| Kahan summation | Defined in config, not implemented | f64 + Kahan for router softmax |
| Router f64 precision | f32 throughout | f64 intermediate values |
| Policy enforcement | Framework exists | Full integration at inference |
| Token counting | Heuristic (~4 chars/token) | Actual tokenizer |
| PF rule validation | Returns error | Full packet filter validation |
| 5 policy packs | Pass with warnings | Full implementation |

### Partially Implemented Features

- **Replay**: Infrastructure complete, determinism verification ongoing
- **Federation**: Framework exists, multi-node not production-ready
- **KV backend**: Dual-write mode active, KV-only migration incomplete

---

## 10. File Reference Quick Guide

### Critical Files (Read Before Modifying Nearby Code)

| Area | File | Key Functions |
|------|------|---------------|
| Inference routing | `server-api/src/inference_core.rs` | `route_and_infer:83`, `route_and_infer_replay:354` |
| RAG context | `server-api/src/handlers/rag_common.rs` | `retrieve_rag_context:127`, `store_rag_evidence:249` |
| Chat context | `server-api/src/chat_context.rs` | `build_chat_prompt:77` (multi-turn) |
| Replay | `server-api/src/handlers/replay_inference.rs` | `execute_replay:326`, `check_availability:112` |
| Worker UDS | `lora-worker/src/uds_server.rs` | |
| Router algorithm | `lora-router/src/lib.rs` | `Router:194`, `route_with_adapter_info:943`, `Decision:1385` |
| Tenant isolation | `server-api/src/security/mod.rs` | `validate_tenant_isolation` |
| Policy hooks | `adapteros-policy/src/hooks.rs` | |
| Policy audit | `adapteros-db/src/policy_audit.rs` | `log_policy_decision:111`, `verify_policy_audit_chain:226` |
| Seed derivation | `adapteros-core/src/seed.rs` | `derive_seed:39`, `derive_seed_typed:60` |
| Config loading | `adapteros-config/src/loader.rs` | |
| JWT auth | `server-api/src/auth.rs` | |

### Backend Files

| Backend | File | Key Functions |
|---------|------|---------------|
| Metal | `lora-kernel-mtl/src/lib.rs` | `MetalKernels:192`, `new:255`, `load:1349` |
| CoreML | `lora-kernel-coreml/src/lib.rs` | `CoreMLBackend:875`, `new:967` |
| MLX | `lora-mlx-ffi/src/backend.rs` | `MLXFFIBackend:42` |
| Factory | `lora-worker/src/backend_factory.rs` | `create_backend_with_model:328` |

### Service Layer

| Service | File | Key Methods |
|---------|------|-------------|
| TrainingService | `server-api/src/services/training_service.rs` | trait:38, impl:98 |
| AdapterService | `server-api/src/services/adapter_service.rs` | trait:40, impl:133 |
| Registry | `adapteros-registry/src/lib.rs` | `register_adapter:53` |
| LifecycleManager | `lora-lifecycle/src/lib.rs` | `promote_adapter:1062`, `activate_stack:1862` |

### Test Locations

| Type | Location |
|------|----------|
| E2E tests | `tests/e2e/` |
| Determinism | `tests/determinism/` |
| API tests | `crates/adapteros-server-api/tests/` |
| DB tests | `crates/adapteros-db/tests/` |
| Router golden | `crates/adapteros-lora-router/tests/router_ring_golden.rs` |

---

## 11. Emergency Reference

### If Determinism Breaks

1. Check seed derivation: same inputs � same seeds?
2. Check router sorting: deterministic tie-breaking?
3. Check Q15 quantization: denominator = 32767.0?
4. Check floating point: no fast-math flags?
5. Run `make determinism-check`

### If Tenant Isolation Breaks

1. Check `tenant_id` in all queries
2. Verify FK triggers (migration 0131)
3. Check `validate_tenant_isolation()` calls
4. Review `admin_tenants` claim handling

### If Builds Fail

1. `cargo clean && cargo build`
2. Check feature flags in Cargo.toml
3. Verify SQLx offline mode: `cargo sqlx prepare`
4. Check migration signatures: `migrations/signatures.json`

---

## 12. Common Misconceptions (Avoid These)

### Structs/Services That Do NOT Exist

| Speculated Name | Reality |
|-----------------|---------|
| `StackService` | Use `LifecycleManager::activate_stack()` |
| `TenantService` | Tenant ops are in Db layer directly |
| `BackendCoordinator` | Use `create_backend_with_model()` factory |
| `BackendCapabilities` | Use `BackendHealth` / `PerformanceMetrics` |
| `GlobalLedger` | Actual name: `GlobalTickLedger` |
| `ModelLoader::load_qwen_model_cached` | Does not exist |

### Handler Naming

Handlers do NOT use `handle_*` prefix:
- `handle_infer()` → **`infer()`**
- `handle_streaming_infer()` → **`streaming_infer()`**
- `handle_infer_batch()` → **`batch_infer()`**

### Telemetry Function Names

- `record_inference_event()` → Use `log_event()` or `InferenceEvent` struct
- `record_router_decision_event()` → Use specific `record_*()` functions in `critical_components.rs`

### Chat State

Chat is **multi-turn**, NOT stateless:
- `build_chat_prompt()` at `chat_context.rs:77` retrieves full session history
- Token budget truncation drops oldest messages first
- Context hash computed via BLAKE3 of sorted message IDs

### Replay Path

Replay does **NOT** bypass InferenceCore:
- Goes through `InferenceCore::route_and_infer_replay()` at `replay_inference.rs:516`
- Which calls `route_and_infer(Some(replay_context))`

### Router Seed

`router_seed` is stored for **AUDIT only**:
- Routing is deterministic by algorithm (sorted scores, tie-breaking)
- NOT by RNG seeding
