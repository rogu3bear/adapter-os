# Dependency Consolidation & Duplicate Export Resolution

**Generated:** 2025-11-21
**Updated:** 2025-11-22
**Commit Reference:** 2a1bd063 (main)

## Changes & Consolidation Status

**Completed Consolidations (2025-11-22):**
- `adapteros-single-file-adapter` crate deleted and consolidated into `adapteros-aos`
- All AOS format handling now unified in `adapteros-aos` crate
- Format specification simplified: single unified, versionless AOS format
- Updated references throughout documentation to reflect unified archive format

## Executive Summary

Analysis identified **16 duplicate type definitions**, **6 dependency version mismatches**, and multiple consolidation opportunities in the AdapterOS workspace.

---

## Part 1: Duplicate Type Definitions

### Critical Duplicates (Different Semantics - Need Rename)

#### 1. `LifecycleState` - 2 Definitions (DIFFERENT SEMANTICS)

| Location | Variants | Purpose |
|----------|----------|---------|
| `adapteros-core/src/lifecycle.rs:58` | Draft, Active, Deprecated, Retired | Adapter maturity/availability lifecycle |
| `adapteros-types/src/adapters/metadata.rs:141` | Registered, Loading, Active, Inactive, Unloading, Unloaded, Expired, Error | Runtime load state |

**Recommendation:** Rename `adapteros-types` version to `AdapterLoadState` to disambiguate.

**Files to update:**
- `crates/adapteros-types/src/adapters/metadata.rs` - Rename enum
- `crates/adapteros-types/src/adapters/mod.rs` - Update export

---

#### 2. `ForkType` - 2 Definitions (DIFFERENT SEMANTICS)

| Location | Variants | Purpose |
|----------|----------|---------|
| `adapteros-core/src/naming.rs:462` | Independent, Extension | Lineage relationship type |
| `adapteros-db/src/metadata.rs:135` | Parameter, Data, Architecture | Modification tracking type |

**Recommendation:** Rename `adapteros-core` version to `LineageForkType`.

**Files to update:**
- `crates/adapteros-core/src/naming.rs` - Rename enum
- `crates/adapteros-core/src/lib.rs` - Update exports (line 58, prelude)
- `crates/adapteros-registry/src/lib.rs` - Update import (line 3)
- `tests/adapter_taxonomy_integration.rs` - Update import (line 10)

---

#### 3. `WorkflowType` - 3 Definitions (IDENTICAL SEMANTICS)

| Location | Variants |
|----------|----------|
| `adapteros-db/src/metadata.rs:163` | Parallel, UpstreamDownstream, Sequential |
| `adapteros-lora-lifecycle/src/workflow_executor.rs:19` | Same |
| `adapteros-server-api/src/handlers/adapter_stacks.rs:52` | Same (+ ToSchema) |

**Recommendation:** Define canonical `WorkflowType` in `adapteros-types` or `adapteros-core`, import elsewhere.

---

#### 4. `MploraConfig` - 3 Definitions (PARTIALLY OVERLAPPING)

| Location | Fields |
|----------|--------|
| `adapteros-policy/src/mplora.rs:228` | Basic: shared_downsample, compression_ratio, orthogonal_constraints, similarity_threshold, penalty_weight, history_window |
| `adapteros-lora-kernel-api/src/lib.rs:661` | Same as above (identical) |
| `adapteros-policy/src/packs/mplora.rs:13` | Extended: adds path_constraints, performance_constraints |

**Recommendation:**
1. Keep extended version in `adapteros-policy/src/packs/mplora.rs` as `MploraConfig`
2. Rename basic version to `MploraKernelConfig` in `adapteros-lora-kernel-api`
3. Remove duplicate from `adapteros-policy/src/mplora.rs`

---

#### 5. `PolicyValidationResult` - 2 Definitions (SAME CRATE)

| Location |
|----------|
| `adapteros-policy/src/policy_packs.rs:304` |
| `adapteros-policy/src/unified_enforcement.rs:114` |

**Recommendation:** Keep in `unified_enforcement.rs`, remove from `policy_packs.rs`.

---

### Naming Collisions (Different Domains - Need Disambiguation)

#### 6. `HealthStatus` - 5+ Definitions

| Location | Suggested Rename |
|----------|------------------|
| `adapteros-core/src/status.rs:14` | `SystemHealthStatus` |
| `adapteros-lora-mlx-ffi/src/monitoring.rs:49` | `BackendHealthStatus` |
| `adapteros-service-supervisor/src/service.rs:65` | `ServiceHealthStatus` |
| `adapteros-lora-worker/src/health.rs:39` | Keep `ProcessHealthStatus` |
| `adapteros-tui/src/app/api.rs:267` | `UIHealthStatus` |

---

#### 7. `ValidationResult` - 6+ Definitions

| Location | Suggested Rename |
|----------|------------------|
| `adapteros-policy/src/hash_watcher.rs:34` | `HashValidationResult` |
| `adapteros-lora-worker/src/patch_validator.rs:25` | `PatchValidationResult` |
| `adapteros-error-recovery/src/validation.rs:18` | `RecoveryValidationResult` |
| `adapteros-server-api/src/validation/response_schemas.rs:29` | `SchemaValidationResult` |
| `adapteros-aos/src/aos2_writer.rs:42` | `AdapterValidationResult` |
| `adapteros-cli/src/commands/datasets.rs:355` | `DatasetValidationResult` |

---

## Part 2: Dependency Version Mismatches

### Critical: `axum` 0.7 vs 0.8

| Crate | Version | Action |
|-------|---------|--------|
| `adapteros-service-supervisor` | 0.7 | **Upgrade to 0.8** |
| Workspace | 0.8 | Reference |

**Change:**
```toml
# crates/adapteros-service-supervisor/Cargo.toml line 14
- axum = { version = "0.7", features = ["json", "macros", "tracing"] }
+ axum = { version = "0.8", features = ["json", "macros", "tracing"] }
```

---

### Critical: `reqwest` 0.11 vs 0.12

| Crate | Version | Action |
|-------|---------|--------|
| `xtask` | 0.11 | Upgrade to 0.12 |
| `adapteros-testing` | 0.11 | Upgrade to 0.12 |
| `adapteros-client` | 0.11 | Upgrade to 0.12 |
| Workspace | 0.12 | Reference |

**Changes:**
```toml
# xtask/Cargo.toml line 20
- reqwest = { version = "0.11", features = ["json", "blocking"] }
+ reqwest = { version = "0.12", features = ["json", "blocking"] }

# crates/adapteros-testing/Cargo.toml line 18
- reqwest = { version = "0.11", features = ["json"] }
+ reqwest = { version = "0.12", features = ["json"] }

# crates/adapteros-client/Cargo.toml line 20
- reqwest = { version = "0.11", features = ["json"] }
+ reqwest = { version = "0.12", features = ["json"] }
```

---

### Critical: `sqlx` Runtime Feature Mismatch

| Crate | Features | Action |
|-------|----------|--------|
| `adapteros-codegraph` | runtime-tokio-rustls | Change to runtime-tokio |
| Workspace | runtime-tokio | Reference |

**Change:**
```toml
# crates/adapteros-codegraph/Cargo.toml line 20
- sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "sqlite"] }
+ sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
```

---

## Part 3: Workspace Dependency Consolidation

### `tokio` - 14 Crates Using Direct Specification

Convert from `tokio = { version = "1.0", ... }` to `tokio = { workspace = true, features = [...] }`:

| Crate | Current | Action |
|-------|---------|--------|
| `adapteros-core` | `version = "1.0"` | `workspace = true` |
| `adapteros-error-recovery` | `version = "1.0"` | `workspace = true` |
| `adapteros-storage` | `version = "1.0"` | `workspace = true` |
| `adapteros-concurrent-fs` | `version = "1.0"` | `workspace = true` |
| `adapteros-patch` | `version = "1.0"` | `workspace = true` |
| `adapteros-temp` | `version = "1.0"` | `workspace = true` |
| `adapteros-lint` | `version = "1.0"` | `workspace = true` |
| `adapteros-aos` | `version = "1.0"` | `workspace = true` |
| `adapteros-server-api` | `version = "1.0"` | `workspace = true` |
| `adapteros-service-supervisor` | `version = "1.0"` | `workspace = true` |
| `deprecated/adapteros-experimental` | `version = "1.0"` | `workspace = true` |
| `test-status-writer` | `version = "1.0"` | `workspace = true` |
| `tests/unit` | `version = "1.0"` | `workspace = true` |

---

### `serde` - 26 Crates Using Direct Specification

Convert from `serde = { version = "1.0", ... }` to `serde = { workspace = true }`:

All crates in the grep output for `serde = { version = "1.0"` should be updated.

---

## Part 4: Implementation Priority

### High Priority (Breaking/Conflicting)
1. **axum 0.7 → 0.8** in service-supervisor
2. **sqlx runtime-tokio-rustls → runtime-tokio** in codegraph
3. **reqwest 0.11 → 0.12** in xtask, testing, client

### Medium Priority (Type Disambiguation)
4. Rename `LifecycleState` → `AdapterLoadState` in adapteros-types
5. Rename `ForkType` → `LineageForkType` in adapteros-core
6. Consolidate `WorkflowType` to single definition
7. Consolidate `MploraConfig` definitions

### Low Priority (Maintenance)
8. Convert tokio specifications to workspace
9. Convert serde specifications to workspace
10. Rename `HealthStatus` variants with domain prefixes
11. Rename `ValidationResult` variants with domain prefixes

---

## Verification Commands

```bash
# After changes, verify build
cargo build --workspace

# Run tests
cargo test --workspace

# Check for unused dependencies
cargo +nightly udeps --workspace

# Format and lint
cargo fmt --all
cargo clippy --workspace -- -D warnings
```

---

**Author:** Generated by Claude Code
**Reference:** CLAUDE.md Guidelines
