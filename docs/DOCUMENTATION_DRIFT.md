# Documentation Drift: Implementation Validation Framework

**Purpose:** Comprehensive documentation of the documentation drift concept, systematic validation framework, and current findings for AdapterOS.

**Last Updated:** 2025-12-13
**Status:** Active validation framework

---

## Table of Contents

1. [What is Documentation Drift?](#what-is-documentation-drift)
2. [The Validation Framework](#the-validation-framework)
3. [Rule Categories](#rule-categories)
4. [Current Findings](#current-findings)
5. [Resolution Process](#resolution-process)
6. [Historical Context](#historical-context)

---

## What is Documentation Drift?

**Documentation drift** is the systematic validation that implementation matches documented specifications. Unlike traditional documentation becoming outdated, this framework ensures **code doesn't silently diverge** from documented requirements.

### Core Problem

Traditional documentation maintenance focuses on keeping docs current with code changes. Documentation drift focuses on ensuring **code stays compliant with documented invariants**, security requirements, and architectural decisions.

### Why It Matters

AdapterOS implements critical security and correctness invariants:

- **Tenant isolation** prevents cross-tenant data leakage
- **Path security** prevents persistent data in `/tmp` (or macOS `/private/tmp`)
- **Deterministic routing** ensures reproducible adapter selection
- **Q15 quantization** maintains precision-critical gate values

**Documentation drift detection ensures these invariants aren't violated by implementation changes.**

---

## The Validation Framework

The framework uses structured rules validated against the codebase to identify gaps between documentation and implementation.

### Rule Structure

Each rule defines an invariant with validation criteria:

```json
{
  "id": "fs-01",
  "category": "path",
  "description": "Runtime state must live under canonical paths and must not persist under /tmp (or /private/tmp)",
  "source": "directive + AGENTS.md + AGENTS.md"
}
```

### Validation Results

For each rule, the system reports:
- **✅ Matches**: Implementation correctly follows documentation
- **❌ Mismatches**: Implementation differs from documentation
- **⚠️ Gaps**: Areas not yet validated or implemented

### Execution

Validation is performed via systematic code analysis:

1. **Rule Definition**: Explicit invariants extracted from documentation
2. **Code Scanning**: Automated analysis of implementation against rules
3. **Gap Identification**: Missing validations or implementations flagged
4. **Status Reporting**: Structured findings with evidence and locations

---

## Rule Categories

### fs-01: Path Security
**Invariant:** Runtime state must live under canonical paths (`var/`) and must NOT persist under `/tmp` (or `/private/tmp`).

**Rationale:** `/tmp` is world-writable and can be manipulated by other users/processes.

**Validation:**
```rust
// ✅ CORRECT: Rejects /tmp and /private/tmp for persisted runtime state
pub fn resolve_telemetry_dir() -> Result<ResolvedPath> {
    resolve_env_or_default_no_tmp("AOS_TELEMETRY_DIR", DEFAULT_TELEMETRY_DIR, "telemetry-dir")
}

pub fn resolve_index_root() -> Result<ResolvedPath> {
    resolve_env_or_default_no_tmp("AOS_INDEX_DIR", DEFAULT_INDEX_ROOT, "index-root")
}
```

### lifecycle-01: Worker Lifecycle
**Invariant:** Worker states follow `created → registered → healthy → draining → stopped/error` transitions and are tenant-scoped in storage/telemetry.

**Rationale:** Ensures clean worker lifecycle management and tenant isolation.

### backend-01: Backend Cache
**Invariant:** Backend selection is deterministic based on capabilities/config; model cache keys disambiguate backend + manifest and eviction is predictable.

**Rationale:** Ensures consistent backend selection and cache behavior.

### routing-01: Routing Policy
**Invariant:** All inference/replay routes through InferenceCore; routing uses Q15 gates with deterministic tie-breaking.

**Rationale:** Critical for deterministic, reproducible inference.

### telemetry-01: Telemetry Schema
**Invariant:** Telemetry events use consistent schema and carry tenant/model context.

**Rationale:** Enables proper observability and tenant attribution.

### tenant-01: Tenant Isolation
**Invariant:** Adapters/base models carry tenant identifiers everywhere; no cross-tenant leakage.

**Rationale:** Fundamental security boundary.

### test-01: Test Coverage
**Invariant:** Each rule above must have covering tests aligned with current specification.

**Rationale:** Ensures invariants remain enforced through testing.

---

## Current Findings

### Active Gaps (Require Code Changes)

#### fs-01: Path Security Gap
**Status:** ✅ Fully Resolved
- ✅ **Implemented:** Worker sockets, telemetry, manifest-cache, adapters, database, and index root paths
- ✅ **Unit Tests:** All path resolvers have `/tmp` rejection tests

**Impact:** Security vulnerability eliminated.

#### tenant-01: Tenant Isolation Gaps
**Status:** ✅ Fully Resolved
- ✅ **Implemented:** Handler validation, composite FKs, migration 0131 triggers
- ✅ **Implemented:** Adapter lifecycle DB queries tenant scoping validation (RECT-001)
- ✅ **Unit Tests:** Cross-tenant denial tests in `crates/adapteros-db/tests/tenant_adapter_lifecycle_tests.rs`

**Impact:** Security boundaries strictly enforced.

### Unverified Components (Require Validation)

#### lifecycle-01: Worker Lifecycle
**Status:** Partial validation
- ✅ **Validated:** WorkerStatus enum transitions
- ⚠️ **Unverified:** WorkerStatus mapping to database schema and telemetry events

#### backend-01: Backend Cache
**Status:** Partial validation
- ✅ **Validated:** BackendStrategy.select_backend() determinism
- ⚠️ **Unverified:** Cache eviction behavior, UI/telemetry exposure

### Resolved Rules

#### routing-01: Routing Policy
**Status:** ✅ Match
- Q15 denominator locked to 32767.0
- Policy hooks validated for live vs replay parity
- All inference routes through InferenceCore

#### telemetry-01: Telemetry Schema
**Status:** ✅ Match
- Tenant validation enforced via TelemetryFilters
- Buffer queries require tenant context

---

## Resolution Process

### Phase 1: Documentation Updates (✅ COMPLETED)

**Objective:** Ensure documentation accurately reflects current implementation status.

**Actions Taken:**
1. Updated `AGENTS.md` critical invariants with accurate implementation status
2. Added path security section to `docs/SECURITY.md`
3. Added tenant isolation implementation section to `docs/DATABASE.md`
4. Updated `docs/ARCHITECTURE.md` with backend cache and worker lifecycle gaps
5. Updated `plan/drift-summary.md` to reflect documentation rectification status

### Phase 2: Code Rectification (PENDING)

**Objective:** Implement missing validations and guards.

**Required Changes:**

#### Fix fs-01: Index Root Path Guard
```rust
// crates/adapteros-config/src/path_resolver.rs
pub fn resolve_index_root() -> Result<ResolvedPath> {
    resolve_env_or_default_no_tmp("AOS_INDEX_DIR", DEFAULT_INDEX_ROOT, "index-root")
}
```

#### Add Unit Tests
```rust
// crates/adapteros-config/src/path_resolver.rs
#[test]
fn test_index_root_rejects_tmp() {
    std::env::set_var("AOS_INDEX_DIR", "/tmp/indices");
    assert!(resolve_index_root().is_err());
}
```

#### Audit tenant-01: Adapter Lifecycle Queries
**Audit Scope:** All adapter CRUD operations in `crates/adapteros-db/src/adapters.rs`

**Validation Checklist:**
```sql
-- Every adapter query must include tenant_id filter
SELECT * FROM adapters WHERE id = ? AND tenant_id = ?
```

**Add Cross-Tenant Denial Tests:**
```rust
// crates/adapteros-db/tests/tenant_adapter_lifecycle_tests.rs
#[tokio::test]
async fn test_adapter_registration_cross_tenant_denial() {
    // Test that tenant-a cannot access tenant-b adapters
}
```

### Phase 3: Validation & Verification

**Post-Rectification Validation:**
```bash
# Re-run drift detection
cargo run --bin drift_detector

# Verify all rules show "match" status
cat plan/drift-findings.json | jq '.findings[].status'
```

**Integration Testing:**
```bash
# Run full test suite
bash scripts/test/all.sh all

# Run determinism checks
cargo test --test determinism_core_suite -- --test-threads=8
cargo test -p adapteros-lora-router --test determinism
bash scripts/check_fast_math_flags.sh

# Run security audit
cargo audit
```

---

## Historical Context

### Origin
Documentation drift validation was implemented to prevent silent divergence between AdapterOS's documented security invariants and actual implementation. Early detection revealed gaps in path security and tenant isolation enforcement.

### Evolution
- **2025-12-10:** Initial drift findings generated
- **2025-12-13:** Documentation updated to accurately reflect implementation status
- **Ongoing:** Code rectification and test coverage expansion

### Lessons Learned
1. **Documentation must reflect reality:** Even well-intentioned documentation can become misleading
2. **Systematic validation is essential:** Manual review misses implementation gaps
3. **Security invariants require enforcement:** Path restrictions and tenant isolation must be coded and tested
4. **Test coverage is critical:** Each invariant needs automated validation

---

## Related Documentation

- **`plan/drift-findings.json`**: Complete validation results and evidence
- **`plan/drift-actions.md`**: Detailed rectification plan
- **`plan/drift-summary.md`**: Human-readable status summary
- **`AGENTS.md`**: Critical invariants (updated with accurate status)
- **`docs/SECURITY.md`**: Path security implementation details
- **`docs/DATABASE.md`**: Tenant isolation implementation status

---

---

## General Documentation Freshness Tracking

### Overview

Beyond security invariants, this framework also tracks general documentation freshness to ensure API documentation, CLI documentation, and operational guides stay current with implementation.

### API Documentation Drift Detection

**Rule:** All registered API endpoints in `crates/adapteros-server-api/src/routes/mod.rs` must be documented in `docs/API_REFERENCE.md`.

**Validation Process:**
1. Extract all route registrations from `routes/mod.rs`
2. Compare against documented endpoints in `API_REFERENCE.md`
3. Flag missing endpoints for documentation
4. Verify OpenAPI spec alignment via `./scripts/ci/check_openapi_drift.sh`

**Status:** Active monitoring required. Last verified: 2026-01-13

### CLI Documentation Drift Detection

**Rule:** All CLI commands in `crates/adapteros-cli/src/main.rs` and `crates/adapteros-cli/src/app.rs` must be documented in `crates/adapteros-cli/docs/aosctl_manual.md`.

**Validation Process:**
1. Extract all command definitions from CLI source files
2. Compare against documented commands in `aosctl_manual.md`
3. Flag missing commands for documentation
4. Verify command examples match current implementation

**Status:** Active monitoring required. Last verified: 2026-01-13

### Operational Documentation Freshness

**Rule:** Operational procedures in `docs/OPERATIONS.md`, `docs/TROUBLESHOOTING.md`, and `docs/DEPLOYMENT.md` must reference existing scripts and current CLI commands.

**Validation Process:**
1. Verify all script paths referenced in docs exist
2. Verify CLI command syntax matches current implementation
3. Update examples with verified command syntax
4. Remove or mark deprecated script references

**Status:** Active monitoring required. Last verified: 2026-01-13

### Documentation Audit Process

**Frequency:** Monthly or before major releases

**Steps:**
1. Run API endpoint verification (compare routes to API_REFERENCE.md)
2. Run CLI command verification (compare commands to aosctl_manual.md)
3. Verify script paths and update references
4. Check UI deployment instructions for consistency
5. Update endpoint/command counts in documentation
6. Run OpenAPI drift check: `./scripts/ci/check_openapi_drift.sh --fix`

**Automation:**
- OpenAPI drift check: `./scripts/ci/check_openapi_drift.sh --fix` (automated in CI, run manually after API changes)
- Manual verification scripts recommended for API/CLI drift detection

**Note:** After updating API documentation, run `./scripts/ci/check_openapi_drift.sh --fix` to ensure the OpenAPI spec (`docs/api/openapi.json`) is synchronized with the codebase. This should be done before committing documentation changes.

**MLNavigator Inc 2025-12-13**
**Last Updated:** 2026-01-13 (Added general documentation freshness tracking)
