# API Consistency Test Suite - Index

Complete integration test suite for validating API consistency, security, types, and database schema.

## Quick Links

- **[TEST_SUITE_SUMMARY.md](TEST_SUITE_SUMMARY.md)** - Complete test documentation with test case list
- **[RUNNING_CONSISTENCY_TESTS.md](RUNNING_CONSISTENCY_TESTS.md)** - Step-by-step guide to running tests

## Test Files

### 1. API Consistency Tests
**File:** `api_consistency_tests.rs` (1200+ lines)

**Purpose:** Validates routes, handlers, and OpenAPI documentation are complete and consistent.

**Test count:** 40 tests across 8 modules

**Modules:**
- `api_consistency` - Core route and handler mapping
- `security_validation` - Basic security validation
- `type_validation` - Type consistency
- `database_validation` - Database schema
- `cli_integration` - CLI to API mapping
- `ui_integration` - UI to API mapping
- `endpoint_documentation` - OpenAPI docs
- `consistency_matrix` - Full system consistency

**Run:** `cargo test -p adapteros-server-api --test api_consistency_tests`

---

### 2. Security Validation Tests
**File:** `security_validation_tests.rs` (600+ lines)

**Purpose:** Comprehensive security validation including RBAC, token revocation, and input validation.

**Test count:** 40 tests across 9 modules

**Modules:**
- `hardcoded_secrets` - No secrets in source code
- `token_revocation` - Token revocation system
- `rbac_enforcement` - Role-based access control
- `inference_permission` - Permission::InferenceExecute enforcement
- `rate_limiting` - Rate limiting validation
- `input_validation` - Input validation checks
- `cors_security` - CORS configuration
- `security_headers` - Security headers
- `database_security` - SQL injection prevention

**Run:** `cargo test -p adapteros-server-api --test security_validation_tests`

---

### 3. Type Validation Tests
**File:** `type_validation_tests.rs` (500+ lines)

**Purpose:** Validates Rust and TypeScript type serialization/deserialization consistency.

**Test count:** 35 tests across 8 modules

**Modules:**
- `serialization` - JSON request/response serialization
- `optional_field_handling` - null/missing/present field handling
- `tier_type_conversion` - String ↔ i32 tier conversion
- `enum_serialization` - Enum serialization format
- `timestamp_consistency` - ISO 8601 timestamp format
- `complex_types` - Nested objects and arrays
- `validation_on_deserialization` - Input validation
- `typescript_compatibility` - TypeScript ↔ Rust matching

**Run:** `cargo test -p adapteros-server-api --test type_validation_tests`

---

### 4. Database Validation Tests
**File:** `database_validation_tests.rs` (700+ lines)

**Purpose:** Validates database schema, migrations, and constraints.

**Test count:** 39 tests across 8 modules

**Modules:**
- `schema_validation` - Table schemas (7 tests)
- `migration_validation` - Migration system (5 tests)
- `constraint_validation` - Database constraints (6 tests)
- `index_validation` - Index coverage (6 tests)
- `data_consistency` - Referential integrity (4 tests)
- `performance_validation` - Query performance (3 tests)
- `backup_validation` - Backup/restore (3 tests)
- `column_type_validation` - Column types (5 tests)

**Run:** `cargo test -p adapteros-server-api --test database_validation_tests`

---

## Test Coverage Matrix

| Area | Tests | Coverage |
|------|-------|----------|
| API Endpoints | 40+ | Routes, handlers, docs |
| Security | 40 | Auth, RBAC, permissions |
| Type System | 35 | JSON, enums, timestamps |
| Database | 39 | Schema, migrations, integrity |
| **Total** | **154** | **Comprehensive validation** |

## Validation Checklist

### 1. API Consistency (40 tests)
- [ ] All routes in routes.rs are defined
- [ ] All routes have corresponding handlers
- [ ] All handlers have OpenAPI documentation
- [ ] Permission matrix is complete
- [ ] Error responses follow consistent format
- [ ] HTTP status codes are correct
- [ ] CLI commands map to API endpoints
- [ ] UI methods call documented endpoints

### 2. Security (40 tests)
- [ ] No hardcoded API keys in source
- [ ] Token revocation system operational
- [ ] RBAC checks on protected endpoints
- [ ] Permission::InferenceExecute enforced
- [ ] Rate limiting active
- [ ] Input validation implemented
- [ ] CORS properly configured
- [ ] Security headers present
- [ ] SQL injection prevented

### 3. Type System (35 tests)
- [ ] JSON request/response serialization
- [ ] Optional fields (null, missing, present)
- [ ] Tier type conversion (string ↔ i32)
- [ ] Enum serialization (lowercase_snake_case)
- [ ] Timestamp format (ISO 8601)
- [ ] TypeScript types match Rust structs
- [ ] Nested objects serialize correctly
- [ ] Complex types handled properly

### 4. Database (39 tests)
- [ ] adapter_activations table exists
- [ ] All required tables exist
- [ ] All columns present and typed correctly
- [ ] All migrations signed and applied
- [ ] Foreign key constraints enforced
- [ ] Indexes on performance-critical columns
- [ ] No orphaned records
- [ ] Database passes integrity checks

## Running All Tests

```bash
# Run all consistency tests
cargo test -p adapteros-server-api --test api_consistency_tests && \
cargo test -p adapteros-server-api --test security_validation_tests && \
cargo test -p adapteros-server-api --test type_validation_tests && \
cargo test -p adapteros-server-api --test database_validation_tests

# Expected result: All tests pass
# test result: ok. 154 passed; 0 failed; 0 ignored
```

## Key Validations

### Endpoints Validated
- 40+ API endpoints
- 189+ total routes (from CLAUDE.md)
- 8+ handler modules

### Database Tables Validated
- adapters
- adapter_activations
- training_jobs
- training_datasets
- audit_logs
- tenants
- users

### Security Checks
- No hardcoded secrets
- Token revocation enforced
- RBAC on all protected endpoints
- InferenceExecute permission required
- Rate limiting enabled
- Input validation
- CORS headers
- Security headers

### Type System Coverage
- Request/response serialization
- Optional field handling
- Tier type conversion
- Enum serialization
- Timestamp formats
- TypeScript compatibility
- Complex type handling

## Documentation Structure

1. **Test Files** (4 files)
   - api_consistency_tests.rs
   - security_validation_tests.rs
   - type_validation_tests.rs
   - database_validation_tests.rs

2. **Documentation** (3 files)
   - TEST_SUITE_SUMMARY.md (detailed breakdown)
   - RUNNING_CONSISTENCY_TESTS.md (execution guide)
   - INDEX.md (this file)

## Getting Started

Step 1: Read this index
Step 2: Review TEST_SUITE_SUMMARY.md for details
Step 3: Follow RUNNING_CONSISTENCY_TESTS.md to execute
Step 4: Review test output for any failures

## File Locations

All test files are in: `/Users/star/Dev/aos/crates/adapteros-server-api/tests/`

```
tests/
├── api_consistency_tests.rs (1200+ lines, 40 tests)
├── security_validation_tests.rs (600+ lines, 40 tests)
├── type_validation_tests.rs (500+ lines, 35 tests)
├── database_validation_tests.rs (700+ lines, 39 tests)
├── TEST_SUITE_SUMMARY.md (complete documentation)
├── RUNNING_CONSISTENCY_TESTS.md (execution guide)
└── INDEX.md (this file)
```

## Integration Points

### Routes
File: `crates/adapteros-server-api/src/routes.rs`
Tests: Validates all routes have handlers and docs

### Handlers
Directory: `crates/adapteros-server-api/src/handlers/`
Tests: Validates handler existence and permission checks

### Types
File: `crates/adapteros-server-api/src/types.rs`
Tests: Validates type serialization/deserialization

### Database
File: `crates/adapteros-db/src/lib.rs`
Tests: Validates schema and migrations

### Permissions
File: `crates/adapteros-server-api/src/permissions.rs`
Tests: Validates RBAC enforcement

## Success Criteria

All tests pass with:
```
test result: ok. 154 passed; 0 failed; 0 ignored; 0 measured
```

## Troubleshooting

See RUNNING_CONSISTENCY_TESTS.md section "Troubleshooting" for:
- Test timeout issues
- File not found errors
- Compilation errors
- Database connection errors

## Next Steps

After running tests:
1. Fix any failures in routes.rs, handlers, types.rs, or database
2. Commit test files: `git add tests/`
3. Push to PR for CI validation
4. Ensure all pass before merging

## Statistics

- **Test Files:** 4
- **Test Cases:** 154
- **Test Code Lines:** 3000+
- **Documentation Pages:** 3
- **API Endpoints Covered:** 40+
- **Database Tables Validated:** 8
- **Security Checks:** 40+
- **Type Validations:** 35+

## References

- API Documentation: See routes.rs OpenAPI paths
- Database Schema: See migrations/ directory
- Handler Implementation: See handlers/ directory
- Type Definitions: See types.rs
- Security: See security/ and permissions.rs
- CLAUDE.md: Project standards and conventions

---

**Version:** 1.0
**Created:** 2025-11-22
**Status:** Ready for execution
