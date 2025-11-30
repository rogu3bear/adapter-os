# Integration Test Quick Reference

## Run Tests

### Documentation Tests (Work Now)
```bash
# All documentation tests
cargo test -p adapteros-server-api --test '*_tests' --test '*_validation*'

# Specific category
cargo test -p adapteros-server-api --test api_consistency_tests
cargo test -p adapteros-server-api --test database_validation_tests
cargo test -p adapteros-server-api --test security_validation_tests
cargo test -p adapteros-server-api --test type_validation_tests

# With output
cargo test -p adapteros-server-api --test database_validation_tests -- --nocapture

# Specific test
cargo test -p adapteros-server-api test_adapter_activations_table_schema
```

### Integration Tests (Once Fixed)
```bash
# Compile without running
cargo test -p adapteros-server-api --no-run

# Run all tests
cargo test -p adapteros-server-api

# Run with ignored tests
cargo test -p adapteros-server-api -- --include-ignored

# Specific test file
cargo test -p adapteros-server-api --test auth_middleware_test

# Specific test
cargo test -p adapteros-server-api test_hydrate_tenant_deterministic -- --ignored
```

## Test Fixtures Usage

### Setup
```rust
let state = setup_state(None).await?;
```

### Create Test Data
```rust
// Adapter
create_test_adapter_default(&state, "adapter-1", "default").await?;

// Or with custom tier/rank
create_test_adapter(&state, "adapter-1", "default", 1, 16).await?;

// Dataset
create_test_dataset(&state, "dataset-1").await?;

// Tenant
create_test_tenant(&state, "tenant-a", "Tenant A").await?;

// Training job
insert_training_job(&state, "job-1", "pending").await?;

// Workspace
let ws_id = create_test_workspace(&state, "workspace-1", "owner-1").await?;

// Notification
let notif_id = create_test_notification(&state, "user-1", "Test").await?;
```

### Get Claims
```rust
let admin = test_admin_claims();
let operator = test_operator_claims();
let viewer = test_viewer_claims();
let compliance = test_compliance_claims();
```

### Cleanup
```rust
delete_test_adapter(&state, "adapter-1").await?;
delete_test_dataset(&state, "dataset-1").await?;
delete_test_training_job(&state, "job-1").await?;
```

## Common Test Patterns

### Basic Test
```rust
#[tokio::test]
async fn test_something() -> anyhow::Result<()> {
    let state = setup_state(None).await?;

    // Test code here

    Ok(())
}
```

### With Data Creation
```rust
#[tokio::test]
async fn test_with_data() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    create_test_adapter_default(&state, "test-1", "default").await?;

    // Test code

    delete_test_adapter(&state, "test-1").await?;
    Ok(())
}
```

### With RBAC
```rust
#[tokio::test]
async fn test_permissions() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let admin = test_admin_claims();
    let viewer = test_viewer_claims();

    // Test admin can do something
    // Test viewer cannot

    Ok(())
}
```

### Multi-tenant
```rust
#[tokio::test]
async fn test_isolation() -> anyhow::Result<()> {
    let state = setup_state(None).await?;

    create_test_tenant(&state, "tenant-a", "A").await?;
    create_test_tenant(&state, "tenant-b", "B").await?;
    create_test_adapter_default(&state, "adapter-a", "tenant-a").await?;
    create_test_adapter_default(&state, "adapter-b", "tenant-b").await?;

    // Test isolation

    Ok(())
}
```

## File Locations

| Item | Path |
|------|------|
| Fixtures | `crates/adapteros-server-api/tests/common/mod.rs` |
| Test files | `crates/adapteros-server-api/tests/*.rs` |
| API state | `crates/adapteros-server-api/src/state.rs` |
| Database | `crates/adapteros-db/src/lib.rs` |
| Types | `crates/adapteros-api-types/src/lib.rs` |

## Documentation

| Document | Purpose |
|----------|---------|
| `RUNNING_CONSISTENCY_TESTS.md` | How to run, troubleshoot, develop tests |
| `TEST_INFRASTRUCTURE_SUMMARY.md` | What was done, test breakdown, metrics |
| `FIXING_TEST_COMPILATION.md` | Step-by-step library compilation fixes |
| `INTEGRATION_TESTS_STATUS.md` | Status report, what's complete |
| `TEST_QUICK_REFERENCE.md` | This file, quick lookup |

## Fix Compilation

```bash
# Check errors
cargo check -p adapteros-server-api 2>&1 | grep "^error" | head -10

# Follow FIXING_TEST_COMPILATION.md for:
# 1. Add schema_version to ErrorResponse
# 2. Fix unresolved imports
# 3. Fix type mismatches

# Verify
cargo test -p adapteros-server-api --no-run
```

## Status

| Item | Status |
|------|--------|
| Documentation tests | ✅ Working |
| Fixtures | ✅ Ready |
| Library compilation | ❌ 58 errors |
| Integration tests | ⏸️ Blocked (ignored) |
| Compilation guide | ✅ Complete |

## Help

**For running tests:** See `RUNNING_CONSISTENCY_TESTS.md`
**For test examples:** See `TEST_INFRASTRUCTURE_SUMMARY.md`
**For compilation errors:** See `FIXING_TEST_COMPILATION.md`
**For current status:** See `INTEGRATION_TESTS_STATUS.md`
**For quick lookup:** See `TEST_QUICK_REFERENCE.md` (this file)
