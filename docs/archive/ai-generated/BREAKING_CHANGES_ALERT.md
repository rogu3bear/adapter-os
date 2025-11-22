# Breaking Changes Alert

**Generated:** 2025-11-17
**Alert Level:** 🔴 CRITICAL
**Purpose:** Track schema drift and breaking changes across UI ↔ CLI ↔ Server

---

## 🔴 CRITICAL: Immediate Action Required

### 1. Merge Conflicts in UI API Layer

**Severity:** CRITICAL
**Impact:** Build failures, type mismatches, runtime errors
**Files Affected:**
- `ui/src/api/types.ts` (2,374 lines)
- `ui/src/api/client.ts` (2,554 lines)

**Conflict Count:** 22 merge conflict markers

**Example Conflict:**
```typescript
// ui/src/api/types.ts:47-96

export type UserRole = 'admin' | 'operator' | 'sre' | 'compliance' | 'auditor' | 'viewer';

export type UserRole = 'Admin' | 'Operator' | 'SRE' | 'Compliance' | 'Viewer';
>
```

**Required Actions:**
1. ✅ Review both branches' type definitions
2. ✅ Choose canonical casing (recommend lowercase for consistency with Rust)
3. ✅ Merge 'auditor' role into final definition
4. ✅ Update all UI components consuming `UserRole`
5. ✅ Add pre-commit hook to prevent future conflicts

**Deadline:** Before next deployment

---

### 2. Duplicate UserRole Type Definition

**Severity:** CRITICAL
**Impact:** TypeScript compilation errors, ambiguous type resolution

**Definition 1 (Line 47):**
```typescript
export type UserRole = 'admin' | 'operator' | 'sre' | 'compliance' | 'auditor' | 'viewer';
```

**Definition 2 (Line 95):**
```typescript
export type UserRole = 'Admin' | 'Operator' | 'SRE' | 'Compliance' | 'Viewer';
```

**Differences:**
| Aspect | Definition 1 | Definition 2 |
|--------|-------------|-------------|
| Casing | lowercase | PascalCase |
| Roles | 6 (includes 'auditor') | 5 (missing 'auditor') |
| Location | Line 47 | Line 95 |

**Recommended Resolution:**
```typescript
// CANONICAL (choose this)
export type UserRole = 'admin' | 'operator' | 'sre' | 'compliance' | 'auditor' | 'viewer';
```

**Rationale:**
- Matches Rust enum naming convention (lowercase variants)
- Includes all roles from RBAC system
- Consistent with existing codebase

**Required Actions:**
1. ✅ Remove second definition (line 95)
2. ✅ Search codebase for 'Admin' | 'Operator' references
3. ✅ Replace with lowercase variants
4. ✅ Update RBAC documentation
5. ✅ Run TypeScript type checker

---

### 3. adapteros-server-api Crate Disabled

**Severity:** HIGH
**Impact:** 62 compilation errors, REST API handlers unavailable

**Status:** Crate excluded from workspace (per CLAUDE.md)

**Affected Endpoints:** ~150+ REST handlers

**Known Issues:**
- Type mismatches in handler signatures
- Missing dependencies
- Outdated Axum integration

**Required Actions:**
1. ✅ Audit compilation errors
2. ✅ Fix type mismatches
3. ✅ Update dependencies
4. ✅ Re-enable in `Cargo.toml` workspace
5. ✅ Run integration tests

**Blockers:**
- Depends on API contract stabilization
- May require OpenAPI schema updates

---

## 🟡 WARNING: Schema Drift Detected

### 4. Rust vs TypeScript Field Naming Inconsistencies

**Severity:** MEDIUM
**Impact:** Manual mapping required, potential runtime bugs

| Type | Rust Field | TypeScript Field | Status |
|------|-----------|-----------------|--------|
| `Adapter` | `adapter_id` | `id` + `adapter_id` | ⚠️ Inconsistent |
| `RouterDecision` | `step` | `step` | ✅ Match |
| `InferenceTrace` | `adapters_used` | `adapters_used` | ✅ Match |

**Example Issue:**
```rust
// Rust (adapteros-api-types/src/adapters.rs:19)
pub struct AdapterResponse {
    pub id: String,           // Primary key
    pub adapter_id: String,   // Adapter identifier
    ...
}
```

```typescript
// TypeScript (ui/src/api/types.ts:409+)
export interface Adapter {
    id: string;              // Which is this?
    adapter_id?: string;     // Optional?
    ...
}
```

**Recommendation:**
- Standardize on snake_case JSON fields (matches Rust serde defaults)
- Use `adapter_id` consistently
- Reserve `id` for database primary keys only

---

### 5. Missing TypeScript Types

**Severity:** MEDIUM
**Impact:** Incomplete UI type coverage, manual type assertions

**Types in Rust but not TypeScript:**
1. `AdapterManifest` (adapteros-api-types/src/adapters.rs:62)
2. `AdapterHealthResponse` (adapteros-api-types/src/adapters.rs:84)
3. `RouterCandidate` (adapteros-api-types/src/inference.rs:41)
4. `BundleMetadata` (adapteros-api-types/src/telemetry.rs:68)

**Required Actions:**
1. ✅ Generate TypeScript types from OpenAPI schema
2. ✅ Add to `ui/src/api/types.ts`
3. ✅ Update API client methods

**Automation Opportunity:**
- Use `openapi-typescript` to auto-generate from `utoipa` derives

---

### 6. Telemetry Type Name Mismatch

**Severity:** MEDIUM
**Impact:** Confusing type names, potential mapping errors

| Rust | TypeScript | Status |
|------|------------|--------|
| `ApiTelemetryEvent` | `UnifiedTelemetryEvent` | ⚠️ Name mismatch |
| `TelemetryBundleResponse` | `TelemetryBundle` | ⚠️ Suffix missing |

**Recommendation:**
- Align TypeScript names with Rust API types
- Suffix responses with `Response` consistently

---

## 🟢 LOW PRIORITY: Documentation Gaps

### 7. Missing API Contract Documentation

**Severity:** LOW
**Impact:** Developer confusion, integration delays

**Missing Documentation:**
- OpenAPI schema (generated but not versioned)
- Type compatibility matrix
- Breaking change policy
- Schema evolution strategy

**Required Actions:**
1. ✅ Generate OpenAPI schema: `cargo run --bin aos-cp -- --export-openapi > docs/api/openapi.json`
2. ✅ Version schema with API
3. ✅ Document breaking change policy
4. ✅ Add schema validation to CI

---

## Change Detection Matrix

### Type Compatibility Status

| Type | Rust ✅ | TypeScript ✅ | CLI ✅ | Drift 🔍 |
|------|--------|--------------|-------|---------|
| `AdapterResponse` | ✅ | ✅ | - | ✅ Match |
| `RegisterAdapterRequest` | ✅ | ❌ | ✅ | ⚠️ Missing TS |
| `InferRequest` | ✅ | ✅ | ✅ | ✅ Match |
| `InferResponse` | ✅ | ✅ | ✅ | ✅ Match |
| `RouterDecision` | ✅ | ✅ | - | ✅ Match |
| `RouterCandidate` | ✅ | ❌ | - | ⚠️ Missing TS |
| `UserRole` | - | ⚠️ DUPLICATE | - | 🔴 Conflict |
| `AdapterManifest` | ✅ | ❌ | - | ⚠️ Missing TS |
| `ApiTelemetryEvent` | ✅ | ⚠️ (`UnifiedTelemetryEvent`) | - | ⚠️ Name drift |

**Legend:**
- ✅ Present and correct
- ❌ Missing
- ⚠️ Present but inconsistent
- 🔴 Critical issue

---

## Schema Evolution Strategy

### Current State
- **No versioning:** Types evolve ad-hoc
- **No validation:** Schema drift detected manually
- **No changelog:** Breaking changes undocumented

### Proposed Strategy

#### 1. OpenAPI as Source of Truth
```yaml
# docs/api/openapi.json
openapi: 3.1.0
info:
  version: "1.0.0"  # Semantic versioning
schemas:
  AdapterResponse:
    type: object
    properties:
      adapter_id:
        type: string
      # ...
```

#### 2. Auto-Generate TypeScript
```bash
# In CI
npm run generate-types
# Uses: npx openapi-typescript docs/api/openapi.json --output ui/src/api/generated.ts
```

#### 3. Contract Testing
```typescript
// tests/contract/adapter-types.test.ts
import { AdapterResponse } from '@/api/generated';
import { validateAgainst } from 'openapi-validator';

test('AdapterResponse matches schema', () => {
  const response: AdapterResponse = { ... };
  expect(validateAgainst(response, 'AdapterResponse')).toBe(true);
});
```

#### 4. Breaking Change Policy
- **Major version bump:** Field removed, type changed
- **Minor version bump:** Field added (optional)
- **Patch version bump:** Documentation only

---

## Automated Drift Detection

### CI Workflow Proposal

**.github/workflows/schema-drift.yml:**
```yaml
name: Schema Drift Detection

on: [pull_request]

jobs:
  detect-drift:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Generate OpenAPI schema
        run: cargo run --bin aos-cp -- --export-openapi > /tmp/openapi.json

      - name: Compare with baseline
        run: |
          diff docs/api/openapi.json /tmp/openapi.json || {
            echo "Schema drift detected!"
            echo "Run: cargo run --bin aos-cp -- --export-openapi > docs/api/openapi.json"
            exit 1
          }

      - name: Check TypeScript types
        run: |
          cd ui
          npm run type-check

      - name: Validate contract compatibility
        run: |
          npm run validate-contracts
```

---

## Resolution Checklist

### Immediate (This Week)
- [ ] ✅ Resolve UI merge conflicts (22 markers)
- [ ] ✅ Fix UserRole duplicate definition
- [ ] ✅ Standardize on lowercase role names
- [ ] ✅ Update RBAC documentation

### Short-term (This Month)
- [ ] ✅ Fix adapteros-server-api compilation (62 errors)
- [ ] ✅ Add missing TypeScript types (4 types)
- [ ] ✅ Align telemetry type names
- [ ] ✅ Generate OpenAPI schema
- [ ] ✅ Add schema drift CI check

### Long-term (Next Quarter)
- [ ] ✅ Implement OpenAPI-driven development
- [ ] ✅ Auto-generate TypeScript from OpenAPI
- [ ] ✅ Add contract testing suite
- [ ] ✅ Document breaking change policy

---

## Contact & Escalation

**For Questions:**
- Schema changes: Repo Health Agent
- API contracts: Server team
- Type compatibility: UI team

**For Escalation:**
- Critical drift: Notify all teams
- Breaking changes: Requires approval before merge

---

**Report Version:** 1.0
**Next Update:** Weekly or on schema changes
**Automation Status:** Manual (recommend weekly CI scan)
