# adapterOS Rectification Patch Plan

**Date:** 2025-12-20
**Scope:** PRD-RECT-001 through PRD-RECT-005
**Status:** Ready for implementation

---

## Executive Summary

| PRD | Work Required | Files | Estimate |
|-----|---------------|-------|----------|
| **001** | Add tenant predicates to 5 DB queries + tests | 4 files | Medium |
| **002** | Add 20+ tests + fix `list_worker_incidents` handler | 3 files | Medium |
| **003** | Validate existing (already complete) | 0 files | None |
| **004** | Add 2 migrations + tests for trigger gaps | 3 files | Small |
| **005** | Already implemented | 0 files | None |

**Total: ~8 files to modify, 2 new migrations**

---

## Phase 1: Tenant Isolation DB Queries (PRD-RECT-001)

### 1.1 Files to Modify

```
crates/adapteros-db/src/adapters.rs      # 4 functions
crates/adapteros-db/src/adapters_kv.rs   # 3 trait methods
crates/adapteros-db/src/models.rs        # 1 function + 1 new
crates/adapteros-server-api/src/handlers/system_info.rs  # 1 call site
```

### 1.2 Function Changes

#### `list_adapters_by_category()` (adapters.rs:2106)

```rust
// BEFORE
pub async fn list_adapters_by_category(&self, category: &str) -> Result<Vec<Adapter>>

// AFTER
pub async fn list_adapters_by_category(&self, tenant_id: &str, category: &str) -> Result<Vec<Adapter>>

// SQL: Add "WHERE tenant_id = ? AND active = 1 AND category = ?"
```

#### `list_adapters_by_scope()` (adapters.rs:2131)

```rust
// BEFORE
pub async fn list_adapters_by_scope(&self, scope: &str) -> Result<Vec<Adapter>>

// AFTER
pub async fn list_adapters_by_scope(&self, tenant_id: &str, scope: &str) -> Result<Vec<Adapter>>

// SQL: Add "WHERE tenant_id = ? AND active = 1 AND scope = ?"
```

#### `list_adapters_by_state()` (adapters.rs:2156)

```rust
// BEFORE
pub async fn list_adapters_by_state(&self, state: &str) -> Result<Vec<Adapter>>

// AFTER
pub async fn list_adapters_by_state(&self, tenant_id: &str, state: &str) -> Result<Vec<Adapter>>

// SQL: Add "WHERE tenant_id = ? AND active = 1 AND current_state = ?"
```

#### `get_adapter_state_summary()` (adapters.rs:2181)

```rust
// BEFORE
pub async fn get_adapter_state_summary(&self) -> Result<Vec<(...)>>

// AFTER
pub async fn get_adapter_state_summary(&self, tenant_id: &str) -> Result<Vec<(...)>>

// SQL: Add "WHERE tenant_id = ? AND active = 1" before GROUP BY
```

#### `list_models()` (models.rs:359) - Add new function

```rust
// Keep existing for admin tooling, add tenant-scoped version:
pub async fn list_models_for_tenant(&self, tenant_id: &str) -> Result<Vec<Model>> {
    // SQL: "WHERE tenant_id = ? OR tenant_id IS NULL" (includes global models)
}
```

### 1.3 Callers to Update

| File | Line | Change |
|------|------|--------|
| `handlers/system_info.rs` | 124 | `list_adapters_by_state(&claims.tenant_id, "hot")` |
| `tests/adapter_schema_tests.rs` | 279,285,294 | Add `"default"` tenant param |
| `tests/adapter_stress_tests.rs` | 152-154,465-467 | Add `"default"` tenant param |
| `handlers/models.rs` | 1393 | Use `list_models_with_stats_for_tenant(&claims.tenant_id)` |

### 1.4 Tests to Add

```rust
// crates/adapteros-db/tests/adapter_tenant_isolation_queries.rs (new file)

#[tokio::test]
async fn list_adapters_by_category_respects_tenant_isolation()

#[tokio::test]
async fn list_adapters_by_scope_respects_tenant_isolation()

#[tokio::test]
async fn list_adapters_by_state_respects_tenant_isolation()

#[tokio::test]
async fn get_adapter_state_summary_respects_tenant_isolation()

#[tokio::test]
async fn list_models_for_tenant_respects_tenant_isolation()

#[tokio::test]
async fn list_models_for_tenant_includes_global_models()
```

---

## Phase 2: Worker Lifecycle Tests (PRD-RECT-002)

### 2.1 Files to Modify

```
crates/adapteros-server-api/tests/worker_lifecycle_tenant_scoping.rs  # Add tests
crates/adapteros-server-api/src/handlers.rs                           # Fix list_worker_incidents
crates/adapteros-server-api/tests/common/mod.rs                       # Add helpers
```

### 2.2 Handler Fix Required

**`list_worker_incidents` (handlers.rs:2867-2919)**

```rust
// BEFORE (gap - no tenant check)
let worker = state.db.get_worker(&worker_id).await?;

// AFTER (tenant-scoped)
let worker = state.db.get_worker_for_tenant(&claims.tenant_id, &worker_id).await?;
match worker {
    None => return Ok(StatusCode::NOT_FOUND.into_response()),
    Some(w) => { /* proceed */ }
}
```

### 2.3 Tests to Add (20+ cases)

```rust
// Extend: crates/adapteros-server-api/tests/worker_lifecycle_tenant_scoping.rs

// === Path Traversal Prevention ===
#[tokio::test]
async fn test_worker_spawn_rejects_tenant_id_with_path_traversal_dots()

#[tokio::test]
async fn test_worker_spawn_rejects_tenant_id_with_forward_slash()

#[tokio::test]
async fn test_worker_uds_path_cannot_escape_tenant_directory()

// === Telemetry Event Routing ===
#[tokio::test]
async fn test_telemetry_event_includes_correct_tenant_id()

#[tokio::test]
async fn test_telemetry_events_for_different_tenants_are_isolated()

// === Incident Listing ===
#[tokio::test]
async fn test_list_worker_incidents_returns_404_for_cross_tenant()

#[tokio::test]
async fn test_list_worker_incidents_cross_tenant_indistinguishable_from_not_found()

// === Register Worker ===
#[tokio::test]
async fn test_register_worker_validates_plan_tenant_match()

#[tokio::test]
async fn test_register_worker_rejected_when_plan_not_found()

// === Notify Worker Status ===
#[tokio::test]
async fn test_notify_worker_status_updates_same_tenant_worker()

#[tokio::test]
async fn test_notify_worker_status_returns_404_for_nonexistent_worker()

#[tokio::test]
async fn test_notify_worker_status_records_telemetry_with_correct_tenant()
```

---

## Phase 3: Cache Eviction (PRD-RECT-003)

### Status: COMPLETE - No Changes Required

The cache implementation is production-ready:
- LRU eviction with 4 blocking factors (pinned, active, re-validation, over-limit)
- Prometheus metrics exposed
- 22 tests in `model_handle_cache_eviction.rs`
- Q15 uses 32767.0 correctly throughout

**Action:** None. Monitor in production.

---

## Phase 4: DB Trigger Gaps (PRD-RECT-004)

### 4.1 New Migrations Required

#### Migration `0223_training_job_tenant_guards.sql`

```sql
-- Triggers for repository_training_jobs tenant isolation

-- repo_id must match tenant
CREATE TRIGGER trg_training_jobs_repo_tenant_match_insert
BEFORE INSERT ON repository_training_jobs
FOR EACH ROW WHEN NEW.repo_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapter_repositories WHERE id = NEW.repo_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: repository_training_jobs.tenant_id must match adapter_repositories.tenant_id')
    END;
END;

-- base_version_id must match tenant
CREATE TRIGGER trg_training_jobs_base_version_tenant_insert
BEFORE INSERT ON repository_training_jobs
FOR EACH ROW WHEN NEW.base_version_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapter_versions WHERE id = NEW.base_version_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: training job base_version_id references version from different tenant')
    END;
END;

-- produced_version_id must match tenant
CREATE TRIGGER trg_training_jobs_produced_version_tenant_insert
BEFORE INSERT ON repository_training_jobs
FOR EACH ROW WHEN NEW.produced_version_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapter_versions WHERE id = NEW.produced_version_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: training job produced_version_id references version from different tenant')
    END;
END;

-- draft_version_id must match tenant
CREATE TRIGGER trg_training_jobs_draft_version_tenant_insert
BEFORE INSERT ON repository_training_jobs
FOR EACH ROW WHEN NEW.draft_version_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapter_versions WHERE id = NEW.draft_version_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: training job draft_version_id references version from different tenant')
    END;
END;

-- (Also add UPDATE triggers for each column)
```

#### Migration `0224_stack_adapter_ids_tenant_guard.sql`

```sql
-- Validate adapter_ids_json entries belong to stack's tenant

CREATE TRIGGER trg_adapter_stacks_adapter_ids_tenant_check
BEFORE INSERT ON adapter_stacks
FOR EACH ROW
WHEN NEW.adapter_ids_json IS NOT NULL
     AND NEW.adapter_ids_json != '[]'
BEGIN
    SELECT CASE
        WHEN EXISTS (
            SELECT 1 FROM json_each(NEW.adapter_ids_json) AS j
            WHERE NOT EXISTS (
                SELECT 1 FROM adapters a
                WHERE a.id = j.value AND a.tenant_id = NEW.tenant_id
            )
        )
        THEN RAISE(ABORT, 'Tenant mismatch: adapter_stacks.adapter_ids_json contains adapters from different tenant')
    END;
END;

-- (Also add UPDATE trigger)
```

### 4.2 Tests to Add

```rust
// crates/adapteros-db/tests/tenant_trigger_isolation.rs (extend)

#[tokio::test]
async fn trigger_rejects_stack_with_cross_tenant_adapter_ids()

#[tokio::test]
async fn trigger_allows_stack_with_same_tenant_adapter_ids()

#[tokio::test]
async fn trigger_rejects_training_job_cross_tenant_repo()

#[tokio::test]
async fn trigger_rejects_training_job_cross_tenant_base_version()

#[tokio::test]
async fn trigger_rejects_training_job_cross_tenant_produced_version()
```

### 4.3 Update Migration Signatures

```bash
# After adding migrations, regenerate signatures
cargo run -p adapteros-db --bin generate_migration_signatures
```

---

## Phase 5: Model Loading (PRD-RECT-005)

### Status: COMPLETE - Already Implemented

Per AGENTS.md invariants table:
- Model cache budget required (no panics)
- GQA config validation is fatal
- Sharded model completeness checked

**Action:** None.

---

## Implementation Order

```
Week 1: PRD-RECT-001 (DB queries)
├── Day 1-2: Modify 5 functions in adapters.rs/models.rs
├── Day 3: Update KV trait + handlers
└── Day 4-5: Add tests, verify with cargo test

Week 2: PRD-RECT-002 (Worker tests) + PRD-RECT-004 (Triggers)
├── Day 1: Fix list_worker_incidents handler
├── Day 2-3: Add worker lifecycle tests
├── Day 4: Add trigger migrations
└── Day 5: Add trigger tests, verify
```

---

## Verification Commands

```bash
# After each phase:
cargo check -p adapteros-db
cargo check -p adapteros-server-api
cargo test -p adapteros-db
cargo test -p adapteros-server-api -- tenant_isolation
cargo test -p adapteros-server-api -- worker_lifecycle
cargo clippy -p adapteros-db -p adapteros-server-api -- -D warnings

# Final verification:
cargo test --workspace
cargo test --test determinism_core_suite -- --test-threads=8
cargo test -p adapteros-lora-router --test determinism
bash scripts/check_fast_math_flags.sh
```

---

## File Summary

### Files to Create
- `migrations/0223_training_job_tenant_guards.sql`
- `migrations/0224_stack_adapter_ids_tenant_guard.sql`
- `crates/adapteros-db/tests/adapter_tenant_isolation_queries.rs` (optional, can extend existing)

### Files to Modify
- `crates/adapteros-db/src/adapters.rs` (4 functions)
- `crates/adapteros-db/src/adapters_kv.rs` (3 trait methods)
- `crates/adapteros-db/src/models.rs` (1 new function)
- `crates/adapteros-server-api/src/handlers.rs` (list_worker_incidents fix)
- `crates/adapteros-server-api/src/handlers/system_info.rs` (1 call site)
- `crates/adapteros-server-api/tests/worker_lifecycle_tenant_scoping.rs` (add tests)
- `crates/adapteros-db/tests/tenant_trigger_isolation.rs` (add tests)
- `crates/adapteros-db/tests/adapter_schema_tests.rs` (update calls)
- `migrations/signatures.json` (regenerate)

### Files Unchanged (Already Complete)
- `crates/adapteros-lora-worker/src/model_handle_cache.rs`
- `crates/adapteros-lora-worker/src/backend_factory.rs`
- `crates/adapteros-lora-router/src/lib.rs` (Q15 = 32767.0)
- `crates/adapteros-config/src/path_resolver.rs`

---

## Risk Assessment

| Change | Risk | Mitigation |
|--------|------|------------|
| DB query signature changes | Medium - breaks callers | Update all callers in same PR |
| New migrations | Low - additive only | Test in :memory: first |
| Handler fix | Low - straightforward | Existing test patterns |
| Q15 constant | None | Already correct |
