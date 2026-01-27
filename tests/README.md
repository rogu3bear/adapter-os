# adapterOS Test Suite

Comprehensive test coverage for deterministic inference, hot-swap, concurrency, and policy enforcement.

**Last Updated**: 2026-01-27
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

### Feature Flag Matrix

AdapterOS uses feature flags to gate tests with special requirements. This prevents CI failures and allows opt-in testing for hardware-dependent or long-running tests.

| Feature Flag | Purpose | Test Count | Enabled By | Prerequisites |
|--------------|---------|------------|------------|---------------|
| `extended-tests` | Long-running/intensive tests | 150+ | Default in full CI | None |
| `hardware-residency` | Metal GPU residency tests | ~10 | Manual | macOS + Metal GPU |
| `loom` | Concurrency model checking | ~5 | Manual | None (CPU-only) |
| `prod-gate` | Pre-deployment validation | ~10 | Manual/deploy CI | None |

---

### Extended Tests

**Feature Flag**: `extended-tests`

**Purpose**: Long-running, resource-intensive, or comprehensive integration tests.

**Enabled by default in CI**: Yes (most workflows)

**Run locally**:
```bash
cargo test --features extended-tests

# Specific test file
cargo test --test server_lifecycle_tests --features extended-tests
```

**Gated Test Files** (157+ files total):
- `tests/adapter_stress_tests.rs` - Stress testing
- `tests/orchestrator_integration.rs` - Full orchestration
- `tests/server_lifecycle_tests.rs` - Server startup/shutdown
- `tests/single_file_training_cli.rs` - CLI training workflows
- `tests/mlx_import_integration.rs` - MLX backend integration
- `tests/e2e/*` - End-to-end workflow tests
- `tests/determinism/*` - Determinism validation suite
- `tests/security/*` - Security and isolation tests
- And 140+ more test files

**Why gated**: These tests take longer to run and may require specific configurations, but don't need special hardware.

---

### Hardware Residency Tests

**Feature Flag**: `hardware-residency`

**Purpose**: Tests requiring actual Metal GPU hardware and memory residency tracking.

**Enabled by default in CI**: No (hardware-specific)

**Run locally**:
```bash
# Full residency test suite
cargo test --features hardware-residency

# Specific residency tests
cargo test --test kv_residency_quota_integration --features hardware-residency
cargo test -p adapteros-lora-worker --features hardware-residency --test residency_probe
```

**Gated Tests**:
- `tests/kv_residency_quota_integration.rs` - KV store with Metal buffer tracking
- `crates/adapteros-lora-worker/tests/residency_probe.rs` - GPU memory residency
- `crates/adapteros-memory/tests/metal_heap_tests.rs` - Metal heap operations

**Prerequisites**:
- macOS with Metal support
- Apple Silicon or AMD GPU
- Metal device available at runtime

**Why gated**: Requires real GPU hardware; cannot run in CI or on non-GPU systems.

---

### Loom Concurrency Tests

**Feature Flag**: `loom`

**Purpose**: Exhaustive concurrency model checking via the Loom framework.

**Enabled by default in CI**: No (too slow for regular CI)

**Run locally**:
```bash
# Set preemption bound to control exploration depth
LOOM_MAX_PREEMPTIONS=3 cargo test --features loom

# With debug output
LOOM_LOG=1 cargo test --features loom test_hotswap_loom
```

**Gated Tests**:
- `tests/adapter_hotswap.rs::test_hotswap_loom` - Hot-swap concurrency
- Lock-free data structure tests
- Atomic operation verification

**What Loom does**:
- Explores 5000+ thread interleavings
- Detects data races and deadlocks
- Validates atomic operations
- Proves absence of use-after-free

**Why gated**: Very slow (minutes per test); used for deep concurrency validation.

---

### Production Gate Tests

**Feature Flag**: `prod-gate`

**Purpose**: Pre-deployment validation tests that must pass before production release.

**Enabled by default in CI**: Only in dedicated `prod-gate` workflow

**Run locally**:
```bash
cargo test -p adapteros-e2e --features prod-gate
```

**Gated Tests**:
- `crates/adapteros-e2e/tests/prod_gate.rs` - Comprehensive E2E validation
- Determinism verification
- Policy enforcement validation
- Telemetry integrity checks

**Why gated**: High-value smoke tests run before deployment; too expensive for every PR.

---

### Metal Backend Tests

**Feature Flag**: `metal-backend`

**Purpose**: Tests requiring Metal GPU acceleration (separate from hardware-residency).

**Enabled**:
```bash
cargo test --features metal-backend
```

**Gated Tests**:
- `tests/gpu_verification_integration.rs` - Metal buffer fingerprinting
- Metal kernel determinism tests

**Platform**: macOS 13.0+ with Apple Silicon or AMD GPU

---

### MLX Backend Tests

**Feature Flag**: `mlx` or `multi-backend`

**Purpose**: Tests requiring MLX C++ FFI backend.

**Enabled**:
```bash
cargo test --features mlx

# Or with multi-backend (default)
cargo test --features multi-backend
```

**Gated Tests**:
- `tests/mlx_import_integration.rs` - MLX model loading
- `tests/training_resume_e2e.rs::training_resume_e2e` - MLX training
- `crates/adapteros-lora-mlx-ffi/tests/*` - MLX FFI layer

**Prerequisites**: Homebrew MLX installed (`brew install mlx`)

---

## Ignored Tests Catalogue

Tests marked with `#[ignore]` require special setup and are not run by default. Run them explicitly with `cargo test -- --ignored`.

**Last Audited**: 2026-01-27

### Summary Statistics

| Metric | Count |
|--------|-------|
| **Total ignored tests** | 143 |
| Tests in `crates/` | 103 |
| Tests in `tests/` | 40 |
| Tests with tracking IDs | 63 |
| Tests missing tracking IDs | 80 |

### By Category

| Category | Count | Hardware Required | CI Safe | Tracking Coverage |
|----------|-------|-------------------|---------|-------------------|
| Model/MLX Backend | 45 | No | Yes (with setup) | Partial |
| Network Security (root/PF) | 24 | Yes (root) | No | None |
| Server/Fixture Setup | 23 | No | Yes (with setup) | Partial |
| API Updates Pending | 8 | No | Yes (when fixed) | Full |
| Metal/GPU Hardware | 6 | Yes | No | Full |
| Tokenizer | 4 | No | Yes (with files) | Full |
| GCP KMS | 4 | No (emulator) | Yes (with emulator) | None |
| Benchmarks/Long-Running | 5 | No | No (too slow) | Partial |
| Fixture Generation | 2 | No | Manual only | None |

### Tracking ID Audit

Ignored tests use tracking IDs in the format `[tracking: STAB-IGN-XXXX]` or `[tracking: TRAIN-TEST-XXXX]`.

**Current ID ranges**:
- `STAB-IGN-0021` through `STAB-IGN-0230` (with gaps)
- `TRAIN-TEST-0001` through `TRAIN-TEST-0007`

**Next available IDs**:
- `STAB-IGN-0231` for general stability tests
- `TRAIN-TEST-0008` for training-specific tests

**Gap analysis**: IDs are not sequential. Major gaps exist at:
- 0022-0025, 0029-0037, 0039, 0045, 0050-0059, 0069-0159, 0164-0169, 0172-0187, 0194-0196, 0202-0209, 0213, 0215-0217

---

### Category Details

#### 1. Model/MLX Backend Tests (45 tests)

**Location**: Multiple crates and workspace tests

**Prerequisites**:
- `TEST_MLX_MODEL_PATH` or `AOS_TEST_BASE_MODEL` environment variable set
- Valid model files (e.g., Qwen2.5-7B-Instruct)
- MLX backend compiled (`--features mlx`)
- Python mlx-lm installed (for bridge tests)

**Key Locations**:
- `tests/e2e_inference_harness.rs` - 11 MLX inference tests
- `crates/adapteros-lora-worker/src/training/trainer/tests.rs` - 11 training tests (7 with `TRAIN-TEST-*` tracking)
- `crates/adapteros-lora-mlx-ffi/tests/resilience_tests.rs` - 17 resilience tests
- `crates/adapteros-lora-mlx-ffi/tests/memory_pool_integration.rs` - 3 tests
- `crates/adapteros-lora-worker/tests/mlx_bridge_integration.rs` - 2 tests

**Run**:
```bash
export TEST_MLX_MODEL_PATH=/var/models/Qwen2.5-7B-Instruct
cargo test --features mlx -- --ignored

# Or for training tests specifically
export AOS_TEST_BASE_MODEL=/var/models/Llama-3.2-3B-Instruct-4bit
cargo test -p adapteros-lora-worker -- --ignored
```

---

#### 2. Network Security Tests (24 tests)

**Location**: `crates/adapteros-node/tests/`

**Prerequisites**:
- Root/sudo privileges
- PF (Packet Filter) rules enabled
- `aos-worker` binary built and in PATH
- Specific system configurations

**Key Locations**:
- `isolation_tests.rs` - 13 tests
- `spawn_worker_tests.rs` - 11 tests

**Run**:
```bash
# Enable PF first
sudo pfctl -e

# Run isolation tests
sudo cargo test -p adapteros-node --test isolation_tests -- --ignored

# Run worker spawn tests
sudo cargo test -p adapteros-node --test spawn_worker_tests -- --ignored

# For egress blocking tests
echo 'block out all' | sudo pfctl -f -
cargo test -p adapteros-node -- --ignored egress
```

**Note**: These tests require root privileges and cannot run in standard CI environments.

---

#### 3. Server/Fixture Setup Tests (23 tests)

**Location**: `crates/adapteros-server-api/tests/`

**Prerequisites**:
- Full server setup with authentication enabled
- Database initialized and migrations run
- Tenant-specific fixtures
- Base model fixtures for some tests

**Key Locations**:
- `tenant_isolation_adapters.rs` - 5 tests
- `model_handlers_integration.rs` - 4 tests
- `model_status_contract.rs` - 3 tests
- `rag_retrieval_test.rs` - 2 tests
- `tests/auth_integration_test.rs` - 4 tests (workspace level)
- `tests/telemetry_endpoints.rs` - 3 tests (workspace level)

**Run**:
```bash
# Start server first
./aosctl db migrate
AOS_DEV_NO_AUTH=1 cargo run -p adapteros-server -- --config configs/cp.toml &

# Run fixture-dependent tests
cargo test -p adapteros-server-api -- --ignored
```

---

#### 4. API Updates Pending (8 tests)

**Location**: Various crates

These tests are temporarily disabled pending API refactoring. They have full tracking coverage.

**Tests**:
- `STAB-IGN-0040`: `memory_management_integration.rs` - memory module functions
- `STAB-IGN-0046`: `concurrency.rs` - MockKernels not exported
- `STAB-IGN-0047`: `gpu_training_integration.rs` - select_optimal_backend private
- `STAB-IGN-0048`: `gpu_training_integration.rs` - detect_available_backends private
- `STAB-IGN-0064`: `secure_fs_integration_tests.rs` - SecureFsConfig fields
- `STAB-IGN-0068`: `telemetry/mod.rs` - prometheus Registry metric_count
- `STAB-IGN-0197-0201`: `policy_enforcement_integration.rs` - policy API updates

**Run** (when APIs are updated):
```bash
cargo test -- --ignored "STAB-IGN-0040\|STAB-IGN-0046"
```

---

#### 5. Metal/GPU Hardware Tests (6 tests)

**Location**: `tests/lora_buffer_population_integration.rs`

**Prerequisites**:
- macOS with Metal support
- Signed Metal kernel library
- Metal device available
- Hardware-residency feature enabled

**Tests** (all have `STAB-IGN-0188` through `STAB-IGN-0193` tracking):
- `test_lora_buffer_population_basic`
- `test_lora_buffer_population_multi_adapter`
- `test_lora_buffer_hash_determinism`
- `test_lora_buffer_memory_pressure`
- `test_lora_buffer_concurrent_population`
- `test_lora_buffer_error_recovery`

**Run**:
```bash
cargo test --features hardware-residency --test lora_buffer_population_integration -- --ignored
```

---

#### 6. Tokenizer Tests (4 tests)

**Location**: `crates/adapteros-ingest-docs/src/embeddings.rs`, `crates/adapteros-lora-worker/src/tokenizer.rs`

**Prerequisites**:
- Tokenizer model files installed
- `AOS_TOKENIZER_PATH` environment variable set

**Tests** (all have `STAB-IGN-0026-0028`, `STAB-IGN-0044` tracking):
- `test_tokenizer_encode` (embeddings)
- `test_tokenizer_decode` (embeddings)
- `test_tokenizer_batch` (embeddings)
- `test_tokenizer_integration` (worker)

**Run**:
```bash
export AOS_TOKENIZER_PATH=./var/models/Qwen2.5-7B-Instruct/tokenizer.json
cargo test --release -- --ignored tokenizer
```

---

#### 7. GCP KMS Tests (4 tests)

**Location**: `crates/adapteros-crypto/src/providers/kms.rs`

**Prerequisites**:
- GCP KMS emulator running on localhost:9011
- `GCP_KMS_EMULATOR_HOST` environment variable set

**Tests** (no tracking IDs - need to be added):
- `test_kms_sign`
- `test_kms_verify`
- `test_kms_encrypt`
- `test_kms_decrypt`

**Run**:
```bash
# Start emulator (in separate terminal)
docker run -p 9011:9011 gcr.io/google.com/cloudsdktool/google-cloud-cli:emulators gcloud beta emulators kms start --host-port=0.0.0.0:9011

# Run tests
export GCP_KMS_EMULATOR_HOST=localhost:9011
cargo test -p adapteros-crypto -- --ignored kms
```

---

#### 8. Benchmarks/Long-Running Tests (5 tests)

**Location**: `tests/training_pipeline.rs`, `tests/load_hot_swap.rs`, `tests/cross_platform_determinism.rs`

**Prerequisites**:
- Extended test time (1+ hours for some)
- Optional: valgrind or profiler tools
- Baseline reference outputs for determinism tests

**Tests**:
- `test_training_performance_benchmark` (`STAB-IGN-0214`) - ~1 hour
- `test_load_hot_swap_1h_soak` - 1 hour soak test
- `test_cross_platform_determinism` - requires baseline
- `generate_reference_outputs` - manual fixture generation

**Run**:
```bash
# Long benchmark (release mode recommended)
cargo test --release --test training_pipeline -- --ignored

# Soak test with profiler
valgrind --tool=memcheck cargo test --test load_hot_swap -- --ignored

# Generate baselines first
cargo test --test cross_platform_determinism -- --ignored generate_reference
# Then run comparison
cargo test --test cross_platform_determinism -- --ignored
```

---

#### 9. Fixture Generation Tests (2 tests)

**Location**: `tests/determinism_replay_harness.rs`, `tests/cross_platform_determinism.rs`

**Purpose**: Regenerate golden test data for determinism verification.

**Run** (manual only):
```bash
cargo test -- --ignored manual_fixture_regeneration
cargo test -- --ignored generate_reference_outputs
```

---

### Running Ignored Tests Locally

#### Quick Reference

```bash
# Run ALL ignored tests (requires all prerequisites)
cargo test --workspace -- --ignored

# Run ignored tests for a specific crate
cargo test -p adapteros-lora-mlx-ffi -- --ignored

# Run ignored tests matching a pattern
cargo test -- --ignored "mlx\|model"

# Run with output for debugging
cargo test -- --ignored --nocapture

# Run ignored tests in release mode (recommended for benchmarks)
cargo test --release -- --ignored
```

#### Environment Variables Cheatsheet

```bash
# MLX model path
export TEST_MLX_MODEL_PATH=/var/models/Qwen2.5-7B-Instruct
export AOS_TEST_BASE_MODEL=/var/models/Llama-3.2-3B-Instruct-4bit

# Tokenizer
export AOS_TOKENIZER_PATH=/var/models/Qwen2.5-7B-Instruct/tokenizer.json

# GCP KMS emulator
export GCP_KMS_EMULATOR_HOST=localhost:9011

# Determinism debugging
export AOS_DEBUG_DETERMINISM=1
```

---

### Tests Missing Tracking IDs

The following test files have ignored tests without tracking IDs and should be updated:

| File | Test Count | Priority |
|------|------------|----------|
| `crates/adapteros-node/tests/isolation_tests.rs` | 13 | Low (root required) |
| `crates/adapteros-node/tests/spawn_worker_tests.rs` | 11 | Low (root required) |
| `crates/adapteros-lora-mlx-ffi/tests/resilience_tests.rs` | 17 | Medium |
| `crates/adapteros-crypto/src/providers/kms.rs` | 4 | Medium |
| `crates/adapteros-server-api/tests/` (various) | 15 | Medium |
| `tests/e2e_inference_harness.rs` | 11 | Medium |
| `crates/adapteros-lora-worker/src/training/trainer/tests.rs` | 4 | Low (partial coverage) |

To add tracking IDs, use the format:
```rust
#[ignore = "Reason for ignoring [tracking: STAB-IGN-XXXX]"]
```

Next available ID: `STAB-IGN-0231`

---

## Standard Test Commands by Scenario

### Quick Development Cycle
```bash
# Run unit tests only (fast, ~30s)
cargo test --lib

# Run specific crate tests
cargo test -p adapteros-lora-router
```

### Full Local Validation
```bash
# Run all non-ignored tests (includes extended-tests)
cargo test --workspace --features extended-tests

# With verbose output
cargo test --workspace --features extended-tests -- --nocapture
```

### Hardware-Dependent Testing (macOS only)
```bash
# Metal residency tests
cargo test --features hardware-residency -- --ignored

# MLX backend tests
export TEST_MLX_MODEL_PATH=./var/models/Qwen2.5-7B-Instruct
cargo test --features mlx -- --ignored
```

### Concurrency Validation
```bash
# Standard concurrency tests (fast)
cargo test --test concurrency

# Deep concurrency checking with Loom (slow)
LOOM_MAX_PREEMPTIONS=3 cargo test --features loom
```

### Pre-Deployment Gate
```bash
# Production gate tests
cargo test -p adapteros-e2e --features prod-gate

# Full extended suite
cargo test --workspace --features extended-tests --release
```

### Network Security Validation (requires root)
```bash
# Enable PF and run isolation tests
sudo pfctl -e
cargo test -p adapteros-node -- --ignored spawn_worker

# Egress blocking tests
echo 'block out all' | sudo pfctl -f -
cargo test -p adapteros-node -- --ignored egress
```

---

## Concurrency Testing

### Loom Model Checking

**Purpose**: Exhaustive concurrency testing via model checking.

**Location**: `tests/adapter_hotswap.rs::test_hotswap_loom`

**Run**:
```bash
LOOM_MAX_PREEMPTIONS=3 cargo test --features loom
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
