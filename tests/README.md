# adapterOS Test Suite

Comprehensive test coverage for deterministic inference, hot-swap, concurrency, and policy enforcement.

**Last Updated**: 2025-01-18
**Coverage**: Unit tests, integration tests, concurrency tests, schema validation
**Status**: 75%+ code coverage, all critical paths tested

---

## Table of Contents

1. [Test Categories](#test-categories)
2. [Running Tests](#running-tests)
3. [Test Organization](#test-organization)
4. [Feature-Gated Tests](#feature-gated-tests)
5. [Concurrency Testing](#concurrency-testing)
6. [Troubleshooting](#troubleshooting)

---

## Test Categories

### 1. Unit Tests

**Location**: `crates/*/src/**/*.rs` (inline with code)

**Purpose**: Test individual functions and modules in isolation.

**Examples**:
- Router k-sparse selection logic
- BLAKE3 hash computations
- Q15 quantization round-trip
- Policy rule validation

**Run**:
```bash
cargo test -p adapteros-lora-router
cargo test -p adapteros-core
cargo test -p adapteros-policy
```

---

### 2. Integration Tests

**Location**: `tests/*.rs`

**Purpose**: Test cross-crate workflows and end-to-end scenarios.

**Test Files**:

| File | Purpose | Requires |
|------|---------|----------|
| `adapter_hotswap.rs` | Adapter hot-swap protocol | `extended-tests` feature |
| `concurrency.rs` | Race condition validation | Default |
| `determinism_tests.rs` | Deterministic execution | Default |
| `gpu_verification_integration.rs` | Metal GPU fingerprinting | Metal backend |
| `load_hotswap.rs` | Load + hot-swap integration | Default |
| `worker_mocked_components.rs` | Worker with mock backends | `extended-tests` feature |
| `stability_reinforcement_tests.rs` | Lifecycle state transitions | Default |
| `server_lifecycle_tests.rs` | Server startup/shutdown/config reload | `extended-tests` feature |

**Run**:
```bash
cargo test --test <test_file_name>

# Example
cargo test --test adapter_hotswap --features extended-tests
cargo test --test determinism_tests
cargo test --test server_lifecycle_tests --features extended-tests
```

---

### 3. Schema Validation Tests

**Location**: `crates/adapteros-db/tests/schema_consistency_tests.rs`

**Purpose**: Ensure database schema matches code structs and SQL queries.

**Validates**:
- Adapter struct fields match `adapters` table columns
- INSERT statements include all required columns
- SELECT queries reference valid columns
- Migration signatures are valid (Ed25519)

**Run**:
```bash
cargo test -p adapteros-db schema_consistency_tests
```

---

### 4. Hot-Swap Tests

**Location**: `tests/adapter_hotswap.rs`

**Purpose**: Validate zero-downtime adapter replacement.

**Coverage**:
- Basic preload + swap cycle
- 100-iteration stress test (A→B→A)
- Rollback on failure
- Stack hash determinism
- Memory leak detection (refcount assertions)

**Run**:
```bash
cargo test --test adapter_hotswap --features extended-tests
```

**Key Tests**:
- `test_preload_and_swap_basic` - Basic swap functionality
- `test_adapter_swap_cycle_100_times` - Stress testing
- `test_stack_hash_determinism` - Hash consistency

---

### 5. Concurrency Tests

**Location**: `tests/concurrency.rs`

**Purpose**: Detect race conditions and data races.

**Coverage**:
- Concurrent hot-swaps
- Parallel inference requests
- Simultaneous state updates
- Refcount synchronization

**Run**:
```bash
cargo test --test concurrency
```

**Tools**:
- **Loom**: Concurrency model checking (5000+ interleavings)
- **Miri**: Undefined behavior detection
- **ThreadSanitizer**: Runtime race detection (future)

---

### 6. Server Lifecycle Tests

**Location**: `tests/server_lifecycle_tests.rs`

**Purpose**: Verify server startup, shutdown, and lifecycle management.

**Prerequisites**:
- Server binary must be built: `cargo build -p adapteros-server`
- Run with extended-tests feature: `cargo test --features extended-tests server_lifecycle`

**Coverage**:
- Server startup with valid configuration
- Health endpoint verification
- Error handling for invalid database paths
- Port conflict detection (PID lock mechanism)
- Graceful shutdown with SIGTERM
- Config reload with SIGHUP (no restart required)
- Health check degradation detection

**Run**:
```bash
# Build server first
cargo build -p adapteros-server

# Run all lifecycle tests
cargo test --test server_lifecycle_tests --features extended-tests

# Run specific test
cargo test --features extended-tests test_server_startup_success -- --nocapture
```

**Key Tests**:
- `test_server_startup_success` - Verify clean startup and shutdown
- `test_server_startup_missing_database` - Verify error handling
- `test_server_port_conflict` - Verify PID lock prevents dual instances
- `test_graceful_shutdown_sigterm` - Verify SIGTERM handling (Unix only)
- `test_config_reload_sighup` - Verify SIGHUP config reload (Unix only)
- `test_health_check_degradation` - Verify health monitoring

**Known Issues**:
As of 2025-11-19, the server has compilation errors that must be fixed before these tests can run:
- `adapteros-lora-worker`: 70+ errors (missing implementations)
- `adapteros-core`: Missing `adapteros_types` dependency

---

### 7. Determinism Tests

**Location**: `tests/determinism_tests.rs`

**Purpose**: Verify reproducible execution across runs.

**Coverage**:
- HKDF seed hierarchy
- Router tie-breaking determinism
- Metal kernel output consistency
- Tick ledger Merkle chain

**Run**:
```bash
cargo test --test determinism_tests
```

**Validates**:
- Same input → same output (across runs)
- Same manifest hash → same seeds
- Same adapter stack → same routing decisions

---

## Running Tests

### Quick Commands

**All tests** (recommended):
```bash
cargo test --workspace --exclude adapteros-lora-mlx-ffi
```

**Specific crate**:
```bash
cargo test -p adapteros-lora-router
cargo test -p adapteros-deterministic-exec
cargo test -p adapteros-db
```

**Specific test**:
```bash
cargo test test_k_sparse_routing
cargo test test_hotswap_manager_commands -- --nocapture
```

**Single-threaded** (for debugging):
```bash
cargo test -- --test-threads=1
```

**With output**:
```bash
cargo test -- --nocapture
```

---

## Test Organization

### Directory Structure

```
tests/
├── README.md                         # This file
├── adapter_hotswap.rs                # Hot-swap protocol tests
├── concurrency.rs                    # Race condition tests
├── determinism_tests.rs              # Deterministic execution tests
├── gpu_verification_integration.rs   # Metal GPU fingerprinting tests
├── load_hotswap.rs                   # Load + hot-swap integration
├── worker_mocked_components.rs       # Worker with mock backends
├── stability_reinforcement_tests.rs  # Lifecycle state transitions
├── server_lifecycle_tests.rs         # Server startup/shutdown/config reload tests
└── common/                           # Shared test utilities
    ├── mod.rs
    ├── fixtures.rs                   # Test fixtures and data
    └── helpers.rs                    # Helper functions
```

### Common Test Utilities

**Location**: `tests/common/`

**Purpose**: Shared fixtures, mocks, and helper functions.

**Usage**:
```rust
// In test file
mod common;
use common::fixtures::create_test_adapter;
use common::helpers::setup_test_db;

#[test]
fn my_test() {
    let adapter = create_test_adapter("test-adapter", 16);
    let db = setup_test_db();
    // ...
}
```

---

## Feature-Gated Tests

### Extended Tests

**Feature Flag**: `extended-tests`

**Purpose**: Long-running or resource-intensive tests.

**Enabled**:
```bash
cargo test --features extended-tests
```

**Gated Tests**:
- `tests/adapter_hotswap.rs` - Hot-swap stress tests
- `tests/worker_mocked_components.rs` - Worker integration tests

**Why gated**: These tests are slower and may require specific hardware (Metal GPU).

---

### Metal Backend Tests

**Feature Flag**: `metal-backend`

**Purpose**: Tests requiring Metal GPU acceleration.

**Enabled**:
```bash
cargo test --features metal-backend
```

**Gated Tests**:
- `tests/gpu_verification_integration.rs` - Metal buffer fingerprinting
- Metal kernel determinism tests

**Platform**: macOS 13.0+ with Apple Silicon only

---

### Mock Backend Tests

**Feature Flag**: `mock-backend`

**Purpose**: CPU-only tests without GPU requirements.

**Enabled**:
```bash
cargo test --features mock-backend
```

**Usage**: CI/CD environments without GPU access.

---

## Concurrency Testing

### Loom Model Checking

**Purpose**: Exhaustive concurrency testing via model checking.

**Location**: `tests/adapter_hotswap.rs::test_hotswap_loom`

**Run**:
```bash
LOOM_MAX_PREEMPTIONS=3 cargo test test_hotswap_loom
```

**What it does**:
- Explores 5000+ thread interleavings
- Detects data races and deadlocks
- Validates atomic operations

**Coverage**:
- No use-after-free (UAF)
- No double-free
- No data races on Arc<Stack> access

---

### Miri Undefined Behavior Scanner

**Purpose**: Detect undefined behavior at compile time.

**Run**:
```bash
cargo +nightly miri test -p adapteros-lora-worker
```

**Validates**:
- No use of uninitialized memory
- No buffer overflows
- No invalid pointer arithmetic
- No data races (on nightly builds)

**Status**: All tests pass (no UB detected)

---

## Troubleshooting

### Common Test Failures

#### 1. "Database locked" errors

**Cause**: Parallel tests accessing same SQLite database.

**Fix**:
```bash
# Run tests sequentially
cargo test -- --test-threads=1

# Or use in-memory database
export DATABASE_URL="sqlite::memory:"
cargo test
```

---

#### 2. "feature `extended-tests` not enabled"

**Cause**: Test requires `extended-tests` feature flag.

**Fix**:
```bash
cargo test --features extended-tests
```

---

#### 3. "Cannot find -lMetal" (Linux builds)

**Cause**: Metal backend tests running on Linux.

**Fix**:
```bash
# Skip Metal tests on Linux
cargo test --workspace --exclude adapteros-lora-kernel-mtl
```

---

#### 4. Loom concurrency test failures

**Cause**: Exceeded exploration budget or actual concurrency bug.

**Debug**:
```bash
# Increase preemption bound
LOOM_MAX_PREEMPTIONS=5 cargo test test_hotswap_loom

# Enable Loom debug output
LOOM_LOG=1 cargo test test_hotswap_loom
```

---

#### 5. Timeout failures in CI

**Cause**: Slow CI runners, tests timing out.

**Fix**:
```bash
# Increase test timeout (in test code)
#[tokio::test]
#[timeout(Duration::from_secs(60))]  // 60 second timeout
async fn my_test() { /* ... */ }
```

---

## Test Coverage

### Coverage Status

✅ **Test coverage is measured in CI via cargo-tarpaulin.**

Coverage is automatically measured on every PR via the `coverage` job in `.github/workflows/integration-tests.yml`. Reports are uploaded to Codecov.

**Coverage Thresholds** (enforced by `scripts/check_coverage.py`):
- Core backends (`adapteros-lora-kernel-*`): ≥80%
- Inference pipeline (`adapteros-lora-router`, `adapteros-lora-worker`): ≥85%
- Security/crypto (`adapteros-policy`, `adapteros-secd`): ≥95%
- API handlers (`adapteros-server-api`): ≥80%
- Default: 80%

**Known Test Gaps**:
- `adapteros-server-api` - Some tests may be disabled due to compilation errors
- `adapteros-system-metrics` - SQLite validation issues
- Retirement queue - Tests not yet implemented (see `/tests/retirement_queue.rs`)

### Generating Coverage Reports Locally

**Using cargo-tarpaulin**:
```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report (same as CI)
cargo tarpaulin --workspace --exclude-files 'tests/*' --out Html

# View report
open tarpaulin-report.html
```

**Using cargo-llvm-cov**:
```bash
# Install llvm-cov
cargo install cargo-llvm-cov

# Generate coverage report
cargo llvm-cov --workspace --html

# View report
open target/llvm-cov/html/index.html
```

---

## Test Best Practices

### 1. Deterministic Test Data

**Always use seeded RNG** for test data:
```rust
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

#[test]
fn test_with_deterministic_data() {
    let mut rng = ChaCha20Rng::seed_from_u64(42);
    let adapter_id = generate_adapter_id(&mut rng);
    // ...
}
```

---

### 2. Cleanup Resources

**Use RAII guards** for cleanup:
```rust
use tempfile::TempDir;

#[test]
fn test_with_temp_dir() {
    let temp_dir = TempDir::with_prefix("aos-test-").unwrap();  // Auto-deleted on drop
    let db_path = temp_dir.path().join("test.db");
    // ...
}
```

---

### 3. Test Isolation

**Each test should be independent**:
```rust
#[test]
fn test_adapter_load() {
    let table = AdapterTable::new();  // Fresh state
    // ...
}
```

**Don't rely on shared state** or test execution order.

---

### 4. Clear Test Names

**Use descriptive names**:
```rust
// GOOD
#[test]
fn test_adapter_swap_rollback_on_hash_mismatch() { /* ... */ }

// BAD
#[test]
fn test1() { /* ... */ }
```

---

## Related Documentation

- **[docs/CONFIGURATION.md](../docs/CONFIGURATION.md)** - Build and configuration
- **[docs/HOT_SWAP.md](../docs/HOT_SWAP.md)** - Hot-swap testing details
- **[AGENTS.md](../AGENTS.md)** - Testing conventions and standards

---

**Maintained by**: James KC Auchterlonie
**Copyright**: © 2025 JKCA / James KC Auchterlonie. All rights reserved.
