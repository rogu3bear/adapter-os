# Integration Test Framework - Quick Reference

**Last Updated:** 2025-11-23 | **Status:** Ready for All Teams

---

## 30-Second Setup

```bash
# Clone/pull repo
git clone ... && cd adapter-os

# Run all integration tests
cargo test --test integration_tests --all

# Run specific team tests
cargo test --test team_1_backend_infrastructure
cargo test --test team_2_api_integration
cargo test --test team_3_inference_pipeline
cargo test --test team_4_training_ml
cargo test --test team_5_security_compliance
```

---

## File Locations

| What | Where |
|------|-------|
| **Test Harness** | `tests/common/test_harness.rs` |
| **Test Fixtures** | `tests/common/fixtures.rs` |
| **API Inventory** | `docs/testing/API_ENDPOINTS_INVENTORY.json` |
| **Test Guide** | `docs/testing/INTEGRATION_TEST_GUIDE.md` |
| **Setup Report** | `docs/testing/INTEGRATION_TEST_FRAMEWORK_SETUP.md` |
| **Team 1 Tests** | `tests/integration/team_1_backend_infrastructure.rs` |
| **Team 2 Tests** | `tests/integration/team_2_api_integration.rs` |
| **Team 3 Tests** | `tests/integration/team_3_inference_pipeline.rs` |
| **Team 4 Tests** | `tests/integration/team_4_training_ml.rs` |
| **Team 5 Tests** | `tests/integration/team_5_security_compliance.rs` |
| **CI/CD** | `.github/workflows/integration-tests.yml` |

---

## Basic Test Template

```rust
#[tokio::test]
async fn test_my_feature() {
    // Initialize test harness (in-memory SQLite, auto migrations)
    let harness = ApiTestHarness::new()
        .await
        .expect("Failed to initialize harness");

    // Authenticate (if needed)
    let mut harness = harness;
    let token = harness.authenticate().await?;

    // Create test data
    harness.create_test_adapter("my-adapter", "default").await?;
    harness.create_test_dataset("my-dataset", "Test").await?;

    // Query database directly
    let result = sqlx::query("SELECT id FROM adapters WHERE id = ?")
        .bind("my-adapter")
        .fetch_one(harness.db().pool())
        .await?;

    // Assert
    assert!(result.is_some());
}
```

---

## Using Fixtures

```rust
use tests::common::fixtures;

// Adapters
fixtures::adapters::basic_adapter_payload()
fixtures::adapters::k_sparse_routing_adapter()
fixtures::adapters::hot_swap_adapter()
fixtures::adapters::pinned_adapter()

// Datasets
fixtures::datasets::qa_dataset()
fixtures::datasets::masked_lm_dataset()
fixtures::datasets::large_chunked_dataset()

// Training
fixtures::training::basic_training_request("dataset-1")
fixtures::training::completed_training_job("job-1")
fixtures::training::failed_training_job("job-2", "Error message")

// Inference
fixtures::inference::basic_inference_request("prompt")
fixtures::inference::streaming_inference_request("prompt")
fixtures::inference::batch_inference_requests()

// Policies
fixtures::policies::egress_policy()
fixtures::policies::determinism_policy()
fixtures::policies::evidence_policy()
fixtures::policies::naming_policy()

// Auth
fixtures::auth::login_request("email@example.com", "password")
fixtures::auth::user_info_response()
```

---

## Running Tests

### All Tests
```bash
cargo test --test integration_tests --all
```

### Specific Team
```bash
cargo test --test team_2_api_integration
```

### Specific Test
```bash
cargo test test_login_endpoint_success -- --nocapture
```

### With Output
```bash
cargo test --test integration_tests -- --nocapture
```

### Single Thread (slow but useful for debugging)
```bash
cargo test --test integration_tests -- --test-threads=1 --nocapture
```

### With Coverage
```bash
cargo tarpaulin --test --out Html --output-dir coverage
```

---

## API Endpoints (189 Total)

Query the JSON inventory:

```bash
# View endpoint count
jq '.total_endpoints' docs/testing/API_ENDPOINTS_INVENTORY.json

# View all adapters endpoints
jq '.endpoints.adapters | length' docs/testing/API_ENDPOINTS_INVENTORY.json

# View all training endpoints
jq '.endpoints.training' docs/testing/API_ENDPOINTS_INVENTORY.json

# Get endpoints requiring auth
jq '.endpoints | to_entries[] | .value[] | select(.auth_required == true) | "\(.method) \(.path)"' \
  docs/testing/API_ENDPOINTS_INVENTORY.json
```

---

## Database Access

```rust
let harness = ApiTestHarness::new().await?;

// Direct query
let rows = sqlx::query("SELECT id FROM adapters")
    .fetch_all(harness.db().pool())
    .await?;

// With parameters
let result = sqlx::query("SELECT id FROM adapters WHERE tenant_id = ?")
    .bind("default")
    .fetch_one(harness.db().pool())
    .await?;

// Insert
sqlx::query("INSERT INTO adapters (id, tenant_id, hash, tier, rank) VALUES (?, ?, ?, ?, ?)")
    .bind("adapter-1")
    .bind("default")
    .bind("hash...")
    .bind("persistent")
    .bind(8)
    .execute(harness.db().pool())
    .await?;
```

---

## Test Data Factory Methods

```rust
let harness = ApiTestHarness::new().await?;

// Create adapter
harness.create_test_adapter("id", "tenant").await?;

// Create dataset
harness.create_test_dataset("id", "name").await?;

// Create training job
harness.create_test_training_job("job-id", "dataset-id", "adapter-id").await?;

// Get database connection
let db = harness.db();

// Get app state
let state = harness.state_ref();
```

---

## Troubleshooting

| Problem | Solution |
|---------|----------|
| Database locked | `rm var/aos-cp.sqlite3 && cargo test` |
| Migration fails | `cargo test -p adapteros-db schema_consistency_tests` |
| Memory exhaustion | `cargo test --test team_1 -- --test-threads=2` |
| Auth test fails | `cargo test -p adapteros-server-api auth_enhanced` |
| Timeout | Increase in CI workflow yaml or use `--release` |

---

## CI/CD

Tests run automatically on:
- Push to main/develop
- Every PR
- Manual trigger via `workflow_dispatch`

Check: `.github/workflows/integration-tests.yml`

---

## Documentation

- **Full Guide:** `docs/testing/INTEGRATION_TEST_GUIDE.md`
- **Setup Report:** `docs/testing/INTEGRATION_TEST_FRAMEWORK_SETUP.md`
- **API Inventory:** `docs/testing/API_ENDPOINTS_INVENTORY.json`
- **Architecture:** `docs/ARCHITECTURE_PATTERNS.md`
- **Standards:** `CLAUDE.md`

---

## Team Quick Links

**Team 1:** `tests/integration/team_1_backend_infrastructure.rs` (10 tests)
- System init, lifecycle, memory, multi-tenant

**Team 2:** `tests/integration/team_2_api_integration.rs` (15+ tests)
- Auth, CRUD, error handling, concurrent operations

**Team 3:** `tests/integration/team_3_inference_pipeline.rs` (12 tests)
- K-sparse routing, streaming, batch, backend selection

**Team 4:** `tests/integration/team_4_training_ml.rs` (15+ tests)
- Dataset ops, training jobs, LoRA config, metrics

**Team 5:** `tests/integration/team_5_security_compliance.rs` (20+ tests)
- Policies, RBAC, audit logs, encryption, signatures

---

## Key Statistics

- **189 API Endpoints** documented
- **266 lines** of test harness code
- **467 lines** of test fixtures
- **605 lines** of documentation
- **72+ template tests** ready to extend
- **5 CI/CD jobs** configured

---

## Next Steps

1. Run tests: `cargo test --test team_1_backend_infrastructure`
2. Read guide: `docs/testing/INTEGRATION_TEST_GUIDE.md`
3. Use fixtures: See "Using Fixtures" section above
4. Extend templates: Add tests to your team's module
5. Query endpoints: Use jq on `API_ENDPOINTS_INVENTORY.json`

---

**Framework Status:** ✅ PRODUCTION READY
**Ready for Teams:** ✅ All (1-5)
**Maintenance:** Review `/docs/testing/INTEGRATION_TEST_GUIDE.md` §Maintenance
