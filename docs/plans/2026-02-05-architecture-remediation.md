# Architecture Remediation Plan

## Executive Summary

This document outlines a remediation plan for three architectural violations identified in the AdapterOS topology audit:

1. **Policy-DB Inversion**: `adapteros-policy` directly depends on `adapteros-db`, creating a core-to-infrastructure coupling that violates the layered architecture.
2. **Core Database Dependencies**: `adapteros-core` contains `rusqlite` and `sqlx` dependencies, when it should be a pure types/algorithms crate.
3. **Server-API Hub-Spoke Inversion**: Spoke crates (`adapteros-server-api-models`, `adapteros-server-api-audit`, etc.) depend on the hub (`adapteros-server-api`) instead of both depending on shared types.

The goal is to restore proper layer separation where:
- Core layer is pure types/algorithms with no I/O dependencies
- Domain layer (policy) defines interfaces, infrastructure layer (db) implements them
- API layer uses shared types rather than creating circular dependencies

**Total Estimated Effort**: 2-3 sprints (M-L combined)

---

## Issue 1: Break `adapteros-policy` -> `adapteros-db` Dependency

### Current State

The `adapteros-policy` crate has a direct dependency on `adapteros-db`:

```toml
# crates/adapteros-policy/Cargo.toml
[dependencies]
adapteros-db = { path = "../adapteros-db" }
```

The coupling exists primarily in `hash_watcher.rs`:

```rust
// crates/adapteros-policy/src/hash_watcher.rs:12
use adapteros_db::Db;

pub struct PolicyHashWatcher {
    db: Arc<Db>,
    // ...
}
```

The `PolicyHashWatcher` calls:
- `db.insert_policy_hash()`
- `db.get_policy_hash()`
- `db.list_policy_hashes()`
- `db.pool()` for raw SQL queries

### Target State

Policy defines traits for persistence operations, and db implements them:

```
adapteros-policy (domain)
    │
    └── defines PolicyHashStore trait
           │
           ▼
adapteros-db (infrastructure)
    │
    └── implements PolicyHashStore for Db
```

The `PolicyHashWatcher` would accept `Arc<dyn PolicyHashStore>` instead of `Arc<Db>`.

### Migration Steps

#### Step 1: Define PolicyHashStore Trait (S)

Create trait in `adapteros-policy/src/traits.rs`:

```rust
#[async_trait]
pub trait PolicyHashStore: Send + Sync {
    async fn insert_policy_hash(
        &self,
        policy_pack_id: &str,
        baseline_hash: &B3Hash,
        cpid: Option<&str>,
        signer_pubkey: Option<&str>,
    ) -> Result<()>;

    async fn get_policy_hash(
        &self,
        policy_pack_id: &str,
        cpid: Option<&str>,
    ) -> Result<Option<PolicyHashRecord>>;

    async fn list_policy_hashes(
        &self,
        cpid: Option<&str>,
    ) -> Result<Vec<PolicyHashRecord>>;

    /// Execute quarantine record insertion
    async fn insert_quarantine_record(
        &self,
        reason: &str,
        cpid: Option<&str>,
        violation_type: &str,
    ) -> Result<()>;
}
```

Move `PolicyHashRecord` struct to `adapteros-policy` as well.

#### Step 2: Implement Trait in adapteros-db (S)

```rust
// crates/adapteros-db/src/policy_hash.rs
impl PolicyHashStore for Db {
    // Move existing methods here with trait signature
}
```

Add `adapteros-policy` as a dev-dependency (for the trait, not the full crate):
```toml
[dependencies]
adapteros-policy-traits = { path = "../adapteros-policy-traits" }
```

Or use feature flags to conditionally include the trait implementation.

#### Step 3: Update PolicyHashWatcher (M)

Change constructor and field:

```rust
pub struct PolicyHashWatcher {
    store: Arc<dyn PolicyHashStore>,
    // ...
}

impl PolicyHashWatcher {
    pub fn new(
        store: Arc<dyn PolicyHashStore>,
        telemetry: Arc<TelemetryWriter>,
        cpid: Option<String>,
    ) -> Self {
        Self { store, telemetry, /* ... */ }
    }
}
```

Replace all `self.db.method()` calls with `self.store.method()`.

#### Step 4: Update All Callers (S)

Update construction sites to pass `Db` as `dyn PolicyHashStore`:

```rust
let watcher = PolicyHashWatcher::new(
    state.db.clone() as Arc<dyn PolicyHashStore>,
    // ...
);
```

#### Step 5: Remove Dependency (XS)

Remove from `crates/adapteros-policy/Cargo.toml`:
```toml
# Remove this line
adapteros-db = { path = "../adapteros-db" }
```

### Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking change in trait signature | Medium | Low | Define comprehensive trait upfront |
| Performance regression from dynamic dispatch | Low | Low | `Arc<dyn>` overhead is negligible for DB ops |
| Circular dependency during migration | Medium | Medium | Use separate traits crate if needed |

### Estimated Effort

**T-shirt Size: M (Medium)**

- Trait definition: 1-2 hours
- Db implementation: 2-4 hours
- PolicyHashWatcher refactor: 4-6 hours
- Caller updates: 2-3 hours
- Testing: 4-6 hours
- **Total: 13-21 hours (~2-3 days)**

---

## Issue 2: Remove `rusqlite`/`sqlx` from `adapteros-core`

### Current State

`adapteros-core` has database dependencies that violate its role as a pure types/algorithms crate:

```toml
# crates/adapteros-core/Cargo.toml
[dependencies]
rusqlite = { workspace = true }
sqlx = { workspace = true, optional = true }
```

These are used for:
1. Error conversions in `src/error.rs` and `src/errors/mod.rs`
2. `From<rusqlite::Error>` and `From<sqlx::Error>` implementations for `AosError`

Current usage in `src/errors/mod.rs`:
```rust
impl From<rusqlite::Error> for AosError {
    fn from(err: rusqlite::Error) -> Self {
        AosError::Storage(AosStorageError::Sqlite(err.to_string()))
    }
}

#[cfg(feature = "sqlx")]
impl From<sqlx::Error> for AosError {
    fn from(err: sqlx::Error) -> Self {
        AosError::Storage(AosStorageError::Sqlx(err.to_string()))
    }
}
```

Similar conversions exist in `src/errors/storage.rs`.

### Target State

- `adapteros-core` has zero database dependencies
- Error conversions are moved to the db crate or a dedicated error-conversion crate
- Core provides generic storage error variants that db maps into

### Migration Steps

#### Step 1: Audit All Database Error Usages (S)

Current files with db error imports:
- `crates/adapteros-core/src/error.rs` (legacy)
- `crates/adapteros-core/src/errors/mod.rs`
- `crates/adapteros-core/src/errors/storage.rs`
- `crates/adapteros-core/src/error_helpers.rs` (may reference)

#### Step 2: Move Conversions to adapteros-db (M)

In `crates/adapteros-db/src/error_conversions.rs`:

```rust
use adapteros_core::errors::{AosError, AosStorageError};

impl From<rusqlite::Error> for AosError {
    fn from(err: rusqlite::Error) -> Self {
        AosError::Storage(AosStorageError::Sqlite(err.to_string()))
    }
}

impl From<sqlx::Error> for AosError {
    fn from(err: sqlx::Error) -> Self {
        AosError::Storage(AosStorageError::Sqlx(err.to_string()))
    }
}
```

#### Step 3: Remove From Impls from Core (S)

Delete the `From<rusqlite::Error>` and `From<sqlx::Error>` implementations from:
- `src/errors/mod.rs`
- `src/errors/storage.rs`

#### Step 4: Update Cargo.toml (XS)

```toml
# Remove these from adapteros-core/Cargo.toml
rusqlite = { workspace = true }  # DELETE
sqlx = { workspace = true, optional = true }  # DELETE

# Remove feature flag
[features]
sqlx = ["dep:sqlx"]  # DELETE
```

#### Step 5: Update Downstream Crates (M)

Crates that depend on `adapteros-core` with `features = ["sqlx"]`:
- `adapteros-db/Cargo.toml`: `adapteros-core = { path = "../adapteros-core", features = ["sqlx", "cache-attestation"] }`
- `adapteros-server-api/Cargo.toml`: `adapteros-core = { path = "../adapteros-core", features = ["sqlx"] }`

After removing the feature, update these to not request it:
```toml
adapteros-core = { path = "../adapteros-core", features = ["cache-attestation"] }
```

#### Step 6: Test Compilation Chain (S)

Run full workspace build to catch any missed usages:
```bash
cargo build --workspace
cargo test --workspace
```

### Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Orphan rule violations | Low | High | Conversions in adapteros-db are fine (it owns the types) |
| Downstream compile failures | Medium | Medium | Comprehensive feature flag audit |
| Runtime behavior change | Low | Low | Conversions are just string wrapping |

### Estimated Effort

**T-shirt Size: S (Small)**

- Audit: 1-2 hours
- Move conversions: 2-3 hours
- Update Cargo.toml files: 1 hour
- Test & fix: 2-3 hours
- **Total: 6-9 hours (~1 day)**

---

## Issue 3: Fix Server-API Hub-Spoke Inversion

### Current State

The spoke crates depend on the hub, creating problematic dependencies:

```
adapteros-server-api-models  ──▶  adapteros-server-api (hub)
adapteros-server-api-audit   ──▶  adapteros-server-api (hub)
adapteros-server-api-health  ──▶  adapteros-server-api (hub)
adapteros-server-api-admin   ──▶  adapteros-server-api (hub)
```

From `crates/adapteros-server-api-models/Cargo.toml`:
```toml
[dependencies]
# Hub crate for shared types (spoke pattern)
adapteros-server-api = { path = "../adapteros-server-api" }
```

The spokes import from the hub:
```rust
// crates/adapteros-server-api-models/src/handlers.rs
use adapteros_server_api::api_error::{ApiError, ApiResult};
use adapteros_server_api::audit_helper::{log_failure_or_warn, log_success_or_warn};
use adapteros_server_api::auth::Claims;
use adapteros_server_api::middleware::require_any_role;
use adapteros_server_api::state::AppState;
// ...
```

### Target State

Both hub and spokes depend on a shared types crate:

```
adapteros-server-api-shared (new crate)
    ├── api_error.rs (ApiError, ApiResult)
    ├── auth.rs (Claims, role helpers)
    ├── state.rs (AppState)
    ├── audit_helper.rs
    ├── types.rs (ErrorResponse, common types)
    └── middleware.rs (common middleware types)

adapteros-server-api (hub)       adapteros-server-api-models (spoke)
        │                                    │
        └────────┬───────────────────────────┘
                 │
                 ▼
        adapteros-server-api-shared
```

### Migration Steps

#### Step 1: Create Shared Crate (M)

Create `crates/adapteros-server-api-shared/`:

```bash
mkdir -p crates/adapteros-server-api-shared/src
```

```toml
# crates/adapteros-server-api-shared/Cargo.toml
[package]
name = "adapteros-server-api-shared"
version = "0.0.1"
edition = "2021"

[dependencies]
# Minimal deps - only what's needed for types
axum = { workspace = true }
serde = { workspace = true }
thiserror = { workspace = true }
# ...
```

#### Step 2: Identify Shared Types (M)

Types currently imported by spokes from the hub:
- `ApiError`, `ApiResult` (api_error.rs)
- `Claims`, `require_any_role` (auth.rs + middleware)
- `AppState` (state.rs)
- `audit_helper::log_failure_or_warn`, `log_success_or_warn`
- `ErrorResponse` (types.rs)
- `UdsClient` (uds_client.rs)
- `model_status::aggregate_status`

Not all of these should move - only truly shared types. Some (like `UdsClient`) may stay in the hub.

#### Step 3: Move Shared Types (L)

Move these modules to the shared crate:
- `api_error.rs` -> Full module
- `auth.rs` -> `Claims` struct only (not handlers)
- `state.rs` -> `AppState` struct definition
- `types.rs` -> `ErrorResponse` and common types
- Middleware types (not implementations)

Create re-exports in the hub for backward compatibility:
```rust
// crates/adapteros-server-api/src/lib.rs
pub use adapteros_server_api_shared::{
    api_error, auth, state, types,
};
```

#### Step 4: Update Spoke Dependencies (S)

Change spoke Cargo.toml files:

```toml
# crates/adapteros-server-api-models/Cargo.toml
[dependencies]
adapteros-server-api-shared = { path = "../adapteros-server-api-shared" }
# Remove: adapteros-server-api = { path = "../adapteros-server-api" }
```

#### Step 5: Update Spoke Imports (M)

Change imports in spoke crates:

```rust
// Before
use adapteros_server_api::api_error::{ApiError, ApiResult};
use adapteros_server_api::state::AppState;

// After
use adapteros_server_api_shared::api_error::{ApiError, ApiResult};
use adapteros_server_api_shared::state::AppState;
```

#### Step 6: Verify Hub-Spoke Independence (S)

Ensure the hub can still function with spokes removed:
```bash
cargo build -p adapteros-server-api
cargo build -p adapteros-server-api-models
cargo build -p adapteros-server-api-shared
```

### Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking existing imports | High | Medium | Re-exports in hub for gradual migration |
| Circular dependency in shared types | Medium | High | Careful audit of what moves to shared |
| Build time regression | Low | Low | Shared crate is small |
| API surface area changes | Medium | Medium | Document public API in shared crate |

### Estimated Effort

**T-shirt Size: L (Large)**

- Shared crate setup: 2-3 hours
- Type identification & planning: 3-4 hours
- Move types: 8-12 hours
- Update spokes: 4-6 hours
- Update hub re-exports: 2-3 hours
- Testing: 6-8 hours
- **Total: 25-36 hours (~4-5 days)**

---

## Sprint Allocation

### Recommended Order

1. **Issue 2 (Core DB deps)** - Smallest, enables cleaner architecture discussion
2. **Issue 1 (Policy-DB)** - Medium complexity, clear benefit
3. **Issue 3 (Hub-Spoke)** - Largest, may benefit from patterns learned in 1-2

### Sprint 1 (Week 1-2)

- [ ] Issue 2: Remove database deps from core (S)
- [ ] Issue 1: Define PolicyHashStore trait (part of M)
- [ ] Issue 1: Implement trait in db crate

### Sprint 2 (Week 3-4)

- [ ] Issue 1: Update PolicyHashWatcher (remaining M)
- [ ] Issue 1: Update all callers, remove dependency
- [ ] Issue 3: Create shared crate, identify types (start of L)

### Sprint 3 (Week 5-6)

- [ ] Issue 3: Move types, update spokes (remaining L)
- [ ] Integration testing across all changes
- [ ] Documentation updates

---

## Dependencies Between Issues

```
Issue 2 (Core DB)
    │
    │ (unblocks cleaner error handling discussion)
    ▼
Issue 1 (Policy-DB)
    │
    │ (establishes trait pattern for API layer)
    ▼
Issue 3 (Hub-Spoke)
    │
    │ (may use similar patterns)
    ▼
(complete)
```

Issue 2 has no dependencies and can start immediately.
Issue 1 can start in parallel but benefits from Issue 2's error handling clarity.
Issue 3 is independent but may want to observe patterns from Issue 1.

---

## Success Criteria

### Issue 1
- [ ] `adapteros-policy` compiles without `adapteros-db` dependency
- [ ] All policy hash operations work via trait
- [ ] Tests pass with trait-based architecture

### Issue 2
- [ ] `adapteros-core/Cargo.toml` has no `rusqlite` or `sqlx` entries
- [ ] All crates compile and tests pass
- [ ] Error conversions work correctly

### Issue 3
- [ ] Spoke crates do not depend on `adapteros-server-api`
- [ ] Both hub and spokes depend on `adapteros-server-api-shared`
- [ ] Existing functionality preserved

---

## Appendix: Current Dependency Graph (Relevant Subset)

```
adapteros-core
    ├── rusqlite [TO REMOVE]
    └── sqlx (optional) [TO REMOVE]

adapteros-policy
    ├── adapteros-core
    ├── adapteros-db [TO REMOVE - use trait instead]
    └── adapteros-telemetry

adapteros-db
    ├── adapteros-core
    └── (will impl PolicyHashStore trait)

adapteros-server-api (hub)
    ├── adapteros-core
    ├── adapteros-db
    ├── adapteros-policy
    └── (many others)

adapteros-server-api-models (spoke)
    ├── adapteros-server-api [TO CHANGE to shared]
    └── adapteros-api-types

adapteros-server-api-audit (spoke)
    ├── adapteros-server-api [TO CHANGE to shared]
    └── (others)
```
