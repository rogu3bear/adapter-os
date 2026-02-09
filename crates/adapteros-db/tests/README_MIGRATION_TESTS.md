# Migration Validation Tests - Quick Reference

## Overview

Comprehensive test suite for validating migrations 0193-0202, covering:
- Schema changes (columns, tables)
- Foreign key constraints
- Triggers (business logic enforcement)
- Indexes (performance optimization)
- Data insertion and integrity
- Cross-migration integration scenarios

**Total Tests**: 34
**File**: `migration_validation_tests.rs`

---

## Running Tests

### All Migration Tests
```bash
cargo test -p adapteros-db migration_validation_tests
```

### With Output
```bash
cargo test -p adapteros-db migration_validation_tests -- --nocapture
```

### Single Migration
```bash
# Migration 0194 - Stop Controller
cargo test -p adapteros-db test_migration_0194

# Migration 0197 - Prefix KV Cache
cargo test -p adapteros-db test_migration_0197

# Migration 0199 - Evidence Envelopes
cargo test -p adapteros-db test_migration_0199

# Migration 0201 - Adapter Version Publish
cargo test -p adapteros-db test_migration_0201
```

### Specific Test
```bash
cargo test -p adapteros-db test_migration_0201_scope_tenant_isolation_trigger
```

### Integration Tests
```bash
cargo test -p adapteros-db test_integration
```

---

## Test Categories

### Schema Validation (10 tests)
Tests that verify columns and tables exist:
- `test_migration_0193_receipt_accounting_columns_exist`
- `test_migration_0194_stop_controller_columns_exist`
- `test_migration_0195_tenant_kv_columns_exist`
- `test_migration_0195_receipt_kv_columns_exist`
- `test_migration_0196_replay_stop_policy_column_exists`
- `test_migration_0197_prefix_templates_table_exists`
- `test_migration_0197_prefix_templates_columns`
- `test_migration_0198_model_cache_identity_column_exists`
- `test_migration_0199_evidence_envelopes_table_exists`
- `test_migration_0199_evidence_envelopes_columns`
- `test_migration_0201_adapter_version_columns_exist`
- `test_migration_0202_adapter_stacks_metadata_column_exists`

### FK Constraint Tests (3 tests)
Tests that verify foreign key constraints work:
- `test_migration_0197_prefix_templates_fk_constraint`
- `test_migration_0199_evidence_fk_constraint`
- `test_migration_0201_scope_tenant_isolation_trigger`

### Trigger Tests (5 tests)
Tests that verify triggers enforce business logic:
- `test_migration_0201_free_mode_no_scope_trigger`
- `test_migration_0201_requires_dataset_needs_scope_trigger`
- `test_migration_0201_scope_tenant_isolation_trigger`

### CHECK Constraint Tests (2 tests)
Tests that verify CHECK constraints:
- `test_migration_0199_evidence_scope_check_constraint`
- `test_migration_0201_attach_mode_check_constraint`

### Index Tests (6 tests)
Tests that verify indexes exist:
- `test_migration_0194_stop_reason_index_exists`
- `test_migration_0195_kv_indexes_exist`
- `test_migration_0197_prefix_indexes_exist`
- `test_migration_0199_evidence_indexes_exist`
- `test_migration_0201_indexes_exist`

### Data Insertion Tests (10 tests)
Tests that verify data can be inserted and queried:
- `test_migration_0194_stop_controller_data_insertion`
- `test_migration_0195_kv_data_insertion`
- `test_migration_0196_replay_stop_policy_data`
- `test_migration_0197_receipt_prefix_columns_exist`
- `test_migration_0198_model_cache_identity_data`
- `test_migration_0199_evidence_unique_sequence_constraint`
- `test_migration_0200_adapter_packages_dropped`
- `test_migration_0202_adapter_stacks_metadata_data`

### Integration Tests (2 tests)
Tests that verify cross-migration scenarios:
- `test_integration_receipt_with_all_new_fields`
- `test_integration_evidence_chain_with_prefix_templates`

---

## Migration Coverage

| Migration | Description | Tests | Status |
|-----------|-------------|-------|--------|
| 0193 | Receipt Accounting (no-op) | 1 | ✅ |
| 0194 | Stop Controller | 3 | ✅ |
| 0195 | KV Quota Residency | 4 | ✅ |
| 0196 | Replay Stop Policy | 2 | ✅ |
| 0197 | Prefix KV Cache | 5 | ✅ |
| 0198 | Model Cache Identity V2 | 2 | ✅ |
| 0199 | Evidence Envelopes | 6 | ✅ |
| 0200 | Drop Adapter Packages | 1 | ✅ |
| 0201 | Adapter Version Publish | 6 | ✅ |
| 0202 | Adapter Stacks Metadata | 2 | ✅ |

---

## Test Patterns

### Column Existence
```rust
let rows = sqlx::query("PRAGMA table_info(table_name)")
    .fetch_all(db.pool()).await?;
let columns: HashSet<String> = rows.iter()
    .map(|row| row.get::<String, _>(1))
    .collect();
assert!(columns.contains("column_name"));
```

### Index Existence
```rust
let index_exists: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM sqlite_master
     WHERE type='index' AND name='index_name'"
).fetch_one(db.pool()).await?;
assert_eq!(index_exists, 1);
```

### FK Constraint
```rust
// Valid FK should succeed
let valid = insert_with_valid_fk().await;
assert!(valid.is_ok());

// Invalid FK should fail
let invalid = insert_with_invalid_fk().await;
assert!(invalid.is_err());
```

### Trigger Validation
```rust
// Valid data should succeed
let valid = insert_valid_data().await;
assert!(valid.is_ok());

// Invalid data should be rejected by trigger
let invalid = insert_invalid_data().await;
assert!(invalid.is_err());
```

---

## CI/CD Integration

### Add to CI Pipeline
```yaml
# .github/workflows/ci.yml
- name: Run Migration Tests
  run: cargo test -p adapteros-db migration_validation_tests --no-fail-fast
```

### Pre-commit Hook
```bash
#!/bin/bash
# .git/hooks/pre-commit
cargo test -p adapteros-db migration_validation_tests --quiet
```

---

## Troubleshooting

### Test Failures

**Symptom**: Column not found
```
Column 'stop_reason_code' missing from inference_trace_receipts
```
**Solution**: Ensure migrations are applied in order. Run:
```bash
cargo test -p adapteros-db test_migration_application
```

**Symptom**: FK constraint violation
```
FOREIGN KEY constraint failed
```
**Solution**: Check that referenced table has required data. Tests create tenants automatically.

**Symptom**: Trigger rejection
```
required_scope_dataset_version_id is required when attach_mode is requires_dataset
```
**Solution**: This is expected behavior. Test validates trigger works correctly.

### Performance Issues

If tests run slowly:
```bash
# Run in parallel (default)
cargo test -p adapteros-db migration_validation_tests

# Run sequentially (for debugging)
cargo test -p adapteros-db migration_validation_tests -- --test-threads=1
```

---

## Adding New Migration Tests

### Template for New Migration

```rust
// =============================================================================
// Migration XXXX: Description
// =============================================================================

#[tokio::test]
async fn test_migration_XXXX_column_exists() -> Result<()> {
    let db = create_test_db().await?;

    let rows = sqlx::query("PRAGMA table_info(table_name)")
        .fetch_all(db.pool())
        .await?;

    let mut columns = HashSet::new();
    for row in rows {
        let col_name: String = row.get(1);
        columns.insert(col_name);
    }

    assert!(columns.contains("new_column"), "new_column missing");

    println!("✓ Migration XXXX: column exists");
    Ok(())
}

#[tokio::test]
async fn test_migration_XXXX_data_insertion() -> Result<()> {
    let db = create_test_db().await?;

    // Setup test data
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('test', 'Test')")
        .execute(db.pool())
        .await?;

    // Insert test data
    let result = sqlx::query("INSERT INTO table_name (column) VALUES (?)")
        .bind("value")
        .execute(db.pool())
        .await;

    assert!(result.is_ok(), "Data insertion should succeed");

    println!("✓ Migration XXXX: data insertion works");
    Ok(())
}
```

---

## References

- **Migration Files**: `migrations/`
- **Test File**: `crates/adapteros-db/tests/migration_validation_tests.rs`
- **Detailed Report**: `MIGRATION_VALIDATION_REPORT.md`
- **Issues Summary**: `MIGRATION_ISSUES_SUMMARY.md`

---

**Last Updated**: 2025-12-12
**Maintainer**: adapterOS Core Team
