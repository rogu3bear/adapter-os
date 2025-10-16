# Integration Test Status

## Current State

**Compilation:** ✅ Passing (Phase 1 complete)
**Execution:** ⚠️ Needs fixes (9/10 tests failing)

## Issues Identified

### 1. UNIQUE Constraint Violations
**Error:** `UNIQUE constraint failed: tenants.name`
**Root Cause:** Tests creating "integration_test" tenant multiple times
**Fix Needed:** Use unique tenant names per test or cleanup between tests

### 2. Binary Target Resolution
**Error:** `no bin target named 'aosctl' in default-run packages`
**Root Cause:** `run_aosctl_command` helper trying to run binary directly
**Fix Needed:** Use `cargo run --bin aosctl` or build and reference binary path

### 3. Shared Database State
**Issue:** Tests sharing database connections causing conflicts
**Fix Needed:** Isolate test databases with unique temp files per test

## Passing Tests

1. ✅ `test_cleanup_and_resource_management` (1/10)

## Failing Tests

1. ❌ `test_build_plan_integration` - bin target issue
2. ❌ `test_concurrent_operations` - tenant conflict
3. ❌ `test_end_to_end_workflow` - tenant conflict  
4. ❌ `test_error_handling_and_recovery` - bin target issue
5. ❌ `test_policy_violation_paths` - tenant conflict
6. ❌ `test_serve_integration` - bin target issue
7. ❌ `test_telemetry_ingest_integration` - tenant conflict
8. ❌ `test_telemetry_throughput` - tenant conflict
9. ❌ Additional unnamed test - tenant conflict

## Recommended Fixes

### Priority 1: Tenant Isolation
```rust
// In setup_test_env():
let tenant_id = format!("test_{}", Uuid::new_v4());
db.create_tenant(&tenant_id, false).await?;
```

### Priority 2: Binary Execution
```rust
// In run_aosctl_command():
let binary_path = env::current_exe()?
    .parent().unwrap()
    .parent().unwrap()
    .join("aosctl");
Command::new(&binary_path)
    .args(args)
    .output()
```

### Priority 3: Test Order Independence
- Use `#[serial]` attribute from `serial_test` crate
- Or ensure each test uses unique resources

## Policy Compliance

Per Build & Release Ruleset #15:
- Integration tests should run clean before promotion
- Current state: Infrastructure issues, not core functionality bugs
- Core packages (router, profiler, codegraph) all passing unit tests

## Next Steps

1. Implement tenant isolation with UUIDs
2. Fix binary path resolution
3. Add test cleanup between runs
4. Enable in CI once fixed

