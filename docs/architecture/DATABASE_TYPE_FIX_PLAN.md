# Database Type Mismatch Fix Plan

## Problem Summary

**Conflict 2**: Type inconsistency between `Db` (SQLite-specific) and `Database` (enum wrapper supporting SQLite + PostgreSQL) causing compilation errors.

### Current Architecture

- **`Db`**: SQLite-specific struct (`adapteros_db::Db`)
  - Has `Db::connect()` method
  - Stores `SqlitePool` internally
  
- **`Database`**: Enum wrapper (`adapteros_db::Database`)
  - Wraps either `Db` (SQLite) or `PostgresDb` (PostgreSQL)
  - Has `Database::connect()` that auto-detects backend from URL
  - Provides unified interface for both backends
  
- **`PolicyHashWatcher`**: Expects `Arc<Database>` (the wrapper)
  - Designed to work with both SQLite and PostgreSQL

### Identified Errors

1. **`hash_watcher.rs:447`** - Compilation Error
   ```rust
   let db = Db::connect(&db_url).await.unwrap();  // ❌ Missing import
   // Error: use of undeclared type `Db`
   ```
   - Issue: `Db` type not imported in test module
   - Fix: Import `adapteros_db::Db` OR use `Database::connect()` directly

2. **`federation_daemon.rs:378`** - Type Mismatch
   ```rust
   let policy_watcher = PolicyHashWatcher::new(
       Arc::new(db.clone()),  // ❌ db is Arc<Db>, expected Arc<Database>
       ...
   );
   // Error: mismatched types: expected `Database`, found `Db`
   ```
   - Issue: `FederationDaemon` stores `Arc<Db>` but `PolicyHashWatcher` needs `Arc<Database>`
   - Fix: Convert `Db` to `Database` using `Database::new(db)` or `db.into()`

---

## Root Cause Analysis

### Why This Happened

1. **Evolution of Architecture**
   - Initially SQLite-only (`Db`)
   - Later added PostgreSQL support via `Database` wrapper
   - Not all code migrated to use `Database` consistently

2. **Type Confusion**
   - `Db` methods still work and are convenient for SQLite-only tests
   - `Database` wrapper adds indirection but enables multi-backend support
   - Conversion paths exist (`From<Db> for Database`) but not always used

3. **Test Code Lag**
   - Test helpers still use `Db::connect()` directly
   - Tests need to wrap in `Database::new()` or use `Database::connect()`

---

## Fix Strategy

### Option A: Convert at Call Site (Recommended)
**Approach**: Keep `FederationDaemon` using `Arc<Db>`, convert when passing to `PolicyHashWatcher`

**Pros**:
- Minimal changes to `FederationDaemon` structure
- Preserves existing SQLite-specific optimizations
- Only affects call sites

**Cons**:
- Requires conversion at every call site
- Doesn't address root architectural inconsistency

### Option B: Migrate to Database Throughout
**Approach**: Change `FederationDaemon` to use `Arc<Database>` everywhere

**Pros**:
- Consistent architecture
- Enables future PostgreSQL support in federation daemon
- Aligns with `PolicyHashWatcher` expectations

**Cons**:
- More invasive changes
- Requires updating all `FederationDaemon` methods that access `db`

### **Selected: Hybrid Approach**
- Fix immediate compilation errors (Option A)
- Migrate `FederationDaemon` to `Database` in a follow-up (Option B)

---

## Implementation Plan

### Phase 1: Fix Compilation Errors (Immediate)

#### Step 1.1: Fix `hash_watcher.rs` Test
**File**: `crates/adapteros-policy/src/hash_watcher.rs:447`

**Change**:
```rust
// Before
let db = Db::connect(&db_url).await.unwrap();

// After (Option 1 - Use Database directly)
use adapteros_db::Database;
let db = Database::connect(&db_url).await.unwrap();

// After (Option 2 - Import Db and wrap)
use adapteros_db::{Db, Database};
let db_inner = Db::connect(&db_url).await.unwrap();
let db = Database::new(db_inner);
```

**Recommendation**: Use Option 1 (`Database::connect()`) for consistency

#### Step 1.2: Fix `federation_daemon.rs` Test
**File**: `crates/adapteros-orchestrator/src/federation_daemon.rs:378`

**Change**:
```rust
// Before
let db = Db::connect(&db_url).await.unwrap();
let policy_watcher = PolicyHashWatcher::new(
    Arc::new(db.clone()),  // ❌ Type mismatch
    ...
);

// After
use adapteros_db::Database;
let db = Database::connect(&db_url).await.unwrap();
let policy_watcher = PolicyHashWatcher::new(
    Arc::new(db.clone()),  // ✅ Correct type
    ...
);
```

**Also update**: `FederationDaemon::new()` call site to convert `Db` to `Database`:
```rust
// In setup_test_daemon()
let db = Database::connect(&db_url).await.unwrap();
let daemon = FederationDaemon::new(
    Arc::new(federation),
    Arc::new(policy_watcher),
    Arc::new(telemetry),
    Arc::new(db.inner().clone()),  // Extract Db for FederationDaemon
    config,
);
```

**Wait** - This reveals another issue: `FederationDaemon` stores `Arc<Db>` but we're creating `Database`. Need to handle this.

**Better approach**:
```rust
// Keep FederationDaemon signature as-is, convert at PolicyHashWatcher creation
let db_sqlite = Db::connect(&db_url).await.unwrap();
let db_wrapper = Database::new(db_sqlite.clone());
let policy_watcher = PolicyHashWatcher::new(
    Arc::new(db_wrapper),
    ...
);
let daemon = FederationDaemon::new(
    Arc::new(federation),
    Arc::new(policy_watcher),
    Arc::new(telemetry),
    Arc::new(db_sqlite),  // Use original Db
    config,
);
```

---

### Phase 2: Architectural Consistency (Follow-up)

#### Step 2.1: Migrate `FederationDaemon` to `Database`
**File**: `crates/adapteros-orchestrator/src/federation_daemon.rs`

**Changes**:
1. Update struct field:
   ```rust
   // Before
   db: Arc<Db>,
   
   // After
   db: Arc<Database>,
   ```

2. Update constructor:
   ```rust
   // Before
   pub fn new(
       ...
       db: Arc<Db>,
       ...
   )
   
   // After
   pub fn new(
       ...
       db: Arc<Database>,
       ...
   )
   ```

3. Update all internal `db` access:
   - Check if methods use SQLite-specific APIs
   - If yes, add backend matching or use `Database` methods
   - If no, should work as-is via `Deref` trait

#### Step 2.2: Update All Call Sites
**Search for**: `FederationDaemon::new(`

**Update pattern**:
```rust
// Before
let db = Db::connect(...).await?;
let daemon = FederationDaemon::new(..., Arc::new(db), ...);

// After
let db = Database::connect(...).await?;
let daemon = FederationDaemon::new(..., Arc::new(db), ...);
```

---

## Testing Plan

### Unit Tests
1. ✅ Fix `hash_watcher.rs` tests - should compile and pass
2. ✅ Fix `federation_daemon.rs` tests - should compile and pass
3. ✅ Verify `Database::connect()` works with SQLite URLs
4. ✅ Verify `Database::new(Db)` wrapper works correctly

### Integration Tests
1. Test `PolicyHashWatcher` with `Database` wrapper
2. Test `FederationDaemon` with both `Db` and `Database` (if Phase 2 completed)
3. Verify migrations work through `Database::migrate()`

### Compilation Verification
```bash
# Run clippy to catch any type issues
cargo clippy --workspace -- -D warnings

# Check specific crates
cargo check -p adapteros-policy
cargo check -p adapteros-orchestrator
```

---

## Verification Checklist

- [ ] `hash_watcher.rs` compiles without errors
- [ ] `federation_daemon.rs` compiles without errors
- [ ] All tests in `adapteros-policy` pass
- [ ] All tests in `adapteros-orchestrator` pass
- [ ] No new clippy warnings introduced
- [ ] `cargo build --workspace` succeeds
- [ ] Database operations work correctly through `Database` wrapper

---

## Risk Assessment

### Low Risk
- Phase 1 fixes (immediate compilation errors)
- Test-only changes
- Using existing conversion paths (`From<Db> for Database`)

### Medium Risk
- Phase 2 changes (architectural migration)
- Runtime behavior changes if `FederationDaemon` methods assume SQLite
- Need to verify `Deref` trait works correctly for all operations

### Mitigation
- Phase 1 first, verify all tests pass
- Phase 2 can be done incrementally
- Keep both `Db` and `Database` APIs available during transition

---

## Timeline Estimate

- **Phase 1**: 30-60 minutes
  - Fix test imports and type conversions
  - Verify compilation and tests
  
- **Phase 2**: 2-4 hours (optional, follow-up)
  - Migrate `FederationDaemon` to `Database`
  - Update all call sites
  - Verify no runtime regressions

---

## Related Files

- `crates/adapteros-db/src/lib.rs` - Database type definitions
- `crates/adapteros-policy/src/hash_watcher.rs` - Error location #1
- `crates/adapteros-orchestrator/src/federation_daemon.rs` - Error location #2
- `crates/adapteros-federation/src/lib.rs` - May use Db types
- `crates/adapteros-server/src/main.rs` - May instantiate these types

---

## Notes

- `Database` wrapper supports both SQLite and PostgreSQL via enum
- `From<Db> for Database` conversion exists: `Database::new(db)` or `db.into()`
- `Deref` trait allows `Database` to be used as `&Db` (but panics for PostgreSQL)
- Test code should prefer `Database::connect()` for consistency

