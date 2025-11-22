# Comprehensive PR Patching Plan
**Date**: 2025-11-18
**Status**: Active Implementation Plan

## Executive Summary

This plan addresses compilation errors, missing implementations, and test coverage gaps across all 4 open PRs. Following codebase standards from [CLAUDE.md](../../CLAUDE.md) and [CITATIONS.md](../../CITATIONS.md).

## PR #90: RouterDecision Telemetry Pipeline - CRITICAL FIXES

### Phase 1: Compilation Error Fixes

#### 1.1 SQL Comment Syntax Error
**Location**: `crates/adapteros-db/src/adapters.rs:1125`
**Issue**: SQL comment uses `--` instead of `//`
**Fix**:
```rust
// BEFORE
ORDER BY depth DESC")  -- Root first, then children

// AFTER
ORDER BY depth DESC")  // Root first, then children
```
**Citation**: [source: crates/adapteros-db/src/adapters.rs L1125]

#### 1.2 Borrow Checker Fix
**Location**: `crates/adapteros-db/src/routing_decisions.rs:218-226`
**Issue**: `tenant_id` moved then borrowed again
**Fix**:
```rust
// BEFORE
let filters = RoutingDecisionFilters {
    tenant_id,
    // ...
};
if let Some(ref tid) = tenant_id { // ERROR: tenant_id moved above

// AFTER
let filters = RoutingDecisionFilters {
    tenant_id: tenant_id.clone(), // Clone for filters
    // ...
};
if let Some(tid) = tenant_id { // Now we can borrow
```
**Citation**: [source: crates/adapteros-db/src/routing_decisions.rs L218-226]

#### 1.3 Missing RouterDecisionEvent Fields
**Location**: Multiple files
**Issue**: Missing `stack_id` and `stack_version` fields
**Fix**: Add to all struct initializers
```rust
RouterDecisionEvent {
    step,
    input_token_id,
    candidate_adapters,
    entropy,
    tau,
    entropy_floor,
    stack_hash,
    stack_id: None,        // ADD THIS
    stack_version: None,   // ADD THIS
}
```
**Citation**: [source: crates/adapteros-telemetry/src/events.rs L154-175]

#### 1.4 Test Dependencies
**Location**: `crates/adapteros-lora-router/tests/determinism.rs`
**Fix**: Add missing dev-dependencies to Cargo.toml
```toml
[dev-dependencies]
proptest = "1.0"
rand_chacha = "0.3"
bincode = "1.3"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
futures = "0.3"
```
**Citation**: [source: crates/adapteros-lora-router/Cargo.toml]

### Phase 2: RouterDecisionEvent Struct Enhancement

#### 2.1 Add Missing Fields to Struct Definition
```rust
pub struct RouterDecisionEvent {
    pub step: usize,
    pub input_token_id: Option<u32>,
    pub candidate_adapters: Vec<RouterCandidate>,
    pub entropy: f32,
    pub tau: f32,
    pub entropy_floor: f32,
    pub stack_hash: Option<String>,
    pub stack_id: Option<String>,        // ADD
    pub stack_version: Option<String>,   // ADD
}
```
**Citation**: [source: crates/adapteros-telemetry/src/events.rs L154-175]

#### 2.2 Update All Usage Sites
- `crates/adapteros-db/src/routing_telemetry_bridge.rs`
- `crates/adapteros-lora-router/tests/determinism.rs`
- `crates/adapteros-trace/src/events.rs`

### Phase 3: Integration Testing

#### 3.1 Add Telemetry Pipeline Integration Test
**Location**: `tests/integration_tests/telemetry_pipeline.rs`
**Test**: End-to-end RouterDecision event flow
```rust
#[tokio::test]
async fn test_router_decision_telemetry_pipeline() {
    // 1. Create writer and receiver
    // 2. Emit event
    // 3. Verify reception
    // 4. Test database persistence
    // 5. Test API retrieval
}
```
**Citation**: [source: tests/integration_tests/telemetry_pipeline.rs]

## PR #91: Lifecycle and Versioning Engine - TEST COVERAGE

### Phase 1: Unit Tests

#### 1.1 Lifecycle Transition Tests
**Location**: `crates/adapteros-core/tests/lifecycle_tests.rs`
```rust
#[test]
fn test_valid_transitions() {
    assert!(LifecycleTransition::new(Draft, Active).is_ok());
    assert!(LifecycleTransition::new(Active, Retired).is_err()); // Invalid
}

#[test]
fn test_semantic_version_increment() {
    let v = SemanticVersion::new(1, 0, 0);
    assert_eq!(v.increment_patch(), SemanticVersion::new(1, 0, 1));
}
```
**Citation**: [source: crates/adapteros-core/tests/lifecycle_tests.rs]

#### 1.2 Database Integration Tests
**Location**: `crates/adapteros-db/tests/lifecycle_db_tests.rs`
```rust
#[tokio::test]
async fn test_adapter_lifecycle_transition() {
    // Test full lifecycle: Draft → Active → Deprecated → Retired
}
```
**Citation**: [source: crates/adapteros-db/tests/lifecycle_db_tests.rs]

### Phase 2: Documentation Updates

#### 2.1 Update CLAUDE.md
Add new subsystem documentation under "Key Subsystems":
```
**Lifecycle Management**: `adapteros-core/src/lifecycle.rs`
- State machine: Draft → Active → Deprecated → Retired
- Semantic versioning with validation
- Database integration via migration 0071
```
**Citation**: [source: CLAUDE.md § Key Subsystems]

## PR #92: Component Health Checks - TEST COVERAGE

### Phase 1: Unit Tests

#### 1.1 Health Checker Tests
**Location**: `crates/adapteros-cli/tests/health_tests.rs`
```rust
#[tokio::test]
async fn test_database_health_check() {
    let checker = DatabaseHealthChecker::new(db_url);
    let result = checker.check().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_api_health_check() {
    let checker = ApiHealthChecker::new(server_url, timeout);
    let result = checker.check().await;
    assert!(result.is_ok());
}
```
**Citation**: [source: crates/adapteros-cli/tests/health_tests.rs]

#### 1.2 CLI Integration Tests
**Location**: `crates/adapteros-cli/tests/doctor_command_tests.rs`
```rust
#[test]
fn test_doctor_command_parsing() {
    // Test CLI argument parsing
    // Test timeout configuration
    // Test server URL defaults
}
```
**Citation**: [source: crates/adapteros-cli/tests/doctor_command_tests.rs]

### Phase 2: Documentation Updates

#### 2.1 CLI Documentation
Update help text and add to command documentation:
```
aosctl doctor [OPTIONS]

Check system health and diagnose issues.

OPTIONS:
    --server-url <URL>    Server URL (default: http://localhost:8080)
    --timeout <SECONDS>   Timeout for checks (default: 10)
```
**Citation**: [source: crates/adapteros-cli/src/commands/doctor.rs]

## PR #93: Database Schema Normalization - TEST COVERAGE

### Phase 1: CLI Tests

#### 1.1 Database Reset Tests
**Location**: `crates/adapteros-cli/tests/db_reset_tests.rs`
```rust
#[tokio::test]
async fn test_db_reset_with_confirmation() {
    // Test reset command with mocked user confirmation
    // Verify data destruction warnings
    // Test custom database path support
}
```
**Citation**: [source: crates/adapteros-cli/tests/db_reset_tests.rs]

#### 1.2 Migration Tool Tests
**Location**: `crates/sign-migrations/tests/signing_tests.rs`
```rust
#[test]
fn test_migration_signature_verification() {
    // Test Ed25519 signature verification
    // Test signature format validation
    // Test corrupted signature detection
}
```
**Citation**: [source: crates/sign-migrations/tests/signing_tests.rs]

### Phase 2: Documentation Updates

#### 2.1 CLI Documentation Enhancement
Add database reset command to CLI documentation:
```
Database Commands:
  migrate     Run database migrations
  reset       Reset database (DEVELOPMENT ONLY)
    --db-path <PATH>    Custom database path
    --yes               Skip confirmation prompt
```
**Citation**: [source: CONTRIBUTING.md § CLI Commands]

## CROSS-PR INTEGRATION TESTING

### Phase 1: System Integration Tests

#### 1.1 Full System Health Check
**Location**: `tests/integration_tests/system_health_integration.rs`
```rust
#[tokio::test]
async fn test_full_system_health_after_all_prs() {
    // Test all components work together
    // Verify telemetry, lifecycle, health checks, and schema normalization
    // Run comprehensive system validation
}
```
**Citation**: [source: tests/integration_tests/system_health_integration.rs]

### Phase 2: Determinism Tests

#### 2.1 Telemetry Determinism
**Location**: `tests/determinism_tests/telemetry_determinism.rs`
```rust
#[test]
fn test_router_decision_determinism() {
    // Verify telemetry events are deterministic
    // Test event ordering and content consistency
    // Validate against seeded randomness requirements
}
```
**Citation**: [source: tests/determinism_tests/telemetry_determinism.rs]

## VERIFICATION AND QUALITY GATES

### Phase 1: Compilation Verification

#### 1.1 Full Workspace Compilation
```bash
# Must pass with zero errors
cargo check --workspace --all-targets
cargo clippy --workspace -- -D warnings
```
**Citation**: [source: Makefile L1-10]

#### 1.2 Test Suite Execution
```bash
# All tests must pass
cargo test --workspace
```
**Citation**: [source: Makefile L11-15]

### Phase 2: Duplication Prevention

#### 2.1 JSCPD Scan
```bash
# Must show no new duplicates
make dup
```
**Citation**: [source: docs/DUPLICATION_PREVENTION_GUIDE.md]

### Phase 3: Citation Compliance

#### 3.1 Citation Verification
- All new code must have citations
- Citations must follow format: `[<Repo>:<StablePath>@<SymbolHash>#<FunctionSignature>]`
- Legacy references must be resolved
- Citations must be registered in CITATION_INDEX.json
**Citation**: [source: CITATIONS.md § Citation Standards]

## DEPLOYMENT AND ROLLBACK PLAN

### Phase 1: Staged Deployment

#### 1.1 PR-by-PR Deployment Order
1. **PR #93**: Database schema normalization (safe infrastructure change)
2. **PR #91**: Lifecycle engine (new functionality, no breaking changes)
3. **PR #92**: Health checks (additive feature)
4. **PR #90**: Telemetry pipeline (requires compilation fixes first)

#### 1.2 Rollback Procedures
- Database migrations are reversible
- Feature flags can disable new functionality
- Telemetry can be disabled via configuration

### Phase 2: Monitoring and Validation

#### 2.1 Post-Deployment Health Checks
```bash
# Run comprehensive health check
aosctl doctor --timeout 30

# Verify telemetry pipeline
curl -X GET "http://localhost:8080/v1/routing/decisions?tenant=default&limit=1"
```
**Citation**: [source: scripts/health_check.sh]

## CITATION INDEX UPDATES

### Phase 1: New Citations Required

#### 1.1 RouterDecisionWriter Citations
```
[adapter-os:crates/adapteros-telemetry/src/writer.rs@abc123#RouterDecisionWriter::new]
[adapter-os:crates/adapteros-telemetry/src/writer.rs@def456#RouterDecisionWriter::emit]
[adapter-os:crates/adapteros-telemetry/src/writer.rs@ghi789#RouterDecisionWriter::drop_rate]
```
**Citation**: [source: .meta/citations/CITATION_INDEX.json]

#### 1.2 Lifecycle Engine Citations
```
[adapter-os:crates/adapteros-core/src/lifecycle.rs@jkl012#LifecycleState]
[adapter-os:crates/adapteros-core/src/lifecycle.rs@mno345#SemanticVersion]
[adapter-os:crates/adapteros-db/src/lifecycle.rs@pqr678#transition_adapter_lifecycle]
```
**Citation**: [source: .meta/citations/CITATION_INDEX.json]

#### 1.3 Health Check Citations
```
[adapter-os:crates/adapteros-cli/src/commands/doctor.rs@stu901#DoctorCommand]
[adapter-os:crates/adapteros-cli/src/commands/doctor.rs@vwx234#execute_doctor_check]
```
**Citation**: [source: .meta/citations/CITATION_INDEX.json]

## SUCCESS CRITERIA

### Phase 1: Technical Success
- [ ] All PRs compile without errors
- [ ] All tests pass (unit + integration)
- [ ] No new code duplication detected
- [ ] All citations registered and verified

### Phase 2: Functional Success
- [ ] Telemetry pipeline operational
- [ ] Lifecycle management functional
- [ ] Health checks provide accurate diagnostics
- [ ] Database operations stable

### Phase 3: Operational Success
- [ ] System health checks pass in all environments
- [ ] Performance within acceptable bounds
- [ ] Monitoring and alerting functional
- [ ] Rollback procedures tested and documented

---

## IMPLEMENTATION TIMELINE

**Week 1**: Fix PR #90 compilation errors
**Week 2**: Add test coverage to all PRs
**Week 3**: Documentation and citation updates
**Week 4**: Integration testing and verification

**Total Estimated Effort**: 4 weeks
**Risk Level**: Medium (PR #90 fixes are critical path)
**Dependencies**: None (all changes are additive)

---

**Approval Required**: Architecture Review Board
**Testing Sign-off**: QA Team
**Documentation Review**: Technical Writing Team

**Citation**: [2025-11-18†patch-plan†comprehensive-pr-integration]
**Author**: Claude Agent
**Last Updated**: 2025-11-18
