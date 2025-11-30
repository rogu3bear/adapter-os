# Section to Add to CLAUDE.md

Add this section under the "Database" section, after the migration information:

---

## Atomic Dual-Write with Rollback (Phase 4)

**Status:** Phase 4 preparation (implementation pending)
**Location:** `crates/adapteros-db/src/adapters.rs`, `crates/adapteros-db/src/lib.rs`
**Documentation:** `docs/ATOMIC_DUAL_WRITE_IMPLEMENTATION.md`

### Overview

Atomic dual-write provides configurable consistency guarantees when transitioning from SQL-primary to KV-primary storage mode.

### Configuration

```rust
use adapteros_db::{Db, adapters::AtomicDualWriteConfig};

// Best-effort mode (default) - KV failures logged but don't fail operations
let db = Db::connect("./var/cp.db").await?
    .with_atomic_dual_write_config(AtomicDualWriteConfig::best_effort());

// Strict atomic mode - KV failures trigger SQL rollback and return error
let db = Db::connect("./var/cp.db").await?
    .with_atomic_dual_write_config(AtomicDualWriteConfig::strict_atomic());

// From environment variable
std::env::set_var("AOS_ATOMIC_DUAL_WRITE_STRICT", "true");
let config = AtomicDualWriteConfig::from_env();
```

### Modes

| Mode | KV Write Fails | SQL Rollback | Error Returned | Use Case |
|------|----------------|--------------|----------------|----------|
| **Best-effort** (default) | Log warning | No | No | Phase 3: Dual-write validation |
| **Strict atomic** | Log error | Yes (if possible) | Yes | Phase 4: Pre-KV-primary transition |

### Rollback Behavior

**Registration (`register_adapter_extended`):**
- SQL INSERT succeeds → KV write fails → DELETE from SQL (rollback)
- If DELETE fails: Log CRITICAL, return error requiring manual intervention

**Updates (`update_adapter_state_tx`, `update_adapter_memory_tx`, etc):**
- SQL UPDATE commits → KV write fails → **Cannot rollback** (transaction already committed)
- Log "CONSISTENCY WARNING" and return error
- Use `ensure_consistency()` to repair

**Deletes (`delete_adapter`, `delete_adapter_cascade`):**
- SQL DELETE succeeds → KV delete fails → **Cannot rollback**
- Log warning about orphaned KV entry (cleaned up in Phase 5)

### Consistency Validation & Repair

```rust
// Repair single adapter (SQL is source of truth)
db.ensure_consistency("adapter-123").await?;

// Batch repair
let adapter_ids = vec!["adapter-1".to_string(), "adapter-2".to_string()];
let results = db.ensure_consistency_batch(&adapter_ids).await;

// Validate entire tenant
let (consistent, inconsistent, errors) = db.validate_tenant_consistency("tenant-123").await?;
println!("Consistent: {}, Inconsistent: {}, Errors: {}", consistent, inconsistent, errors);
```

### Phase 4 Migration Path

1. **Phase 3 (current):** Best-effort dual-write (SQL primary, KV secondary)
   ```rust
   let db = Db::connect(path).await?
       .with_storage_mode(StorageMode::DualWrite)
       .with_atomic_dual_write_config(AtomicDualWriteConfig::best_effort());
   ```

2. **Phase 4a:** Enable strict atomic dual-write
   ```rust
   let db = Db::connect(path).await?
       .with_storage_mode(StorageMode::DualWrite)
       .with_atomic_dual_write_config(AtomicDualWriteConfig::strict_atomic());
   ```

3. **Phase 4b:** Validate consistency across all tenants
   ```bash
   aosctl db validate-consistency --tenant all --repair
   ```

4. **Phase 4c:** Switch to KV-primary mode (reads from KV, writes to both)
   ```rust
   let db = Db::connect(path).await?
       .with_storage_mode(StorageMode::KvPrimary)
       .with_atomic_dual_write_config(AtomicDualWriteConfig::strict_atomic());
   ```

5. **Phase 5:** KV-only mode (deprecate SQL writes)
   ```rust
   let db = Db::connect(path).await?
       .with_storage_mode(StorageMode::KvOnly);
   ```

### Error Handling

**Critical Rollback Failure:**
```
ERROR CRITICAL: Failed to rollback SQL insert after KV failure - database inconsistency detected
  adapter_id="adapter-123"
  original_error="connection timeout"
  rollback_error="adapter not found"
```
**Action:** Manually inspect database, delete adapter from SQL if needed, or run `ensure_consistency()`

**Consistency Warning (Update Methods):**
```
ERROR CONSISTENCY WARNING: SQL state update committed but KV write failed in strict mode. Use ensure_consistency() to repair.
  adapter_id="adapter-123"
  state="warm"
```
**Action:** Run `db.ensure_consistency("adapter-123").await?`

### Monitoring

**Metrics to Track:**
- Dual-write success rate (% where both SQL and KV succeed)
- Rollback count (SQL rollbacks due to KV failures)
- Consistency check results (consistent/inconsistent/error counts)
- Repair operations (times `ensure_consistency()` fixes issues)

**Log Patterns:**
```rust
// Success
DEBUG Adapter written to both SQL and KV backends adapter_id="..."

// Best-effort KV failure
WARN Failed to write adapter to KV backend (dual-write, best-effort mode) adapter_id="..." error="..."

// Strict mode rollback
ERROR KV write failed in strict atomic mode - rolling back SQL insert adapter_id="..." error="..."

// Consistency repair
WARN Inconsistency detected between SQL and KV - repairing from SQL adapter_id="..."
DEBUG Successfully repaired adapter consistency adapter_id="..."
```

### CLI Commands

```bash
# Validate consistency for specific tenant
aosctl db validate-consistency --tenant default

# Validate and repair all tenants
aosctl db validate-consistency --tenant all --repair

# Check consistency for specific adapter
aosctl db ensure-consistency --adapter adapter-123

# Enable strict atomic mode via config
export AOS_ATOMIC_DUAL_WRITE_STRICT=true
aosctl server start
```

### Testing

**Unit Tests:** `crates/adapteros-db/tests/atomic_dual_write_tests.rs`
```bash
cargo test atomic_dual_write --package adapteros-db
```

**Integration Tests:** Run with KV backend enabled
```bash
cargo test --package adapteros-db --test schema_consistency_tests -- atomic_dual_write --ignored
```

### Files Modified

- `crates/adapteros-db/src/adapters.rs` - Add `AtomicDualWriteConfig`, update mutation methods, add `ensure_consistency()`
- `crates/adapteros-db/src/lib.rs` - Add config field to `Db`, add accessors
- `crates/adapteros-db/tests/atomic_dual_write_tests.rs` - Unit tests
- `crates/adapteros-cli/src/main.rs` - Add `db validate-consistency` command

### Implementation Status

- [ ] Add `AtomicDualWriteConfig` struct to `adapters.rs`
- [ ] Add config field to `Db` struct
- [ ] Update `register_adapter_extended()` with rollback logic
- [ ] Update update methods with strict mode handling
- [ ] Implement `ensure_consistency()` and batch/tenant variants
- [ ] Add unit tests
- [ ] Add CLI commands
- [ ] Add monitoring/metrics
- [ ] Update CLAUDE.md

See `docs/ATOMIC_DUAL_WRITE_IMPLEMENTATION.md` for complete implementation details.

---

**Add this to Configuration System section:**

```
ATOMIC_DUAL_WRITE | AOS_ATOMIC_DUAL_WRITE_STRICT | Atomic dual-write mode | bool (false)
```

**Add this to Anti-Patterns section:**

| Avoid | Fix |
|-------|-----|
| Ignoring consistency errors in strict mode | Run `ensure_consistency()` to repair |
| Manual SQL fixes without syncing KV | Use `ensure_consistency()` to propagate changes |
