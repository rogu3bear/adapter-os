# Running API Consistency Tests

Complete guide to running and understanding the API consistency test suite.

## Quick Start

### Run all consistency tests:
```bash
cargo test -p adapteros-server-api --test api_consistency_tests
cargo test -p adapteros-server-api --test security_validation_tests
cargo test -p adapteros-server-api --test type_validation_tests
cargo test -p adapteros-server-api --test database_validation_tests
```

### Run with output:
```bash
cargo test -p adapteros-server-api --test api_consistency_tests -- --nocapture --test-threads=1
```

### Run single test:
```bash
cargo test -p adapteros-server-api --test api_consistency_tests test_core_routes_defined -- --nocapture
```

---

## Test File Organization

### 1. `api_consistency_tests.rs` (154 lines, 40 tests)

**What it validates:**
- Routes in routes.rs have corresponding handlers
- All endpoints are documented in OpenAPI
- Permission matrix is complete
- Error responses follow consistent format
- Handler naming conventions are followed
- Parameter validation for path/query params

**Run it:**
```bash
cargo test -p adapteros-server-api --test api_consistency_tests -- --nocapture
```

**Expected output:**
```
test api_consistency::test_core_routes_defined ... ok
test api_consistency::test_handler_naming_conventions ... ok
test api_consistency::test_infer_permission_requirements ... ok
test api_consistency::test_admin_endpoint_protection ... ok
test api_consistency::test_route_handler_mapping ... ok
test api_consistency::test_type_serialization_consistency ... ok
test api_consistency::test_permission_matrix_coverage ... ok
test api_consistency::test_error_response_consistency ... ok
test api_consistency::test_http_status_code_consistency ... ok
test api_consistency::test_endpoint_parameter_validation ... ok
test api_consistency::test_query_parameter_documentation ... ok

test security_validation::test_no_hardcoded_api_keys ... ok
test security_validation::test_token_revocation_enforced ... ok
test security_validation::test_rbac_enforcement_on_protected_endpoints ... ok
test security_validation::test_permission_check_ordering ... ok
test security_validation::test_sensitive_endpoints_require_jwt ... ok
test security_validation::test_rate_limiting_active ... ok

... (more tests)

test result: ok. 40 passed; 0 failed; 0 ignored; 0 measured; 114 filtered out
```

**Key checks:**
- All tests show `ok` status
- `test result: ok` at the end
- No failures or ignored tests

---

### 2. `security_validation_tests.rs` (412 lines, 40 tests)

**What it validates:**
- No hardcoded API keys in source
- Token revocation system works
- RBAC enforcement on protected endpoints
- Permission::InferenceExecute required for /v1/infer
- Rate limiting active
- Input validation implemented
- CORS headers present
- Security headers present
- SQL injection prevention via prepared statements

**Run it:**
```bash
cargo test -p adapteros-server-api --test security_validation_tests -- --nocapture
```

**Detailed checks:**
```bash
# 1. Check no hardcoded secrets
grep -r "api_key.*=" crates/adapteros-server-api/src/
# Should return: 0 matches

# 2. Check token revocation file exists
ls crates/adapteros-server-api/src/security/token_revocation.rs
# Should return: file exists

# 3. Check require_permission calls
grep -r "require_permission" crates/adapteros-server-api/src/handlers/ | head -10
# Should show multiple require_permission calls

# 4. Check InferenceExecute enforcement
grep -r "Permission::InferenceExecute" crates/adapteros-server-api/src/
# Should show in handlers::infer, streaming_infer, batch_infer

# 5. Check rate limiting middleware
grep -r "rate_limiting_middleware" crates/adapteros-server-api/src/
# Should show in routes.rs

# 6. Check security headers middleware
grep -r "security_headers_middleware" crates/adapteros-server-api/src/
# Should show in routes.rs

# 7. Check prepared statements
grep -r "sqlx::query" crates/adapteros-server-api/src/
# Should show parameterized queries, not string concatenation
```

---

### 3. `type_validation_tests.rs` (380 lines, 35 tests)

**What it validates:**
- Request/response JSON serialization/deserialization
- Optional fields (null, missing, present) handled correctly
- Tier type converts correctly (string "tier_1" ↔ i32 1)
- Enums serialize as lowercase_snake_case
- Timestamps use ISO 8601 format
- TypeScript types match Rust structs

**Run it:**
```bash
cargo test -p adapteros-server-api --test type_validation_tests -- --nocapture
```

**Manual verification of type conversions:**
```bash
# 1. Check Tier enum serialization
grep -A 10 "enum Tier\|#\[derive.*Serialize.*Tier" crates/adapteros-server-api/src/types.rs

# 2. Check optional field serde annotations
grep -B 2 "Option<" crates/adapteros-server-api/src/types.rs | grep -E "skip_serializing|default"

# 3. Check timestamp types
grep "DateTime<Utc>" crates/adapteros-server-api/src/types.rs | head -10

# 4. Check TypeScript types match
diff <(grep "interface.*{" ui/src/api/types.ts) <(grep "pub struct.*{" crates/adapteros-server-api/src/types.rs)
```

**Example type test output:**
```
Test: InferRequest serialization
  Input: {"prompt": "test", "max_tokens": 100}
  Expected struct fields:
    - prompt: String (required)
    - max_tokens: Option<i32> (optional, default 256)
    - temperature: Option<f32> (optional, default 0.7)
```

---

### 4. `database_validation_tests.rs` (525 lines, 39 tests)

**What it validates:**
- adapter_activations table exists with correct schema
- All required tables exist with correct columns
- All migrations applied cleanly
- Foreign key constraints enforced
- Indexes exist on performance-critical columns
- Data consistency (referential integrity)
- Database passes PRAGMA integrity_check
- Column types match struct definitions

**Run it:**
```bash
cargo test -p adapteros-server-api --test database_validation_tests -- --nocapture
```

**Manual database verification:**
```bash
# 1. Check adapter_activations table
sqlite3 var/aos-cp.sqlite3 ".schema adapter_activations"
# Expected:
#   CREATE TABLE adapter_activations(
#     id TEXT PRIMARY KEY,
#     adapter_id TEXT NOT NULL FOREIGN KEY,
#     request_id TEXT NOT NULL,
#     gate_value REAL NOT NULL,
#     selected BOOLEAN NOT NULL,
#     created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
#   );

# 2. Check all tables exist
sqlite3 var/aos-cp.sqlite3 ".tables"
# Should show: adapter_activations, adapters, tenants, training_jobs, etc.

# 3. Check schema version
sqlite3 var/aos-cp.sqlite3 "SELECT version FROM schema_version;"
# Should return: 80 (or latest migration number)

# 4. Check migrations are signed
jq '.migrations | length' migrations/signatures.json
ls migrations/*.sql | wc -l
# Should be same number

# 5. Check foreign key enforcement
sqlite3 var/aos-cp.sqlite3 "PRAGMA foreign_keys;"
# Should return: 1 (enabled)

# 6. Check database integrity
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"
# Should return: ok

# 7. Check indexes exist
sqlite3 var/aos-cp.sqlite3 ".indexes adapter_activations"
# Should show index(es)

# 8. Check no orphaned records
sqlite3 var/aos-cp.sqlite3 \
  "SELECT COUNT(*) FROM adapter_activations a
   WHERE NOT EXISTS (SELECT 1 FROM adapters WHERE id = a.adapter_id);"
# Should return: 0
```

---

## Test Execution Checklist

### Before Running Tests

- [ ] Database initialized: `cargo run -p adapteros-db -- db migrate`
- [ ] No uncommitted changes: `git status`
- [ ] Latest code pulled: `git pull`

### Running Tests

```bash
# 1. API Consistency
echo "Running API Consistency Tests..."
cargo test -p adapteros-server-api --test api_consistency_tests -- --nocapture

# 2. Security Validation
echo "Running Security Validation Tests..."
cargo test -p adapteros-server-api --test security_validation_tests -- --nocapture

# 3. Type Validation
echo "Running Type Validation Tests..."
cargo test -p adapteros-server-api --test type_validation_tests -- --nocapture

# 4. Database Validation
echo "Running Database Validation Tests..."
cargo test -p adapteros-server-api --test database_validation_tests -- --nocapture

# Summary
echo "All tests completed!"
```

### After Running Tests

- [ ] All tests passed: `test result: ok`
- [ ] No failures reported
- [ ] No ignored tests
- [ ] Review test output for security warnings

---

## Interpreting Test Results

### Success (All tests pass):
```
test result: ok. 40 passed; 0 failed; 0 ignored
```

**Action:** No issues. API is consistent.

### Failures (Tests fail):
```
test result: FAILED. 35 passed; 5 failed; 0 ignored

failures:

---- api_consistency::test_core_routes_defined stdout ----
thread 'api_consistency::test_core_routes_defined' panicked at 'assertion failed'
```

**Action:** Review the assertion failure message. Check:
1. Is route missing from routes.rs?
2. Is handler missing from handlers/*.rs?
3. Is OpenAPI documentation missing?

### Ignored tests:
```
test result: ok. 38 passed; 0 failed; 2 ignored
```

**Action:** Check why tests are ignored. Use `#[ignore]` only for temporary issues.

---

## Validating Security Fixes

### Verify hardcoded API key removal:
```bash
grep -r "api_key.*=" crates/adapteros-server-api/src/ --include="*.rs"
grep -r "secret.*=" crates/adapteros-server-api/src/ --include="*.rs"
# Should return: 0 matches
```

### Verify token revocation system:
```bash
# Check file exists
ls crates/adapteros-server-api/src/security/token_revocation.rs

# Check used in middleware
grep "is_revoked\|revoke" crates/adapteros-server-api/src/middleware.rs

# Check used in logout
grep "revoke\|token_revocation" crates/adapteros-server-api/src/handlers/auth.rs
```

### Verify RBAC enforcement:
```bash
# Check permission checks in handlers
grep -l "require_permission" crates/adapteros-server-api/src/handlers/*.rs | wc -l
# Should be non-zero

# Check InferenceExecute is enforced
grep -A 5 "async fn infer" crates/adapteros-server-api/src/handlers/adapters.rs | grep "require_permission"
grep -A 5 "async fn streaming_infer" crates/adapteros-server-api/src/handlers/streaming_infer.rs | grep "require_permission"
grep -A 5 "async fn batch_infer" crates/adapteros-server-api/src/handlers/batch.rs | grep "require_permission"
```

### Verify rate limiting:
```bash
# Check middleware installed
grep "rate_limiting_middleware" crates/adapteros-server-api/src/routes.rs

# Check applied to endpoints
grep -A 10 "build(state: AppState)" crates/adapteros-server-api/src/routes.rs | grep -A 10 "rate_limiting"
```

---

## Integration with CI/CD

### GitHub Actions Example:
```yaml
name: API Consistency Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - name: Initialize database
        run: cargo run -p adapteros-db -- db migrate
      - name: Run API consistency tests
        run: cargo test -p adapteros-server-api --test api_consistency_tests
      - name: Run security validation tests
        run: cargo test -p adapteros-server-api --test security_validation_tests
      - name: Run type validation tests
        run: cargo test -p adapteros-server-api --test type_validation_tests
      - name: Run database validation tests
        run: cargo test -p adapteros-server-api --test database_validation_tests
```

---

## Troubleshooting

### Test hangs or times out:
```bash
# Run with timeout and single thread
timeout 60 cargo test -p adapteros-server-api --test api_consistency_tests -- --test-threads=1 --nocapture
```

### Cannot find test file:
```bash
# Verify test file exists
ls crates/adapteros-server-api/tests/api_consistency_tests.rs

# Check Cargo.toml includes test
grep "\[\[test\]\]" crates/adapteros-server-api/Cargo.toml
```

### Compilation errors:
```bash
# Check for syntax errors
cargo check -p adapteros-server-api

# View detailed errors
cargo build -p adapteros-server-api 2>&1 | head -50
```

### Database connection errors:
```bash
# Verify database exists
ls var/aos-cp.sqlite3

# Reset database for testing
rm var/aos-cp.sqlite3
cargo run -p adapteros-db -- db migrate
```

---

## Performance Expectations

### Test execution time:
- API consistency: ~1-2 seconds
- Security validation: ~1-2 seconds
- Type validation: ~1-2 seconds
- Database validation: ~1-2 seconds
- **Total:** ~4-8 seconds

If tests are slower, check:
- Is database query performance acceptable?
- Are there blocking operations?
- Is the system under heavy load?

---

## Next Steps

After running tests:

1. **Review any failures** and fix issues
2. **Commit test files**: `git add tests/`
3. **Push to PR**: Tests will run in CI
4. **Monitor CI results**: Ensure all pass
5. **Merge when passing**: All tests must pass

---

## Summary of Test Coverage

| Category | Tests | Endpoints Covered | Key Validations |
|----------|-------|-------------------|-----------------|
| API Consistency | 40 | 40+ | Routes, handlers, docs |
| Security | 40 | All protected | Auth, RBAC, rate limit |
| Type System | 35 | Request/Response | Serialization, enums |
| Database | 39 | 8+ tables | Schema, migrations, constraints |
| **Total** | **154** | **100+** | **Comprehensive validation** |

---

## Contact & Support

For issues with tests:
1. Check this guide for solutions
2. Review test output carefully
3. Run individual tests to isolate issues
4. Check database schema: `sqlite3 var/aos-cp.sqlite3 ".schema"`

---

**Last Updated:** 2025-11-22
**Test Suite Version:** 1.0
