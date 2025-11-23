# Integration Test Framework Guide

**Version:** 1.0
**Date:** 2025-11-23
**Maintainer:** Team 6 (API & Integration)

---

## Overview

This guide documents the comprehensive integration test framework for AdapterOS v0.3-alpha. The framework supports all teams (1-5) with:

- **189 REST API endpoints** fully documented and inventoried
- **In-memory SQLite test database** with automatic migrations
- **Reusable test fixtures** for adapters, datasets, training, and policies
- **Axum-test harness** for API endpoint testing
- **Template tests** for each team (Teams 1-5)
- **CI/CD integration** via GitHub Actions

---

## Quick Start

### 1. Run All Integration Tests

```bash
# Run all integration tests
cargo test --test integration_tests --all

# Run specific team tests
cargo test --test team_1_backend_infrastructure
cargo test --test team_2_api_integration

# Run with output
cargo test --test integration_tests -- --nocapture
```

### 2. Run With Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html --output-dir coverage --test
```

### 3. Run Locally With Output

```bash
# Run single test with full output
cargo test test_system_initialization -- --nocapture --test-threads=1
```

---

## Test Structure

### Directory Layout

```
tests/
├── common/
│   ├── mod.rs                    # Module root
│   ├── auth.rs                   # Authentication utilities
│   ├── cleanup.rs                # Test cleanup helpers
│   ├── migration_setup.rs        # Database migration setup
│   ├── test_harness.rs           # API test harness (NEW)
│   └── fixtures.rs               # Test data fixtures (NEW)
└── integration/
    ├── team_1_backend_infrastructure.rs  # Team 1 tests
    ├── team_2_api_integration.rs         # Team 2 tests
    ├── team_3_inference_pipeline.rs      # Team 3 tests (template)
    ├── team_4_training_ml.rs             # Team 4 tests (template)
    └── team_5_security_compliance.rs     # Team 5 tests (template)
```

### Database Setup

The test harness automatically:
1. Creates an in-memory SQLite database (`sqlite::memory:`)
2. Runs all migrations from `migrations/` directory
3. Seeds default tenant and admin user
4. Provides database access via `ApiTestHarness::db()`

---

## Using the Test Harness

### Basic Setup

```rust
use tests::common::test_harness::ApiTestHarness;

#[tokio::test]
async fn test_my_feature() {
    // Initialize harness with in-memory database
    let harness = ApiTestHarness::new()
        .await
        .expect("Failed to initialize harness");

    // Now you can use harness.db() for database queries
    // or harness.state_ref() for app state access
}
```

### With Authentication

```rust
#[tokio::test]
async fn test_authenticated_endpoint() {
    let mut harness = ApiTestHarness::new().await?;

    // Login and get token
    let token = harness.authenticate().await?;
    assert!(!token.is_empty());

    // Token is automatically stored in harness.auth_token
}
```

### Creating Test Data

```rust
#[tokio::test]
async fn test_with_fixtures() {
    let harness = ApiTestHarness::new().await?;

    // Create test adapter
    harness.create_test_adapter("my-adapter", "default").await?;

    // Create test dataset
    harness.create_test_dataset("my-dataset", "Test Dataset").await?;

    // Create training job
    harness.create_test_training_job("job-1", "my-dataset", "my-adapter").await?;

    // Query database directly
    let result = sqlx::query("SELECT id FROM adapters WHERE id = ?")
        .bind("my-adapter")
        .fetch_one(harness.db().pool())
        .await?;
}
```

---

## Using Test Fixtures

Test fixtures provide pre-built JSON payloads for all major entities:

### Adapter Fixtures

```rust
use tests::common::fixtures;

#[test]
fn test_adapter_creation() {
    // Use predefined fixture
    let adapter = fixtures::adapters::basic_adapter_payload();
    assert_eq!(adapter["tier"], "persistent");

    // Customize fixture
    let custom = fixtures::adapters::with_id("custom-id");
    assert_eq!(custom["id"], "custom-id");

    // Use specialized fixtures
    let router = fixtures::adapters::k_sparse_routing_adapter();
    let hotswap = fixtures::adapters::hot_swap_adapter();
    let pinned = fixtures::adapters::pinned_adapter();
}
```

### Dataset Fixtures

```rust
let qa_dataset = fixtures::datasets::qa_dataset();
let masked_lm = fixtures::datasets::masked_lm_dataset();
let chunked = fixtures::datasets::large_chunked_dataset();
let invalid = fixtures::datasets::malformed_dataset();
```

### Training Fixtures

```rust
let training_req = fixtures::training::basic_training_request("dataset-1");
let completed = fixtures::training::completed_training_job("job-1");
let failed = fixtures::training::failed_training_job("job-2", "OOM error");
```

### Inference Fixtures

```rust
let basic_req = fixtures::inference::basic_inference_request("Hello world");
let streaming = fixtures::inference::streaming_inference_request("Explain AI");
let batch = fixtures::inference::batch_inference_requests();
let response = fixtures::inference::inference_response("Generated text");
```

### Composing Fixtures

```rust
use tests::common::fixtures::utils;

// Merge fixtures with custom fields
let base = fixtures::adapters::basic_adapter_payload();
let overrides = json!({"tier": "ephemeral"});
let merged = utils::merge_fixture(base, overrides);

// Create multiple fixtures
let adapters = utils::create_multiple_fixtures(
    fixtures::adapters::with_id,
    10
);
```

---

## API Endpoints Inventory

All 189 REST API endpoints are documented in:

**File:** `/docs/testing/API_ENDPOINTS_INVENTORY.json`

### Structure

```json
{
  "version": "1.0",
  "total_endpoints": 189,
  "summary": {
    "health_auth": 8,
    "adapters": 30,
    "tenants": 8,
    "training": 8,
    "inference": 3,
    ...
  },
  "endpoints": {
    "health_auth": [
      {
        "method": "GET",
        "path": "/healthz",
        "description": "Health check",
        "auth_required": false
      },
      ...
    ],
    "adapters": [...],
    ...
  }
}
```

### Querying the Inventory

```bash
# Get total endpoint count
jq '.total_endpoints' docs/testing/API_ENDPOINTS_INVENTORY.json

# Get all adapter endpoints
jq '.endpoints.adapters' docs/testing/API_ENDPOINTS_INVENTORY.json

# Get endpoints requiring authentication
jq '.endpoints | .[] | .[] | select(.auth_required == true)' docs/testing/API_ENDPOINTS_INVENTORY.json
```

---

## Team-Specific Test Templates

### Team 1: Backend Infrastructure

**File:** `tests/integration/team_1_backend_infrastructure.rs`

**Test Coverage:**
- System initialization
- Database migrations
- Memory management
- Adapter lifecycle (Unloaded → Cold → Warm → Hot → Resident)
- Backend coordination
- Health checks
- Multi-tenant isolation

**Example Test:**
```rust
#[tokio::test]
async fn test_adapter_lifecycle_transitions() {
    let harness = ApiTestHarness::new().await?;

    // Create adapter
    harness.create_test_adapter("lifecycle-test", "default").await?;

    // Verify it exists in database
    let result = sqlx::query("SELECT id FROM adapters WHERE id = ?")
        .bind("lifecycle-test")
        .fetch_one(harness.db().pool())
        .await?;

    assert!(result.is_some());
}
```

### Team 2: API & Integration

**File:** `tests/integration/team_2_api_integration.rs`

**Test Coverage:**
- All 189 REST API endpoints
- Authentication flows (login, refresh, logout, sessions)
- Adapter CRUD (Create, Read, Update, Delete)
- Dataset operations
- Training job management
- Error handling and validation
- Concurrent operations
- Rate limiting (when implemented)

**Example Test:**
```rust
#[tokio::test]
async fn test_login_endpoint_success() {
    let mut harness = ApiTestHarness::new().await?;

    let token = harness.authenticate().await?;
    assert!(!token.is_empty());
}
```

### Team 3: Inference Pipeline

**File:** `tests/integration/team_3_inference_pipeline.rs` (Template)

**Test Coverage (TO BE IMPLEMENTED):**
- Multi-adapter routing (K-sparse selection)
- Batch inference
- Streaming inference (SSE)
- Determinism validation
- Backend selection (CoreML, MLX, Metal)
- Latency measurements
- Q15 quantization verification

### Team 4: Training & ML

**File:** `tests/integration/team_4_training_ml.rs` (Template)

**Test Coverage (TO BE IMPLEMENTED):**
- Dataset upload and validation
- Chunked dataset upload
- Training job creation and monitoring
- Job cancellation
- Training template selection
- Adapter packaging after training
- Training metrics collection

### Team 5: Security & Compliance

**File:** `tests/integration/team_5_security_compliance.rs` (Template)

**Test Coverage (TO BE IMPLEMENTED):**
- Policy enforcement (23 canonical policies)
- RBAC permission validation
- Audit logging
- Encryption/decryption
- Signature verification
- Egress blocking in production mode
- Determinism policy enforcement

---

## Writing New Tests

### 1. Choose the Right Team Module

Select the team module where your test belongs:
- **Team 1:** Infrastructure, lifecycle, memory, backends
- **Team 2:** API endpoints, auth, CRUD operations
- **Team 3:** Inference, routing, streaming
- **Team 4:** Training, datasets, ML operations
- **Team 5:** Security, policies, RBAC, audit

### 2. Use the Test Harness

```rust
#[tokio::test]
async fn test_my_feature() {
    // Initialize
    let harness = ApiTestHarness::new().await?;

    // Create test data
    harness.create_test_adapter("test-adapter", "default").await?;

    // Query database
    let result = sqlx::query("SELECT id FROM adapters WHERE id = ?")
        .bind("test-adapter")
        .fetch_one(harness.db().pool())
        .await?;

    // Assert
    assert!(result.is_some());
}
```

### 3. Leverage Fixtures

```rust
use tests::common::fixtures;

#[test]
fn test_with_fixtures() {
    let adapter = fixtures::adapters::basic_adapter_payload();
    let dataset = fixtures::datasets::qa_dataset();
    let policy = fixtures::policies::egress_policy();

    // Use fixtures in your test
    assert_eq!(adapter["tier"], "persistent");
}
```

### 4. Follow Naming Conventions

```
test_<entity>_<operation>_<scenario>

Examples:
- test_adapter_lifecycle_transitions
- test_training_job_cancellation
- test_api_error_response_format
- test_concurrent_adapter_operations
- test_policy_enforcement_egress_blocking
```

### 5. Add Documentation

```rust
/// Test that adapter transitions between lifecycle states correctly.
///
/// Verifies:
/// - Adapter starts in Unloaded state
/// - Transitions to Cold when loaded
/// - Promotes to Warm under load
/// - Can be evicted back to Unloaded
///
/// Related: docs/LIFECYCLE.md
#[tokio::test]
async fn test_adapter_lifecycle_transitions() {
    // ...
}
```

---

## CI/CD Integration

### GitHub Actions Workflow

**File:** `.github/workflows/integration-tests.yml`

Automatically runs on:
- Every push to `main` branch
- Every pull request
- Manual trigger via `workflow_dispatch`

### Local CI Simulation

```bash
# Run tests like GitHub Actions would
cargo test --workspace --test integration_tests

# With coverage
cargo tarpaulin --test --out Html

# With linting
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

### Coverage Requirements

By component (from VERIFICATION-STRATEGY.md):

| Component | Target |
|-----------|--------|
| API Handlers | 80% |
| Core Backends | 80% |
| Training | 70% |
| Inference | 85% |
| Security/Crypto | 95% |

---

## Troubleshooting

### Database Locked

If you see "database is locked" errors:

```bash
# Use a temporary test database
export TEST_DATABASE_URL=sqlite:///tmp/test-aos-$(date +%s).db

cargo test --test integration_tests
```

### Migration Issues

If migrations fail to apply:

```bash
# Check migration status
sqlx migrate list

# Verify migration syntax
cargo test -p adapteros-db schema_consistency_tests

# Reset test database (removes var/aos-cp.sqlite3)
rm var/aos-cp.sqlite3 && cargo test --test integration_tests
```

### Memory Exhaustion

If tests consume excessive memory:

```bash
# Run with reduced parallelism
cargo test --test integration_tests -- --test-threads=2

# Run single test
cargo test test_system_initialization -- --test-threads=1
```

### Token/Auth Issues

If authentication tests fail:

```bash
# Verify auth module
cargo test -p adapteros-server-api auth_enhanced
cargo test --test auth_integration_test
```

---

## Maintenance & Updates

### Adding New Endpoints

1. Extract route from `crates/adapteros-server-api/src/routes.rs`
2. Add to `docs/testing/API_ENDPOINTS_INVENTORY.json`
3. Create test template in appropriate team module
4. Update this guide's endpoint count

### Updating Fixtures

When schema changes:

1. Update fixture definitions in `tests/common/fixtures.rs`
2. Add migration version to fixtures (if breaking)
3. Update fixture tests in `tests/common/fixtures.rs`

### Deprecating Tests

When features are removed:

```rust
#[deprecated(since = "0.4.0", note = "use test_new_feature instead")]
#[tokio::test]
async fn test_deprecated_feature() { ... }
```

---

## Related Documentation

- [API_ENDPOINTS_INVENTORY.json](./API_ENDPOINTS_INVENTORY.json) - Complete endpoint listing
- [VERIFICATION-STRATEGY.md](./VERIFICATION-STRATEGY.md) - Test strategy and coverage targets
- [../ARCHITECTURE_PATTERNS.md](../ARCHITECTURE_PATTERNS.md) - System architecture
- [../DETERMINISTIC_EXECUTION.md](../DETERMINISTIC_EXECUTION.md) - Determinism requirements
- [../LIFECYCLE.md](../LIFECYCLE.md) - Adapter lifecycle state machine

---

## Performance Targets

| Test Type | Target Duration | Max Acceptable |
|-----------|-----------------|-----------------|
| Unit test | < 100ms | 500ms |
| Integration test | < 1s | 5s |
| E2E API test | < 5s | 30s |
| Stress test | < 60s | 120s |

---

## Success Criteria

- [x] Test harness ready for Teams 1-5
- [x] 189 API endpoints documented in JSON
- [x] Template tests created for each team
- [x] Comprehensive documentation
- [ ] CI/CD configured (WIP)
- [ ] All teams using templates within Phase 1

---

**Last Updated:** 2025-11-23
**Next Review:** 2025-12-07
