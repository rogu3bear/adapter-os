# Testing Guide

This document provides a comprehensive guide to testing in AdapterOS, covering unit tests, integration tests, E2E tests, and determinism verification.

## Table of Contents

1. [Testing Overview](#testing-overview)
2. [Test Commands](#test-commands)
3. [Unit Testing](#unit-testing)
4. [Integration Testing](#integration-testing)
5. [E2E Testing](#e2e-testing)
6. [Determinism Testing](#determinism-testing)
7. [UI Testing](#ui-testing)
8. [Test Patterns](#test-patterns)
9. [Troubleshooting](#troubleshooting)

---

## Testing Overview

AdapterOS has a comprehensive test suite covering multiple layers:

- **Unit Tests**: Individual crate functionality
- **Integration Tests**: API handlers, database interactions, multi-component workflows
- **E2E Tests**: Full system workflows (adapter lifecycle, training, inference)
- **Determinism Tests**: Router and inference reproducibility

### Test Organization

| Location | Purpose |
|----------|---------|
| `tests/` | Root-level E2E and integration tests |
| `crates/*/tests/` | Crate-specific integration tests |
| `crates/*/src/lib.rs` | Inline unit tests (via `#[cfg(test)]` modules) |

---

## Test Commands

### Basic Commands

```bash
# Run all tests (excludes experimental MLX FFI)
cargo test --workspace --exclude adapteros-lora-mlx-ffi

# Single crate
cargo test -p adapteros-server-api

# Specific test file
cargo test --test auth_flow_tests

# Specific test function
cargo test test_hydrate_tenant_deterministic

# With output visible
cargo test -- --nocapture

# Sequential execution (useful for debugging)
cargo test -- --test-threads=1

# Compile tests without running
cargo test --no-run
```

### Makefile Commands

```bash
# Run all checks (fmt + clippy + test + determinism)
make check

# Run all tests + Miri (memory safety)
make test

# Determinism test suite
make determinism-check

# E2E worker startup test
make e2e-worker-test

# MLX-specific tests (if MLX features enabled)
make test-mlx
```

---

## Unit Testing

Unit tests live in `#[cfg(test)]` modules within crate source files.

### Example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seed_derivation() {
        let global_seed = [0u8; 32];
        let seed1 = derive_seed(&global_seed, "context1", "label1");
        let seed2 = derive_seed(&global_seed, "context1", "label1");

        assert_eq!(seed1, seed2, "Same inputs must produce same seed");
    }
}
```

### Run Unit Tests

```bash
# All unit tests in a crate
cargo test -p adapteros-core --lib

# Specific test module
cargo test -p adapteros-core seed_tests

# With output
cargo test -p adapteros-core -- --nocapture
```

---

## Integration Testing

Integration tests verify multi-component interactions, API handlers, and database operations.

### Test Infrastructure

**Fixtures**: Located in `crates/adapteros-server-api/tests/common/mod.rs`

```rust
use common::*;

#[tokio::test]
async fn test_adapter_creation() -> anyhow::Result<()> {
    // Setup test state with in-memory database
    let state = setup_state(None).await?;

    // Create test data
    create_test_adapter_default(&state, "adapter-1", "default").await?;

    // Run test assertions
    let adapter = state.db().get_adapter("adapter-1", "default").await?;
    assert_eq!(adapter.adapter_id, "adapter-1");

    // Cleanup
    delete_test_adapter(&state, "adapter-1").await?;
    Ok(())
}
```

### Available Fixtures

#### Setup
```rust
let state = setup_state(None).await?;  // In-memory DB
```

#### Create Test Data
```rust
// Adapter
create_test_adapter_default(&state, "adapter-1", "default").await?;
create_test_adapter(&state, "adapter-1", "default", 1, 16).await?; // Custom tier/rank

// Dataset
create_test_dataset(&state, "dataset-1").await?;

// Tenant
create_test_tenant(&state, "tenant-a", "Tenant A").await?;

// Training job
insert_training_job(&state, "job-1", "pending").await?;

// Workspace
let ws_id = create_test_workspace(&state, "workspace-1", "owner-1").await?;

// Notification
let notif_id = create_test_notification(&state, "user-1", "Test message").await?;
```

#### RBAC Claims
```rust
let admin = test_admin_claims();       // Full permissions
let operator = test_operator_claims(); // Operational access
let viewer = test_viewer_claims();     // Read-only
let compliance = test_compliance_claims(); // Audit access
```

#### Cleanup
```rust
delete_test_adapter(&state, "adapter-1").await?;
delete_test_dataset(&state, "dataset-1").await?;
delete_test_training_job(&state, "job-1").await?;
```

### Run Integration Tests

```bash
# All integration tests for server-api
cargo test -p adapteros-server-api

# Specific test file
cargo test -p adapteros-server-api --test auth_flow_tests

# With ignored tests included
cargo test -p adapteros-server-api -- --include-ignored

# Database validation tests
cargo test -p adapteros-server-api --test schema_validation

# API consistency tests
cargo test -p adapteros-server-api --test api_consistency_tests

# Security validation
cargo test -p adapteros-server-api --test security_validation_tests
```

---

## E2E Testing

E2E tests verify complete workflows through the system.

### Available E2E Tests

Located in `tests/`:

- `e2e_adapter_lifecycle.rs` - Adapter registration, activation, inference
- `e2e_training_workflow.rs` - Full training pipeline
- `e2e_hotswap_minimal.rs` - Hot-swap adapter replacement
- `e2e_multi_user_rbac.rs` - Multi-tenant RBAC workflows
- `e2e_policy_enforcement.rs` - Policy hook verification
- `e2e_system_stress.rs` - Load and stress testing
- `e2e_pinned_adapters_graceful_degradation.rs` - Pinned adapter fallback

### Run E2E Tests

```bash
# All E2E tests
cargo test --test 'e2e_*'

# Specific workflow
cargo test --test e2e_adapter_lifecycle

# With full output
cargo test --test e2e_training_workflow -- --nocapture

# Worker startup E2E (requires model files)
make e2e-worker-test
```

### E2E Test Pattern

```rust
#[tokio::test]
async fn test_full_inference_workflow() -> anyhow::Result<()> {
    // 1. Start server
    let state = setup_state(None).await?;

    // 2. Register adapter
    let adapter_bytes = std::fs::read("test_data/adapters/test.aos")?;
    register_adapter(&state, "test-adapter", adapter_bytes).await?;

    // 3. Activate stack
    activate_stack(&state, "test-stack", &["test-adapter"]).await?;

    // 4. Run inference
    let response = infer(&state, "test-stack", "Hello world").await?;

    // 5. Verify response
    assert!(!response.output.is_empty());
    assert!(response.evidence.is_some());

    Ok(())
}
```

---

## Determinism Testing

Determinism tests ensure reproducibility of routing and inference decisions.

### Key Invariants

1. **Seed Derivation**: Same inputs must produce same seeds
2. **Router Sorting**: Score DESC, then index ASC for tie-breaking
3. **Q15 Quantization**: Denominator is exactly 32767.0 (NOT 32768)
4. **Replay**: Same inputs + metadata must produce identical outputs

### Run Determinism Tests

```bash
# Determinism test suite
make determinism-check

# Or manually:
cargo test --test determinism_core_suite -- --test-threads=8
cargo test -p adapteros-lora-router --test determinism

# Release mode (faster):
PROFILE=release make determinism-check
```

### Determinism Test Pattern

```rust
#[test]
fn test_router_determinism() {
    let adapters = vec![
        AdapterInfo { id: "a1", gate_q15: 16383 },
        AdapterInfo { id: "a2", gate_q15: 16383 }, // Tied score
    ];

    let decision1 = router.route(&adapters, K_LIMIT);
    let decision2 = router.route(&adapters, K_LIMIT);

    // Must be identical
    assert_eq!(decision1.selected_ids, decision2.selected_ids);
    assert_eq!(decision1.router_seed, decision2.router_seed);
}
```

### Troubleshooting Determinism Issues

1. **Check seed derivation**: Same inputs → same seeds
2. **Verify router sorting**: Score DESC, index ASC tie-break
3. **Confirm Q15 denominator**: Must be 32767.0
4. **Validate replay metadata**: All fields stored correctly
5. **Run**: `make determinism-check`

---

---

## Test Patterns

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
    assert!(admin_operation(&state, &admin).await.is_ok());

    // Test viewer cannot
    assert!(admin_operation(&state, &viewer).await.is_err());

    Ok(())
}
```

### Multi-tenant Isolation

```rust
#[tokio::test]
async fn test_isolation() -> anyhow::Result<()> {
    let state = setup_state(None).await?;

    create_test_tenant(&state, "tenant-a", "Tenant A").await?;
    create_test_tenant(&state, "tenant-b", "Tenant B").await?;
    create_test_adapter_default(&state, "adapter-a", "tenant-a").await?;
    create_test_adapter_default(&state, "adapter-b", "tenant-b").await?;

    // Test tenant-a cannot access tenant-b's adapter
    let result = get_adapter(&state, "adapter-b", "tenant-a").await;
    assert!(result.is_err());

    Ok(())
}
```

### Error Handling

```rust
#[tokio::test]
async fn test_error_handling() -> anyhow::Result<()> {
    let state = setup_state(None).await?;

    // Test error case
    let result = invalid_operation(&state).await;

    match result {
        Err(AosError::ValidationError(msg)) => {
            assert!(msg.contains("expected message"));
        }
        _ => panic!("Expected ValidationError"),
    }

    Ok(())
}
```

---

## Troubleshooting

### Compilation Errors

```bash
# Check for errors
cargo check -p adapteros-server-api 2>&1 | grep "^error" | head -10

# Verify migrations
cargo sqlx prepare --check

# Clean build
cargo clean && cargo build
```

### Test Failures

```bash
# Run with output to see what failed
cargo test test_name -- --nocapture

# Run sequentially to avoid race conditions
cargo test -- --test-threads=1

# Skip ignored tests (if they're flaky)
cargo test -- --skip ignored
```

### Port Conflicts

```bash
# Clean up ports
make prepare
lsof -ti:8080 | xargs kill
lsof -ti:3200 | xargs kill
```

### Database Issues

```bash
# Reset test database
rm -f var/test-*.sqlite3

# Verify migrations
cargo sqlx migrate run
cargo sqlx prepare
```

### Environment Setup

```bash
# Load environment variables (with direnv)
direnv allow

# Or manually
set -a
source .env
source .env.local
set +a

# Verify model files exist
ls -lh models/
```

---

## File Locations

| Item | Path |
|------|------|
| E2E Tests | `tests/e2e_*.rs` |
| Integration Tests | `crates/*/tests/*.rs` |
| Test Fixtures | `crates/adapteros-server-api/tests/common/mod.rs` |
| Test Data | `test_data/` |
| Benchmark Tests | `tests/benchmark/` |

---

## Best Practices

1. **Use fixtures**: Leverage the common test fixtures for consistent setup
2. **Clean up**: Always delete test data after tests complete
3. **Isolation**: Use unique IDs to prevent test interference
4. **Determinism**: Avoid time-based or random values in tests
5. **Error messages**: Use descriptive assertion messages
6. **Documentation**: Comment complex test scenarios
7. **Performance**: Use `--test-threads=8` for parallel execution
8. **CI/CD**: Ensure tests pass before merging

---

## Additional Resources

- **AGENTS.md**: Development commands and workflows
- **DETERMINISM.md**: Determinism and replay guarantees
- **POLICIES.md**: Policy enforcement and hooks
- **DATABASE.md**: Database structure and migrations
- **SECURITY_TESTING.md**: Security-specific test guidance

---

MLNavigator Inc 2025-12-11
