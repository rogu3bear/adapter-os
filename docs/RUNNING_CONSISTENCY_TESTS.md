# Running Consistency Integration Tests

**Last Updated:** 2025-11-22
**Status:** 23 test files available, 154+ documentation/integration tests
**Test Framework:** Tokio async runtime, custom fixtures via `tests/common/mod.rs`

## Overview

The adapteros-server-api integration test suite includes 23 test files with **154+ tests** across multiple categories:

1. **Documentation Tests** (~2400 lines) - No external dependencies
2. **Integration Tests** (~800+ lines) - Async tests with database fixtures
3. **Ignored Tests** (~70 tests) - Pending API refactoring, documented with reasons

## Test Files Summary

| File | Type | Tests | Status |
|------|------|-------|--------|
| `api_consistency_tests.rs` | Documentation | 19 | Compiles independently |
| `database_validation_tests.rs` | Documentation | 15 | Compiles independently |
| `security_validation_tests.rs` | Documentation | 15 | Compiles independently |
| `type_validation_tests.rs` | Documentation | 12 | Compiles independently |
| `streaming_inference.rs` | Documentation + impl | 685 lines | In progress |
| `auth_middleware_test.rs` | Integration | 6+ | Uses common fixtures |
| `telemetry.rs` | Integration | 18 ignored | Pending refactoring |
| `workspaces_integration.rs` | Integration | 20 ignored | Pending refactoring |
| `aos_upload_test.rs` | Integration | 10 ignored | Pending refactoring |
| `db_to_api_integration_test.rs` | Integration | 11 ignored | Pending refactoring |
| `directory_adapter_test.rs` | Integration | 4 ignored | Pending refactoring |
| `batch_infer.rs` | Integration | 3 ignored | Pending refactoring |
| `training_api.rs` | Integration | 2 ignored | Pending refactoring |
| `training_control.rs` | Integration | 5 ignored | Pending refactoring |
| `load_model.rs` | Integration | 7 ignored | Pending refactoring |
| `concurrent_operations.rs` | Integration | Empty | Pending implementation |
| `integration.rs` | Integration | 10 ignored | Pending refactoring |
| `activity_integration.rs` | Integration | 1 ignored | Pending refactoring |
| `dashboard_integration.rs` | Integration | 1 ignored | Pending refactoring |
| `notifications_integration.rs` | Integration | 1 ignored | Pending refactoring |
| `git_repository.rs` | Integration | 1 ignored | Pending refactoring |
| `replay_tests.rs` | Integration | 1 ignored | Pending refactoring |
| `api_contracts.rs` | Integration | 21 ignored | Pending refactoring |

## How Tests Are Organized

### 1. Documentation Tests (Compile Independently)

Located in:
- `crates/adapteros-server-api/tests/api_consistency_tests.rs` (846 lines)
- `crates/adapteros-server-api/tests/database_validation_tests.rs` (616 lines)
- `crates/adapteros-server-api/tests/security_validation_tests.rs` (603 lines)
- `crates/adapteros-server-api/tests/type_validation_tests.rs` (555 lines)

**Features:**
- No external dependencies required
- Compile and run independently of main library
- Document expected behavior/invariants
- Provide reference for implementation verification

**Example test:**
```rust
#[test]
fn test_adapter_activations_table_schema() {
    println!("Table: adapter_activations");
    println!("Expected columns:");
    println!("  id (primary key)");
    // ... etc
}
```

### 2. Integration Tests (Using Fixtures)

Located in: `crates/adapteros-server-api/tests/`

**Marked as `#[ignore]` pending refactoring:**
```rust
#[tokio::test]
#[ignore = "Pending API refactoring - setup_test_state needs update"]
async fn test_example() {
    // TODO: Implement once setup_test_state is updated
}
```

**Reason for ignore:** Tests were written against earlier AppState API. The refactoring is documented with specific notes (e.g., "setup_test_state needs update").

## Test Fixtures (Database Setup)

Located in: `crates/adapteros-server-api/tests/common/mod.rs`

### Core Fixtures

#### Database Setup
```rust
pub async fn setup_state(uds_path: Option<&PathBuf>) -> anyhow::Result<AppState>
```
- Creates in-memory SQLite database with all migrations
- Initializes default tenants ("default", "tenant-1")
- Sets up metrics infrastructure
- Returns fully configured AppState for tests

#### Role-Based Claims
```rust
pub fn test_admin_claims() -> Claims          // admin role
pub fn test_operator_claims() -> Claims       // operator role
pub fn test_viewer_claims() -> Claims         // viewer role
pub fn test_compliance_claims() -> Claims     // compliance role
```

#### Data Creation Helpers
```rust
pub async fn create_test_adapter(state, id, tenant, tier, rank) -> Result<()>
pub async fn create_test_adapter_default(state, id, tenant) -> Result<()>
pub async fn create_test_dataset(state, id) -> Result<()>
pub async fn create_test_tenant(state, id, name) -> Result<()>
pub async fn insert_training_job(state, id, status) -> Result<()>
pub async fn create_test_workspace(state, name, owner) -> Result<String>
pub async fn create_test_notification(state, user, title) -> Result<String>
```

#### Cleanup Helpers
```rust
pub async fn delete_test_adapter(state, id) -> Result<()>
pub async fn delete_test_dataset(state, id) -> Result<()>
pub async fn delete_test_training_job(state, id) -> Result<()>
```

### Usage Example

```rust
#[tokio::test]
async fn test_adapter_loading() -> anyhow::Result<()> {
    // Setup
    let state = setup_state(None).await?;
    let admin = test_admin_claims();

    // Create test data
    create_test_adapter_default(&state, "test-adapter", "default").await?;

    // Run test
    assert!(state.db.pool().acquire().await.is_ok());

    // Cleanup
    delete_test_adapter(&state, "test-adapter").await?;

    Ok(())
}
```

## Compilation Status

### Documentation Tests (Working)
```bash
cargo test -p adapteros-server-api --test api_consistency_tests --no-run
cargo test -p adapteros-server-api --test database_validation_tests --no-run
cargo test -p adapteros-server-api --test security_validation_tests --no-run
cargo test -p adapteros-server-api --test type_validation_tests --no-run
```

**Result:** These compile successfully (no external dependencies)

### Running Documentation Tests
```bash
# Run specific test file
cargo test -p adapteros-server-api --test api_consistency_tests

# Run with output
cargo test -p adapteros-server-api --test api_consistency_tests -- --nocapture

# Run specific test
cargo test -p adapteros-server-api test_adapter_activations_table_schema
```

### Integration Tests (Require Library Compilation)
```bash
# Currently blocked due to library compilation errors
# See "Current Issues" section below
```

## Running All Tests

### Step 1: Fix library compilation
```bash
# Check what's blocking compilation
cargo check -p adapteros-server-api

# Fix errors (see issues below)
# Likely requires:
# - Updating operation_tracker.rs imports
# - Resolving missing type definitions
# - Checking middleware dependencies
```

### Step 2: Run documentation tests
```bash
cargo test -p adapteros-server-api --test '*_validation_tests' --test '*_consistency_tests'
```

### Step 3: Run integration tests
```bash
# Once library compiles, run specific integration test
cargo test -p adapteros-server-api --test auth_middleware_test

# Run all tests (including #[ignore]d ones)
cargo test -p adapteros-server-api -- --include-ignored
```

## Current Issues and Fixes

### Issue 1: Library Compilation Errors

**Status:** 58 errors in adapteros-server-api library

**Main categories:**
1. **Unresolved imports**
   - `crate::types::OperationProgressEvent` (operation_tracker.rs:11)
   - Missing type definitions in `types.rs` module

2. **Module resolution errors**
   - Some handler modules referenced but not properly defined
   - Check `handlers/mod.rs` for missing exports

**Fix steps:**
```bash
# First, check what's missing in types module
ls -la crates/adapteros-server-api/src/types*.rs

# Check operation_tracker dependencies
grep -r "OperationProgressEvent" crates/adapteros-server-api/src/

# View error details
cargo check -p adapteros-server-api 2>&1 | grep "error\[E" | head -10
```

### Issue 2: Handler Module Exports

**Likely fix:** Ensure all handler modules are properly exported in `handlers/mod.rs`

```rust
// crates/adapteros-server-api/src/handlers/mod.rs
pub mod adapters;
pub mod auth_enhanced;
pub mod infer;
pub mod training;
pub mod health;
pub mod ready;
pub mod telemetry;
// ... etc - verify all handler modules are listed
```

### Issue 3: Ignored Tests Require API Updates

**Status:** ~70 tests marked with `#[ignore]`

**Reason:** API refactoring in progress. Tests document expected behavior.

**To fix:**
1. Review each test's ignore comment
2. Update test to use current `AppState` API
3. Remove `#[ignore]` attribute
4. Run test: `cargo test --ignored test_name`

**Example of fix needed:**
```rust
// Before (ignored)
#[tokio::test]
#[ignore = "Pending API refactoring - setup_test_state needs update"]
async fn test_hydrate_tenant_deterministic() {
    // TODO: Implement once setup_test_state is updated
}

// After (fixed)
#[tokio::test]
async fn test_hydrate_tenant_deterministic() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    // ... actual test implementation
    Ok(())
}
```

## Test Execution Commands

### Run Only Documentation Tests
```bash
# All documentation tests (safe - no external deps)
cargo test -p adapteros-server-api test_core_routes_defined -- --nocapture
cargo test -p adapteros-server-api test_adapter_activations_table_schema -- --nocapture
cargo test -p adapteros-server-api test_no_hardcoded_api_keys -- --nocapture
```

### Run Specific Test Category
```bash
# API consistency tests
cargo test -p adapteros-server-api --test api_consistency_tests

# Security validation tests
cargo test -p adapteros-server-api --test security_validation_tests

# Database validation tests
cargo test -p adapteros-server-api --test database_validation_tests

# Type validation tests
cargo test -p adapteros-server-api --test type_validation_tests
```

### Run All Tests (with ignored)
```bash
cargo test -p adapteros-server-api -- --include-ignored --nocapture
```

### Run Tests with Output
```bash
cargo test -p adapteros-server-api --test api_consistency_tests -- --nocapture
```

## Test Development Workflow

### Creating a New Integration Test

1. **Use the fixture helpers:**
```rust
#[tokio::test]
async fn test_my_feature() -> anyhow::Result<()> {
    // Setup
    let state = setup_state(None).await?;
    let admin_claims = test_admin_claims();

    // Create test data
    create_test_adapter_default(&state, "adapter-1", "default").await?;

    // Test implementation
    // ...

    // Cleanup
    delete_test_adapter(&state, "adapter-1").await?;

    Ok(())
}
```

2. **Run with output:**
```bash
cargo test -p adapteros-server-api test_my_feature -- --nocapture
```

3. **If test fails, check:**
   - Database fixtures are created properly
   - State is initialized with migrations
   - Claims have correct permissions for the operation

### Updating an Ignored Test

1. **Find the test:**
```bash
grep -n "test_.*ignore" crates/adapteros-server-api/tests/integration.rs
```

2. **Read the ignore reason:**
```rust
#[ignore = "Pending API refactoring - setup_test_state needs update"]
```

3. **Update the test:**
   - Remove `#[ignore]` attribute
   - Use `setup_state(None).await?` instead of `setup_test_state()`
   - Add proper error handling with `-> anyhow::Result<()>`
   - Implement test body

4. **Test it:**
```bash
cargo test -p adapteros-server-api test_name -- --nocapture
```

## Troubleshooting

### Build Fails with "can't find crate"
```
error[E0463]: can't find crate for `sqlx`
```
**Fix:** Ensure dependencies in Cargo.toml:
```toml
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio"] }
```

### Test Compilation Hangs
```bash
# Kill and restart
cargo clean --doc
cargo build -p adapteros-server-api
```

### Database Migration Fails
```
Error: Failed to create in-memory DB: Migration error
```
**Fix:** Check migrations are valid:
```bash
cargo test -p adapteros-db schema_consistency_tests
```

### Fixture Setup Returns Error
```
Error: Failed to create metrics exporter
```
**Fix:** Check metrics_exporter dependencies:
```bash
cargo check -p adapteros-metrics-exporter
```

## Success Criteria

All tests should:
1. Compile without errors: `cargo test -p adapteros-server-api --no-run`
2. Run successfully: `cargo test -p adapteros-server-api`
3. Include proper setup/teardown with fixtures
4. Document expected behavior
5. Have meaningful assertions (not just `println!`)
6. Use `#[ignore]` with explanation only when necessary

## References

- **Test Structure:** `crates/adapteros-server-api/tests/`
- **Fixtures Module:** `crates/adapteros-server-api/tests/common/mod.rs`
- **API State:** `crates/adapteros-server-api/src/state.rs`
- **Database:** `crates/adapteros-db/src/lib.rs`
- **Integration Guide:** See CLAUDE.md "Common Patterns" section

## Next Steps

1. Step 1: Fix library compilation errors (58 errors)
2. Step 2: Run documentation tests to verify setup
3. Step 3: Update ignored tests one by one
4. Step 4: Add new tests for uncovered functionality
5. Step 5: Integrate into CI/CD pipeline
