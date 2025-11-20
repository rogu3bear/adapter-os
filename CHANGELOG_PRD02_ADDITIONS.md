# CHANGELOG Additions for PRD-02

**Purpose:** Proposed additions to CHANGELOG.md for PRD-02 implementation
**Date:** 2025-11-19
**Status:** DRAFT - To be merged into main CHANGELOG.md after review

---

## [Unreleased]

### Added (PRD-02: Metadata Normalization & Version Guarantees)

#### Database Schema Enhancements
- **Metadata Normalization**: Added `version` and `lifecycle_state` columns to `adapters` table (migration 0068)
  - Supports both Semantic Versioning (MAJOR.MINOR.PATCH) and monotonic versioning (sequential integers)
  - Lifecycle states: draft → active → deprecated → retired
  - Migration: `/migrations/0068_metadata_normalization.sql`

- **SQL Trigger Enforcement**: Database-level validation of lifecycle state transitions (migration 0075)
  - Rule 1: Retired is a terminal state (cannot transition out)
  - Rule 2: Ephemeral tier adapters cannot be deprecated (must go directly to retired)
  - Rule 3: No backward transitions (state machine is forward-only)
  - Performance indexes added for lifecycle_state queries
  - Migration: `/migrations/0075_lifecycle_state_transition_triggers.sql`

- **Lifecycle Audit Trail**: Complete history tracking for adapter version and lifecycle changes (migration 0071)
  - Records all state transitions with timestamps
  - Tracks version changes over time
  - Supports compliance and debugging workflows
  - Migration: `/migrations/0071_lifecycle_version_history.sql`

- **Routing Decision Telemetry**: Persistent storage of router decisions for analysis (migration 0070)
  - Tracks K-sparse routing decisions
  - Enables post-hoc analysis of adapter selection patterns
  - Supports router performance optimization
  - Migration: `/migrations/0070_routing_decisions.sql`

#### API Type System Overhaul
- **API Schema Versioning**: All API responses include `schema_version: "1.0.0"` field
  - Enables version negotiation between clients and server
  - Supports graceful degradation for old clients
  - Constant defined in `crates/adapteros-db/src/metadata.rs`: `pub const API_SCHEMA_VERSION: &str = "1.0.0";`

- **Centralized API Types**: Refactored into `adapteros-api-types` crate (15 modules, 1,309 LOC)
  - `adapters.rs` (103 LOC) - Adapter request/response types
  - `auth.rs` (40 LOC) - Authentication types
  - `dashboard.rs` (71 LOC) - Dashboard response types
  - `domain_adapters.rs` (134 LOC) - Domain adapter types
  - `git.rs` (54 LOC) - Git integration types
  - `inference.rs` (63 LOC) - Inference request/response
  - `lib.rs` (141 LOC) - Module root with schema_version
  - `metrics.rs` (74 LOC) - Metrics types
  - `nodes.rs` (58 LOC) - Node status types
  - `plans.rs` (69 LOC) - Plan response types
  - `repositories.rs` (50 LOC) - Repository types
  - `telemetry.rs` (134 LOC) - Telemetry event types
  - `tenants.rs` (49 LOC) - Tenant response types
  - `training.rs` (230 LOC) - Training request/response
  - `workers.rs` (39 LOC) - Worker status types

- **Type Validation Suite**: 36 comprehensive tests for API type compatibility (1,787 LOC)
  - OpenAPI compatibility tests (12 tests, 530 LOC)
  - Frontend TypeScript compatibility tests (14 tests, 582 LOC)
  - Round-trip serialization tests (10 tests, 532 LOC)
  - Location: `tests/type_validation/`

#### Documentation
- **Version Guarantee Policy**: Added `docs/VERSION_GUARANTEES.md` (248 lines)
  - Defines semantic versioning and monotonic versioning formats
  - Documents lifecycle state transition rules
  - Specifies backward/forward compatibility guarantees
  - Defines illegal state combinations and validation rules

- **Implementation Guides**: Added PRD-02 implementation documentation (6 files, ~2,300 lines)
  - `PRD-02_INDEX.md` - Navigation guide and completion status
  - `PRD-02_FIX_ROADMAP.md` - 62% → 100% completion roadmap
  - `PRD-02_OPTION_A_IMPLEMENTATION.md` - Critical fixes summary
  - `PRD-02_EXECUTIVE_SUMMARY.txt` - One-page status overview
  - `PRD-02_KEY_FILES_MANIFEST.txt` - File inventory
  - `PRD-02_DELIVERABLES_CHECKLIST.txt` - Completion tracking

### Changed (PRD-02: Breaking Changes)

**⚠️ BREAKING CHANGES - READ MIGRATION GUIDE BEFORE UPGRADING ⚠️**

#### API Response Format Changes
- **BREAKING**: All API responses now include `schema_version` field
  - Example: `{"schema_version": "1.0.0", "adapters": [...]}`
  - **Impact**: API clients must handle new field or configure JSON parsers to ignore unknown fields
  - **Migration**: Update client code to parse `schema_version` or add permissive JSON parsing
  - **Backward Compatibility**: Old clients can safely ignore this field if using permissive parsers

- **BREAKING**: Adapter metadata now requires `version` and `lifecycle_state` fields
  - New required fields in `AdapterResponse`:
    - `version`: String (SemVer or monotonic)
    - `lifecycle_state`: Enum (draft, active, deprecated, retired)
  - **Impact**: API consumers expecting old schema will receive errors
  - **Migration**: Update API client models to include new fields
  - **Backward Compatibility**: Not backward compatible - clients must update

#### Database Schema Changes
- **BREAKING**: Lifecycle state transitions enforced by SQL triggers
  - Retired state is terminal (cannot transition back to active/deprecated/draft)
  - Ephemeral tier adapters cannot transition to deprecated state
  - No backward state transitions allowed (e.g., active → draft is forbidden)
  - **Impact**: Automation scripts attempting invalid transitions will fail with database errors
  - **Migration**: Update automation to respect state machine rules
  - **Rollback**: Revert to migration 0074 if needed (loses trigger enforcement)

- **BREAKING**: Direct SQL updates to `adapters.version` or `adapters.lifecycle_state` now validated
  - **Impact**: Raw SQL scripts bypassing application layer will fail if transitions are invalid
  - **Migration**: Use application API or `aosctl` CLI for state transitions
  - **Workaround**: Disable triggers temporarily for bulk migrations (not recommended)

#### Type System Changes
- **BREAKING**: New canonical structs: `AdapterMeta`, `AdapterStackMeta`
  - Old ad-hoc metadata types deprecated
  - **Impact**: Rust code using old types must be updated
  - **Migration**: Use `adapteros-db::metadata::{AdapterMeta, AdapterStackMeta}`
  - **Compatibility**: Old types removed in future major version (deprecation warning added)

- **BREAKING**: WorkflowType parsing now case-insensitive
  - Previously: Only PascalCase ("Parallel", "UpstreamDownstream", "Sequential")
  - Now: Accepts lowercase ("parallel", "upstreamdownstream", "sequential")
  - **Impact**: Database values stored in lowercase now parse correctly
  - **Migration**: No action required (backward compatible enhancement)
  - **Note**: This fixes a bug, not a true breaking change

### Fixed (PRD-02: Critical Fixes)

#### Database Integrity
- Fixed lifecycle state transition validation allowing invalid transitions
  - Issue: Application-layer validation could be bypassed via direct SQL
  - Fix: Added SQL triggers enforcing state machine at database level
  - Impact: Prevents database corruption from invalid state changes

- Fixed ephemeral adapters incorrectly allowed to transition to deprecated state
  - Issue: Ephemeral tier should skip deprecated and go directly to retired
  - Fix: SQL trigger prevents ephemeral + deprecated combination
  - Impact: Enforces correct ephemeral adapter lifecycle

#### Documentation Accuracy
- Corrected migration number references in documentation (0070 → 0068)
  - Issue: Comments referenced wrong migration for metadata normalization
  - Fix: Updated `crates/adapteros-db/src/adapters.rs:357` comment
  - Impact: Developers now reference correct migration file

- Fixed WorkflowType::from_str case sensitivity bug
  - Issue: Database stores lowercase but parser expected PascalCase
  - Fix: Made parser case-insensitive by converting to lowercase
  - Impact: Database values now parse correctly

### Added (PRD-07: Security - Implemented Concurrently)

#### JWT Authentication
- **JWT Token System**: Ed25519-based JWT tokens with 8-hour TTL (migration 0077)
  - Stateless authentication with cryptographic signatures
  - Token refresh mechanism for long-lived sessions
  - Revocation support via deny-list
  - Location: `migrations/0077_jwt_security.sql`

#### RBAC System
- **Role-Based Access Control**: 5 roles with 20+ granular permissions
  - Roles: Admin (full access), Operator (runtime ops), SRE (infra debug), Compliance (audit-only), Viewer (read-only)
  - Permissions: AdapterRegister, AdapterLoad, PolicyApply, TenantManage, AuditView, etc.
  - Per-endpoint permission checks in `adapteros-server-api/src/permissions.rs`
  - Location: `crates/adapteros-server-api/src/permissions.rs`

#### Tenant Security
- **Enhanced Multi-Tenancy**: Tenant-level security isolation (migration 0078)
  - Tenant-scoped API keys
  - Tenant-specific resource quotas
  - Cross-tenant access prevention
  - Location: `migrations/0078_tenant_security.sql`

#### Audit Logging
- **Immutable Audit Trail**: Append-only audit logs for all security-relevant operations
  - Records: user_id, action, resource, status, timestamp
  - Cannot be modified or deleted (database constraints)
  - Queryable via `/v1/audit/logs` API (Admin/SRE/Compliance roles only)
  - Location: `crates/adapteros-db/src/audit.rs`

### Fixed (Build System: 70 Compilation Errors Resolved)

#### Lora-Worker Fixes
- Resolved 70 compilation errors in `adapteros-lora-worker` crate
  - Fixed missing dependencies: `half`, `bytemuck` (20 errors)
  - Restored missing types: `StackHandle`, `UmaStats`, `KernelAdapterBackend` (11 errors)
  - Fixed method signature mismatches (16 errors)
  - Resolved lifetime borrowing issues (1 error)
  - Fixed Send trait violations in async code (2 errors)
  - Miscellaneous fixes (20 errors)
  - Impact: Unblocks server-api, CLI, and UI integration

#### Trait Object Safety
- Fixed `TelemetryEventSink` trait to be dyn-compatible
  - Issue: Trait used async methods, preventing trait object usage
  - Fix: Converted to impl Future + Send pattern
  - Impact: Telemetry can now be used dynamically

#### Sign-Migrations
- **⚠️ Still Broken**: Lifetime borrowing error in tests
  - Issue: Temporary value dropped while borrowed in `signing_tests.rs:64`
  - Status: Not yet fixed (non-blocking for runtime)
  - Impact: Cannot run migration signing tests

### Documentation (PRD-02)

#### Developer Documentation
- Updated `CLAUDE.md` with API types usage guide
  - Quick start examples for schema versioning
  - Common patterns for error responses and pagination
  - OpenAPI integration instructions
  - TypeScript integration guide
  - Testing best practices

#### Database Documentation
- Documented 74 database migrations with Ed25519 signatures
  - Cryptographic verification of migration integrity
  - Tamper-proof migration history
  - Signature file: `migrations/signatures.json`

#### Architecture Documentation
- Added PRD-02 to architecture index
  - Lifecycle state machine diagrams
  - Database schema evolution timeline
  - API versioning strategy

### Known Issues (Blocking Production Deployment)

**Critical Blockers:**
1. **Database Test Failures**: 7/30 tests failing (audit and lifecycle tests)
   - `audit::tests::test_resource_audit_trail`
   - `audit::tests::test_audit_log_creation`
   - `routing_telemetry_bridge::tests::test_persist_router_decisions`
   - `lifecycle::tests::test_no_op_transition`
   - `routing_decisions::tests::test_routing_decision_crud`
   - `lifecycle::tests::test_adapter_lifecycle_transition`
   - `lifecycle::tests::test_check_active_stack_references`
   - **Status**: Under investigation
   - **ETA**: 2-4 hours to fix

2. **Sign-Migrations Lifetime Error**: Cannot compile migration signing tests
   - **Status**: Non-blocking for runtime (only affects test suite)
   - **ETA**: 1 hour to fix

3. **UI TypeScript Errors**: 465 syntax errors (duplicate try-catch blocks)
   - **Status**: Blocks UI build
   - **ETA**: 3-4 hours to fix (per PRD-02_FIX_ROADMAP.md)

4. **Server API Integration**: Incomplete (blocked by lora-worker dependency)
   - **Status**: Compilation errors resolved, integration work remains
   - **ETA**: 2-3 hours to integrate

5. **CLI Integration**: Incomplete (blocked by Metal shader build)
   - **Status**: Compilation errors resolved, CLI commands not yet added
   - **ETA**: 1-2 hours to integrate

**Non-Critical Issues:**
- SQLX validation disabled (queries are stubs, validated at runtime only)
- Some clippy warnings remain (unused imports, deprecated functions)
- No migration guide for breaking changes (documentation gap)
- CHANGELOG not updated (this file addresses that)

### Migration Notes (Important - Read Before Upgrading)

**⚠️ BREAKING CHANGES PRESENT - FOLLOW MIGRATION GUIDE ⚠️**

**Pre-Upgrade Requirements:**
1. Backup production database before running migrations
2. Review breaking changes in API response format
3. Update client code to handle `schema_version` field
4. Test state transition automation against new SQL triggers
5. Review rollback procedures (documented in deployment guide)

**Database Migration Order:**
```bash
# Run migrations in this exact order:
./target/release/aosctl db migrate --target 0068  # Metadata normalization
./target/release/aosctl db migrate --target 0070  # Routing decisions
./target/release/aosctl db migrate --target 0071  # Lifecycle history
./target/release/aosctl db migrate --target 0075  # State transition triggers
./target/release/aosctl db migrate --target 0077  # JWT security (PRD-07)
./target/release/aosctl db migrate --target 0078  # Tenant security (PRD-07)
```

**API Client Migration:**
1. Update JSON parsers to handle `schema_version` field (or configure to ignore unknown fields)
2. Add `version` and `lifecycle_state` fields to AdapterResponse models
3. Update error handling for lifecycle state transition failures
4. Test with schema version negotiation (if implementing version-aware clients)

**Rust Code Migration:**
1. Replace ad-hoc metadata types with `adapteros-db::metadata::{AdapterMeta, AdapterStackMeta}`
2. Update `use` statements to import from `adapteros-api-types` instead of inline definitions
3. Recompile and run tests to catch type mismatches

**Rollback Procedures:**
- Database: Restore from backup or rollback to migration 0067
- Backend: Redeploy previous binary version
- Frontend: Restore previous UI build
- **Warning**: Data created with new schema may be incompatible with old code

**Estimated Downtime:**
- Database migration: 2-5 minutes (depends on table sizes)
- Backend deployment: 1-2 minutes
- Frontend deployment: 30 seconds
- **Total**: ~5-10 minutes for full deployment

**Post-Upgrade Verification:**
```bash
# Verify database schema
./target/release/aosctl db verify-schema

# Test lifecycle state transitions
./target/release/aosctl adapter lifecycle set test-adapter active
./target/release/aosctl adapter lifecycle set test-adapter deprecated

# Verify API schema version
curl http://localhost:8080/api/health | jq '.schema_version'
# Expected output: "1.0.0"

# Test trigger enforcement (should fail)
./target/release/aosctl adapter lifecycle set test-adapter active
# Expected error: "Cannot transition from retired state (terminal)"
```

### Contributors (PRD-02 Implementation)

- **Primary Developer**: James KC Auchterlonie (@rogu3bear)
- **Email**: vats-springs0m@icloud.com
- **Implementation Period**: 2025-11-15 to 2025-11-19
- **Code Review**: Pending (awaiting blocker resolution)

### References

- **PRD-02 Documentation**: `/PRD-02_INDEX.md`
- **Version Guarantee Policy**: `/docs/VERSION_GUARANTEES.md`
- **Migration Guide**: `/docs/PRD-02_MIGRATION_GUIDE.md` (to be created)
- **Deployment Guide**: `/docs/DEPLOYMENT.md` (to be updated)
- **Fix Roadmap**: `/PRD-02_FIX_ROADMAP.md`

---

**Changelog Entry Status:** DRAFT
**Requires Review:** Yes
**Merge After:** All critical blockers resolved (7 database tests pass, compilation clean)
**Target Release:** v0.05-unstable

---

**END OF CHANGELOG ADDITIONS**
