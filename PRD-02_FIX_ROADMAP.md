# PRD-02 Fix Roadmap
**Path from 62% to 100% Completion**

**Last Updated:** 2025-11-19
**Current Status:** 62% Verified Complete
**Target:** 100% Production Ready
**Estimated Effort:** 40-50 hours (5-6 work days)

---

## Quick Start

**Critical Path (Must Fix First):**
1. Add SQL trigger enforcement → 3 hours
2. Fix 70 lora-worker errors → 10-15 hours
3. Update documentation accuracy → 2 hours

**Total Critical Path:** 15-20 hours to unblock integration

---

## Priority Matrix

| Priority | Issue | Severity | Impact | Effort | Blocks |
|----------|-------|----------|--------|--------|--------|
| **P0** | No SQL triggers | CRITICAL | Database integrity at risk | 3h | Production deployment |
| **P0** | 70 lora-worker errors | CRITICAL | Server-API + CLI blocked | 10-15h | All integration work |
| **P0** | Doc inaccuracies | HIGH | Misleading claims | 2h | Credibility |
| **P1** | 465 TypeScript errors | HIGH | UI blocked | 3-4h | UI integration |
| **P1** | No TS build config | HIGH | Cannot build UI | 1h | UI development |
| **P1** | Tests can't run | HIGH | Cannot verify claims | 2h | Test validation |
| **P1** | No CLAUDE.md docs | MEDIUM | Unusable API types | 2-3h | Developer adoption |
| **P2** | Server-API integration | MEDIUM | No API endpoints | 2-3h | End-to-end |
| **P2** | CLI integration | MEDIUM | No CLI commands | 1-2h | End-to-end |
| **P2** | UI integration | MEDIUM | No UI components | 1-2h | End-to-end |
| **P3** | LOC count errors | LOW | Metrics incorrect | 30m | Documentation |
| **P3** | File path errors | LOW | Copy-paste fails | 15m | Documentation |

---

## Phase 1: Critical Database Fixes (3-4 hours)

### Task 1.1: Create Migration 0075 - State Transition Triggers

**File:** `/Users/star/Dev/aos/migrations/0075_lifecycle_state_transition_triggers.sql`

**Content:**
```sql
-- Migration 0075: SQL Trigger Enforcement for Lifecycle State Transitions
-- PRD-02 Critical Gap Fix
-- Ensures database-level enforcement of state machine rules

-- ============================================================================
-- ADAPTER LIFECYCLE STATE TRANSITION ENFORCEMENT
-- ============================================================================

CREATE TRIGGER IF NOT EXISTS enforce_adapter_lifecycle_transitions
BEFORE UPDATE OF lifecycle_state ON adapters
FOR EACH ROW
WHEN OLD.lifecycle_state != NEW.lifecycle_state
BEGIN
    -- Rule 1: Retired is a terminal state (cannot transition out)
    SELECT CASE
        WHEN OLD.lifecycle_state = 'retired'
        THEN RAISE(ABORT, 'Cannot transition from retired state (terminal)')
    END;

    -- Rule 2: Ephemeral tier adapters cannot be deprecated
    SELECT CASE
        WHEN NEW.tier = 'ephemeral' AND NEW.lifecycle_state = 'deprecated'
        THEN RAISE(ABORT, 'Ephemeral adapters cannot be deprecated; must transition directly to retired')
    END;

    -- Rule 3: No backward transitions (state machine is forward-only)
    SELECT CASE
        -- active cannot regress to draft
        WHEN OLD.lifecycle_state = 'active' AND NEW.lifecycle_state = 'draft'
        THEN RAISE(ABORT, 'Invalid backward transition: active -> draft')

        -- deprecated cannot regress to active or draft
        WHEN OLD.lifecycle_state = 'deprecated' AND NEW.lifecycle_state IN ('draft', 'active')
        THEN RAISE(ABORT, 'Invalid backward transition: deprecated -> active/draft')

        -- retired cannot regress (already covered by Rule 1, but explicit here)
        WHEN OLD.lifecycle_state = 'retired'
        THEN RAISE(ABORT, 'Invalid transition from retired (terminal state)')
    END;
END;

-- ============================================================================
-- ADAPTER STACK LIFECYCLE STATE TRANSITION ENFORCEMENT
-- ============================================================================

CREATE TRIGGER IF NOT EXISTS enforce_stack_lifecycle_transitions
BEFORE UPDATE OF lifecycle_state ON adapter_stacks
FOR EACH ROW
WHEN OLD.lifecycle_state != NEW.lifecycle_state
BEGIN
    -- Same rules as adapters (stacks follow same state machine)

    SELECT CASE
        WHEN OLD.lifecycle_state = 'retired'
        THEN RAISE(ABORT, 'Cannot transition from retired state (terminal)')
    END;

    SELECT CASE
        WHEN OLD.lifecycle_state = 'active' AND NEW.lifecycle_state = 'draft'
        THEN RAISE(ABORT, 'Invalid backward transition: active -> draft')

        WHEN OLD.lifecycle_state = 'deprecated' AND NEW.lifecycle_state IN ('draft', 'active')
        THEN RAISE(ABORT, 'Invalid backward transition: deprecated -> active/draft')

        WHEN OLD.lifecycle_state = 'retired'
        THEN RAISE(ABORT, 'Invalid transition from retired (terminal state)')
    END;
END;

-- ============================================================================
-- VERSION FORMAT VALIDATION (Semantic Versioning or Monotonic)
-- ============================================================================

-- Note: Version format is currently validated in application layer only
-- (adapteros-db/src/metadata.rs:303-318: validate_version function)
--
-- Adding database-level validation would require CHECK constraints:
-- CHECK (version GLOB '[0-9]*.[0-9]*.[0-9]*' OR CAST(version AS INTEGER) IS NOT NULL)
--
-- However, SQLite CHECK constraints are limited and may not cover all edge cases.
-- Recommend keeping version validation in application layer for flexibility.

-- ============================================================================
-- INDEXES FOR PERFORMANCE (query lifecycle states efficiently)
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_adapters_lifecycle_state
    ON adapters(lifecycle_state);

CREATE INDEX IF NOT EXISTS idx_adapter_stacks_lifecycle_state
    ON adapter_stacks(lifecycle_state);

-- ============================================================================
-- MIGRATION METADATA
-- ============================================================================

-- This migration addresses PRD-02 critical gap: database-level enforcement
-- of lifecycle state transition rules documented in docs/VERSION_GUARANTEES.md
--
-- References:
-- - docs/VERSION_GUARANTEES.md (state machine specification)
-- - adapteros-core/src/lifecycle.rs (application-layer validation)
-- - adapteros-db/src/metadata.rs (validate_state_transition function)
--
-- Testing: See tests/database/test_lifecycle_triggers.rs
```

**Estimated Time:** 1 hour (write + test)

---

### Task 1.2: Add Trigger Validation Tests

**File:** `/Users/star/Dev/aos/crates/adapteros-db/tests/lifecycle_trigger_tests.rs`

**Test Cases:**
1. `test_trigger_blocks_retired_to_active()` - Verify retired → active fails
2. `test_trigger_blocks_ephemeral_to_deprecated()` - Verify ephemeral tier cannot be deprecated
3. `test_trigger_blocks_backward_transitions()` - Verify active → draft fails
4. `test_trigger_allows_valid_transitions()` - Verify draft → active → deprecated → retired succeeds
5. `test_trigger_allows_same_state_updates()` - Verify same-state updates (no-op) succeed
6. `test_stack_trigger_enforcement()` - Verify stack triggers work identically

**Estimated Time:** 1 hour (write + debug)

---

### Task 1.3: Update Schema Consistency Tests

**File:** `/Users/star/Dev/aos/crates/adapteros-db/tests/schema_consistency_tests.rs`

**Changes:**
```rust
// Line 62-111: Add to required_fields list
let required_fields = vec![
    // ... existing fields ...

    // PRD-02 metadata normalization (migration 0068)
    "version",              // ADD THIS
    "lifecycle_state",      // ADD THIS

    // Timestamps
    "created_at",
    "updated_at",
];
```

**Estimated Time:** 30 minutes (add + verify)

---

### Task 1.4: Fix Migration Number Comment

**File:** `/Users/star/Dev/aos/crates/adapteros-db/src/adapters.rs`

**Change:**
```rust
// Line 357
// OLD:
// Metadata normalization (from migration 0070)
pub version: String,
pub lifecycle_state: String,

// NEW:
// Metadata normalization (from migration 0068)
pub version: String,
pub lifecycle_state: String,
```

**Estimated Time:** 5 minutes

---

### Task 1.5: Fix WorkflowType::from_str Case Sensitivity

**File:** `/Users/star/Dev/aos/crates/adapteros-db/src/metadata.rs`

**Change:**
```rust
// Lines 171-177
// OLD:
impl WorkflowType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Parallel" => Some(Self::Parallel),
            // ...
        }
    }
}

// NEW:
impl WorkflowType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "parallel" => Some(Self::Parallel),
            "upstream_downstream" => Some(Self::UpstreamDownstream),
            "sequential" => Some(Self::Sequential),
            _ => None,
        }
    }
}
```

**Estimated Time:** 15 minutes

---

**Phase 1 Total:** 3-4 hours

**Outcome:** Database layer 100% production-ready with SQL-enforced integrity

---

## Phase 2: Lora-Worker Compilation Fixes (10-15 hours)

### Overview

**Current Status:** 70 compilation errors blocking server-api and CLI
**Root Cause:** Nov 18 comprehensive patching introduced async refactoring bugs
**Strategy:** Fix errors by category (imports → types → methods → lifetime)

---

### Task 2.1: Fix Import Errors (1 hour)

**20 errors:** Unresolved imports

**File:** `/Users/star/Dev/aos/crates/adapteros-lora-worker/Cargo.toml`

**Add missing dependencies:**
```toml
[dependencies]
tokio = { version = "1.35", features = ["full", "sync"] }  # ADD: tokio::watch
libc = "0.2"                                                # ADD: libc APIs
mach = "0.3"                                                # ADD: mach APIs (macOS)
```

**Files to fix:**
- `src/lib.rs:43` - Add `use tokio::sync::watch;`
- `src/memory.rs:86` - Add `use mach::host_info;`
- `src/memory.rs:88` - Add `use mach::vm_statistics64;`

**Verification:** `cargo check -p adapteros-lora-worker` should show ~50 remaining errors

---

### Task 2.2: Define Missing Types (2-3 hours)

**11 errors:** Type not found

**Missing types:**
1. `StackHandle` - Appears in `adapter_hotswap.rs:732`
2. `UmaStats` - Appears in memory monitoring code
3. `KernelAdapterBackend` - Import from adapteros-lora-lifecycle

**Solutions:**

**2.2.1: Fix KernelAdapterBackend Import**
```rust
// src/lib.rs
use adapteros_lora_lifecycle::KernelAdapterBackend;  // Check if this type exists
```

**If type doesn't exist in lifecycle crate:**
- Check git history: `git log --all --grep="KernelAdapterBackend"`
- Restore from previous commit or redefine

**2.2.2: Define or Restore StackHandle**
```rust
// Check if StackHandle was deleted in commit c0386731:
git show c0386731:crates/adapteros-lora-worker/src/adapter_hotswap.rs | grep -A 10 "struct StackHandle"

// If found, restore definition
// If not found, check if it should be Arc<Stack> or similar
```

**2.2.3: Define or Restore UmaStats**
```rust
// Check memory.rs history for UmaStats definition
git log --all -p -- crates/adapteros-lora-worker/src/memory.rs | grep -A 5 "UmaStats"
```

**Estimated Time:** 2-3 hours (research + restore/redefine)

---

### Task 2.3: Fix Method Signature Mismatches (2-3 hours)

**16 errors:** Method/function not found, wrong argument count

**Common patterns:**

**3.1: parse_vm_stat function missing**
```rust
// src/memory.rs:204
// Check if function was deleted in commit c0386731:
git show c0386731^:crates/adapteros-lora-worker/src/memory.rs | grep -A 20 "fn parse_vm_stat"

// Restore or reimplement based on git history
```

**3.2: Pointer method errors (as_ptr, as_mut_ptr)**
```rust
// src/kvcache.rs - Raw pointer method calls failing
// Likely caused by type inference issues after refactoring
// Review each error and fix pointer casts
```

**3.3: mach_host_self and vm_kernel_page_size missing**
```rust
// src/memory.rs
// These are mach crate functions, ensure proper imports:
use mach::mach_init::{mach_host_self, vm_kernel_page_size};
```

**Estimated Time:** 2-3 hours

---

### Task 2.4: Fix Lifetime/Borrowing Issues (1-2 hours)

**1 error:** Temporary value dropped while borrowed

**File:** `src/adapter_hotswap.rs:370`

**Issue:**
```rust
// BROKEN:
let mut ids: Vec<_> = self.active.read().keys().collect();
//                    ^^^^^^^^^^^^^^^^^ temporary value freed while still in use

// FIX Option 1: Extend lifetime with explicit binding
let active_guard = self.active.read();
let mut ids: Vec<_> = active_guard.keys().cloned().collect();

// FIX Option 2: Clone keys immediately
let mut ids: Vec<_> = self.active.read().keys().cloned().collect();
```

**Estimated Time:** 30 minutes - 1 hour

---

### Task 2.5: Fix Send Trait Violations (1 hour)

**2 errors:** MutexGuard cannot be sent between threads

**Pattern:**
```rust
// BROKEN:
let guard = self.stacks.lock().await;
tokio::spawn(async move {
    // guard used here - NOT ALLOWED (MutexGuard is not Send)
});

// FIX: Drop guard before spawning
let data = {
    let guard = self.stacks.lock().await;
    guard.clone()  // Or extract needed data
};  // guard dropped here

tokio::spawn(async move {
    // Use data instead of guard
});
```

**Files to review:**
- Search for `tokio::spawn` calls that capture MutexGuard
- Refactor to extract data before spawning

**Estimated Time:** 1 hour

---

### Task 2.6: Fix Miscellaneous Errors (1-2 hours)

**5 errors:** Various scope/syntax issues

1. `circuit_breaker` not in scope - Define or import
2. `self` in non-impl context - Fix function signature
3. Struct not in scope - Add use statement

**Estimated Time:** 1-2 hours

---

**Phase 2 Total:** 10-15 hours

**Outcome:** Lora-worker compiles, unblocking server-api and CLI integration

---

## Phase 3: Documentation Accuracy Updates (2 hours)

### Task 3.1: Fix Migration Number References

**Files to update:**
- `docs/PRD-02-COMPLETION-GUIDE.md` (lines 11, 312, 375)
- Any other docs referencing "migration 0070" for metadata normalization

**Find/Replace:**
```bash
# Search pattern
grep -r "migration 0070" docs/ crates/adapteros-db/

# Replace 0070 → 0068 where context is "metadata normalization"
# Keep 0070 where context is "routing decisions"
```

**Estimated Time:** 30 minutes

---

### Task 3.2: Fix File Path Base Directories

**File:** `docs/PRD-02-COMPLETION-GUIDE.md`

**Find/Replace:**
```bash
# 9 instances to fix
sed -i '' 's|/home/user/adapter-os|/Users/star/Dev/aos|g' docs/PRD-02-COMPLETION-GUIDE.md
```

**Estimated Time:** 15 minutes

---

### Task 3.3: Update LOC Counts

**Files:**
- `PRD-02_INDEX.md` - Update per-file LOC counts
- `docs/VERSION_GUARANTEES.md` - Update claimed LOC (850 → 248)
- `PRD-02_COMPLETION_REPORT.md` - Update total (1,405 → 706)

**Estimated Time:** 30 minutes

---

### Task 3.4: Update PRD-02-BLOCKERS.md for Accuracy

**Changes:**
```markdown
# OLD:
- "51+ lora-worker errors"
- "Metal shader build failures"
- "CLI blocked by Metal"
- "Pre-existing build issues"

# NEW:
- "70 lora-worker errors" (actual count)
- "Metal kernels build successfully" (corrected)
- "CLI blocked by lora-worker dependency" (accurate)
- "Errors introduced Nov 18 during concurrent work" (timeline clarified)
```

**Estimated Time:** 30 minutes

---

**Phase 3 Total:** 2 hours

**Outcome:** Documentation 100% accurate, no misleading claims

---

## Phase 4: Type System Fixes (6-8 hours)

### Task 4.1: Add CLAUDE.md API Types Documentation (2-3 hours)

**File:** `/Users/star/Dev/aos/CLAUDE.md`

**Add new section after "Architecture Patterns":**

```markdown
## API Types (adapteros-api-types)

### Overview
**Location:** `crates/adapteros-api-types/`
**Purpose:** Centralized type definitions for all API requests and responses
**Schema Version:** 1.0 (includes forward/backward compatibility guarantees)

### Quick Start

All API response types automatically include schema versioning:

```rust
use adapteros_api_types::{AdapterResponse, schema_version};

let response = AdapterResponse {
    schema_version: schema_version(), // Returns "1.0"
    id: "adapter-123".to_string(),
    name: "code-review-adapter".to_string(),
    // ... other fields
};
```

### Available Types

**15 Modules:**
- `adapters` - Adapter request/response types
- `auth` - Authentication (Login, UserInfo)
- `dashboard` - Dashboard configuration
- `domain_adapters` - Domain-specific adapters
- `git` - Git integration types
- `inference` - Inference requests/responses
- `lib` - Common types (ErrorResponse, HealthResponse, PaginatedResponse)
- `metrics` - System and adapter metrics
- `nodes` - Distributed node management
- `plans` - Plan execution types
- `repositories` - Repository scanning
- `telemetry` - Telemetry events
- `tenants` - Multi-tenancy
- `training` - Training jobs and datasets
- `workers` - Worker node status

### Schema Version Policy

**Current Version:** 1.0

**Compatibility Guarantees:**
- **Minor version changes** (1.0 → 1.1): Backward compatible (add optional fields)
- **Major version changes** (1.x → 2.0): Breaking changes (require migration)

See [docs/VERSION_GUARANTEES.md](docs/VERSION_GUARANTEES.md) for full policy.

### Common Patterns

**Error Responses:**
```rust
use adapteros_api_types::ErrorResponse;

ErrorResponse::new("NotFound", "Adapter not found")
    .with_code("ADAPTER_NOT_FOUND")
    .with_details(json!({"adapter_id": "missing-123"}));
```

**Paginated Responses:**
```rust
use adapteros_api_types::PaginatedResponse;

PaginatedResponse {
    schema_version: schema_version(),
    items: adapters,
    total: 100,
    page: 1,
    page_size: 20,
}
```

### OpenAPI Integration

All types derive `ToSchema` for automatic OpenAPI generation:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterResponse {
    // Utoipa automatically generates OpenAPI schema
}
```

**Generate OpenAPI spec:**
```bash
cargo run --bin export-openapi > openapi.json
```

### TypeScript Integration

TypeScript types are automatically generated from Rust types.

**Location:** `ui/src/api/types.ts`

**Usage:**
```typescript
import { AdapterResponse } from '@/api/types';

const adapter: AdapterResponse = await apiClient.get('/api/adapters/123');
console.log(adapter.schema_version); // "1.0"
```

### Testing

Type validation tests ensure compatibility:

```bash
cargo test type_validation
# 36 tests: OpenAPI compat, frontend compat, round-trip serialization
```

### Migration Guide

**From old types:**
```rust
// OLD (direct struct definitions)
pub struct AdapterResponse {
    pub id: String,
    // ...
}

// NEW (use centralized types)
use adapteros_api_types::AdapterResponse;
```

**Adding new response types:**
1. Add to appropriate module in `crates/adapteros-api-types/src/`
2. Include `#[serde(default = "schema_version")]` field
3. Derive `Serialize`, `Deserialize`, `ToSchema`
4. Add to module exports in `lib.rs`
5. Write type validation tests

### References

- **VERSION_GUARANTEES.md** - Versioning policy
- **PRD-02-COMPLETION-GUIDE.md** - Implementation details
- **adapteros-api-types/src/lib.rs** - Central exports
```

**Estimated Time:** 2-3 hours

---

### Task 4.2: Refactor Type Validation Tests (2 hours)

**File:** `/Users/star/Dev/aos/tests/type_validation/round_trip.rs`

**Issue:** Tests import from `adapteros-server-api` which is broken

**Solution:** Refactor to use only `adapteros-api-types`

**Changes:**
```rust
// OLD:
use adapteros_server_api::types::*;

// NEW:
use adapteros_api_types::*;

// Remove tests for server-api-specific types that aren't in api-types:
// - InferResponse (server-api specific)
// - BatchInferRequest/Response (server-api specific)
// - RouterDecision (server-api specific)

// Keep tests for api-types types:
// - AdapterResponse
// - ErrorResponse
// - HealthResponse
// - PaginatedResponse
// - All 15 module types
```

**Estimated Time:** 2 hours

---

### Task 4.3: Add Schema Version Test Assertions (1 hour)

**File:** `/Users/star/Dev/aos/tests/type_validation/round_trip.rs`

**Add to each test:**
```rust
// Example for test_adapter_response_round_trip:
#[tokio::test]
async fn test_adapter_response_round_trip() {
    let original = AdapterResponse { /* ... */ };
    let json = serde_json::to_string(&original).unwrap();
    let deserialized: AdapterResponse = serde_json::from_str(&json).unwrap();

    // EXISTING ASSERTIONS
    assert_eq!(original.id, deserialized.id);
    // ...

    // NEW ASSERTIONS (add these)
    assert_eq!(deserialized.schema_version, "1.0");
    let json_obj: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(json_obj.as_object().unwrap().contains_key("schema_version"));
}
```

**Estimated Time:** 1 hour (add assertions to 21 tests)

---

### Task 4.4: Run and Verify Tests (1 hour)

```bash
cargo test type_validation --lib -- --nocapture
# Should show 36 tests passing
# Capture output for documentation
```

**Estimated Time:** 1 hour (includes fixing any test failures)

---

**Phase 4 Total:** 6-8 hours

**Outcome:** Type system 100% documented and tested

---

## Phase 5: UI Fixes (5-7 hours)

### Task 5.1: Fix TypeScript Syntax Errors (3-4 hours)

**Strategy:** Bulk fix duplicate try-catch blocks

**Script:** Create `/Users/star/Dev/aos/fix-ui-syntax.sh`

```bash
#!/bin/bash
# Fix duplicate try-catch blocks in UI components

FILES=(
  "ui/src/components/Adapters.tsx"
  "ui/src/components/InferencePlayground.tsx"
  "ui/src/components/CodeIntelligenceTraining.tsx"
  "ui/src/components/ModelSelector.tsx"
  "ui/src/components/ProcessDebugger.tsx"
  "ui/src/layout/RootLayout.tsx"
  "ui/src/layout/FeatureLayout.tsx"
  "ui/src/data/role-guidance.ts"
  "ui/src/utils/logger.ts"
  "ui/src/components/TrainingWizard.tsx"
  "ui/src/components/TraceVisualizer.tsx"
  "ui/src/components/SpawnWorkerModal.tsx"
  "ui/src/components/AlertsPage.tsx"
  "ui/src/components/Policies.tsx"
  "ui/src/hooks/useActivityFeed.ts"
  "ui/src/components/AdapterMemoryMonitor.tsx"
)

for FILE in "${FILES[@]}"; do
  echo "Fixing $FILE..."
  # Manual fix required - pattern is complex and varies per file
  # Open each file and:
  # 1. Find duplicate catch blocks
  # 2. Merge into single catch block
  # 3. Preserve logger.error calls
  # 4. Remove old console.error calls
done
```

**Manual process per file:**
1. Search for pattern: `} catch (err) {` followed by `} catch (error) {`
2. Keep second catch block (has proper logger calls)
3. Ensure state updates (setError, setLoading) are preserved
4. Remove duplicate error handling

**Estimated Time:** 3-4 hours (14 files × 15-20 minutes each)

---

### Task 5.2: Add TypeScript Build Configuration (1 hour)

**Task 5.2.1: Create tsconfig.json**

**File:** `/Users/star/Dev/aos/ui/tsconfig.json`

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,

    /* Bundler mode */
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",

    /* Linting */
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,

    /* Paths */
    "baseUrl": ".",
    "paths": {
      "@/*": ["src/*"]
    }
  },
  "include": ["src"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

**Task 5.2.2: Create vite.config.ts**

**File:** `/Users/star/Dev/aos/ui/vite.config.ts`

```typescript
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    port: 3000,
    proxy: {
      '/api': 'http://localhost:8080',
    },
  },
})
```

**Task 5.2.3: Update package.json scripts**

**File:** `/Users/star/Dev/aos/ui/package.json`

```json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "type-check": "tsc --noEmit"
  }
}
```

**Estimated Time:** 1 hour

---

### Task 5.3: Verify UI Builds (30 minutes)

```bash
cd ui/
pnpm install
pnpm type-check  # Should show 0 errors after fixes
pnpm build       # Should build successfully
```

**Estimated Time:** 30 minutes

---

**Phase 5 Total:** 5-7 hours

**Outcome:** UI builds successfully, syntax errors eliminated

---

## Phase 6: Integration (5-8 hours)

### Task 6.1: Server API Integration (2-3 hours)

**Follow:** `docs/PRD-02-COMPLETION-GUIDE.md` Phase 2

**Key files:**
- `crates/adapteros-server-api/src/handlers.rs`
- `crates/adapteros-server-api/src/routes.rs`

**Changes:**
1. Import `AdapterMeta` and `AdapterStackMeta` from `adapteros-db`
2. Update handler functions to use metadata structs
3. Add `schema_version` to all responses
4. Test with `curl` or Postman

**Verification:**
```bash
cargo build -p adapteros-server-api
# Should compile after lora-worker is fixed

cargo run --bin adapteros-server &
curl http://localhost:8080/api/adapters | jq '.schema_version'
# Should return "1.0"
```

**Estimated Time:** 2-3 hours

---

### Task 6.2: CLI Integration (1-2 hours)

**Follow:** `docs/PRD-02-COMPLETION-GUIDE.md` Phase 3

**Key file:**
- `crates/adapteros-cli/src/commands/adapter.rs`

**Changes:**
1. Update `list` command to display version and lifecycle_state
2. Add new commands: `lifecycle set`, `lifecycle show`
3. Update help text

**Verification:**
```bash
cargo build -p adapteros-cli
# Should compile after lora-worker is fixed

./target/release/aosctl adapter list
# Should show Version and Lifecycle columns
```

**Estimated Time:** 1-2 hours

---

### Task 6.3: UI Integration (1-2 hours)

**Follow:** `docs/PRD-02-COMPLETION-GUIDE.md` Phase 4

**Key files:**
- `ui/src/components/Adapters.tsx`
- `ui/src/components/AdapterLifecycleManager.tsx`

**Changes:**
1. Add Version column to adapter tables
2. Add Lifecycle State badges
3. Implement color coding (draft=outline, active=default, deprecated=secondary, retired=destructive)

**Verification:**
```bash
cd ui/
pnpm dev
# Navigate to http://localhost:3000/adapters
# Verify version and lifecycle columns appear
```

**Estimated Time:** 1-2 hours

---

### Task 6.4: End-to-End Testing (1-2 hours)

**Test scenarios:**

1. **Create adapter with version/lifecycle:**
   ```bash
   aosctl adapter register --id test-001 --version 1.0.0 --lifecycle draft
   ```

2. **Transition lifecycle states:**
   ```bash
   aosctl adapter lifecycle set test-001 active
   aosctl adapter lifecycle set test-001 deprecated
   aosctl adapter lifecycle set test-001 retired
   ```

3. **Verify trigger enforcement:**
   ```bash
   # Should FAIL (retired is terminal)
   aosctl adapter lifecycle set test-001 active
   ```

4. **Verify API responses:**
   ```bash
   curl http://localhost:8080/api/adapters/test-001 | jq
   # Should show schema_version, version, lifecycle_state
   ```

5. **Verify UI displays:**
   - Check adapter table shows Version and Lifecycle columns
   - Check lifecycle badges are color-coded correctly

**Estimated Time:** 1-2 hours

---

**Phase 6 Total:** 5-8 hours

**Outcome:** Full stack integration complete, end-to-end functionality verified

---

## Phase 7: Polish (2-3 hours)

### Task 7.1: Add Test Infrastructure (2 hours)

**UI Testing:**

**File:** `/Users/star/Dev/aos/ui/vite.config.ts`

Add Vitest configuration:

```typescript
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './src/test/setup.ts',
  },
  // ... rest of config
})
```

**File:** `/Users/star/Dev/aos/ui/src/test/setup.ts`

```typescript
import { expect, afterEach } from 'vitest'
import { cleanup } from '@testing-library/react'
import '@testing-library/jest-dom'

afterEach(() => {
  cleanup()
})
```

**Sample test:**

**File:** `/Users/star/Dev/aos/ui/src/components/Adapters.test.tsx`

```typescript
import { render, screen } from '@testing-library/react'
import { describe, it, expect } from 'vitest'
import Adapters from './Adapters'

describe('Adapters Component', () => {
  it('renders version column', () => {
    render(<Adapters />)
    expect(screen.getByText('Version')).toBeInTheDocument()
  })

  it('renders lifecycle column', () => {
    render(<Adapters />)
    expect(screen.getByText('Lifecycle')).toBeInTheDocument()
  })
})
```

**Estimated Time:** 2 hours

---

### Task 7.2: Documentation Cleanup (1 hour)

**Finalize documentation:**

1. Update PRD-02_INDEX.md with final completion %
2. Update PRD-02_COMPLETION_REPORT.md status
3. Add "100% Complete" badge to README.md
4. Update ARCHITECTURE_INDEX.md with PRD-02 references

**Estimated Time:** 1 hour

---

**Phase 7 Total:** 2-3 hours

**Outcome:** Test infrastructure in place, documentation finalized

---

## Summary Timeline

| Phase | Tasks | Estimated Time | Cumulative |
|-------|-------|----------------|------------|
| **Phase 1** | Database fixes (triggers + tests) | 3-4 hours | 3-4 hours |
| **Phase 2** | Lora-worker compilation (70 errors) | 10-15 hours | 13-19 hours |
| **Phase 3** | Documentation accuracy | 2 hours | 15-21 hours |
| **Phase 4** | Type system (tests + docs) | 6-8 hours | 21-29 hours |
| **Phase 5** | UI fixes (syntax + build config) | 5-7 hours | 26-36 hours |
| **Phase 6** | Integration (server + CLI + UI) | 5-8 hours | 31-44 hours |
| **Phase 7** | Polish (tests + docs) | 2-3 hours | 33-47 hours |
| **TOTAL** | | **33-47 hours** | **5-6 work days** |

---

## Progress Tracking

**Use this checklist to track completion:**

### Phase 1: Database ✅❌
- [ ] Migration 0075 created
- [ ] Trigger validation tests added (6 tests)
- [ ] Schema consistency tests updated
- [ ] Migration number comment fixed
- [ ] WorkflowType::from_str case fixed
- [ ] All database tests pass (18 total)

### Phase 2: Lora-Worker ✅❌
- [ ] Import errors fixed (20)
- [ ] Missing types defined (11)
- [ ] Method signatures fixed (16)
- [ ] Lifetime issues fixed (1)
- [ ] Send trait violations fixed (2)
- [ ] Miscellaneous errors fixed (5)
- [ ] Cargo build succeeds (0 errors)

### Phase 3: Documentation ✅❌
- [ ] Migration number references fixed (0070 → 0068)
- [ ] File paths updated (9 instances)
- [ ] LOC counts updated
- [ ] PRD-02-BLOCKERS.md accuracy updated
- [ ] All documentation passes peer review

### Phase 4: Type System ✅❌
- [ ] CLAUDE.md API types section added
- [ ] Type validation tests refactored
- [ ] Schema version assertions added (21 tests)
- [ ] All type tests pass (36 tests)

### Phase 5: UI ✅❌
- [ ] TypeScript syntax errors fixed (14 files)
- [ ] tsconfig.json created
- [ ] vite.config.ts created
- [ ] package.json scripts added
- [ ] pnpm type-check passes (0 errors)
- [ ] pnpm build succeeds

### Phase 6: Integration ✅❌
- [ ] Server API handlers updated
- [ ] CLI commands updated
- [ ] UI components updated
- [ ] End-to-end test scenarios pass (5)

### Phase 7: Polish ✅❌
- [ ] Vitest configuration added
- [ ] Sample UI tests written
- [ ] Documentation finalized
- [ ] README updated with 100% badge

---

## Risk Management

### High-Risk Areas

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Lora-worker fixes introduce new bugs | MEDIUM | HIGH | Incremental testing after each fix category |
| Type tests still fail after refactor | MEDIUM | MEDIUM | Keep server-api types in separate test file |
| UI syntax fixes break logic | LOW | MEDIUM | Manual code review, preserve state updates |
| Database triggers have edge cases | LOW | HIGH | Comprehensive test coverage (6+ tests) |
| Integration reveals new issues | MEDIUM | MEDIUM | Test each component independently first |

### Rollback Plan

**If critical issues arise:**

1. **Database:** Revert migration 0075 via `rollback.sql`
2. **Lora-worker:** Revert to commit before fixes (git revert)
3. **UI:** Revert syntax fixes (git revert)
4. **Integration:** Deploy database/API layers only, defer presentation

---

## Success Criteria

**PRD-02 is 100% complete when:**

1. ✅ Database layer has SQL trigger enforcement
2. ✅ All 70 lora-worker errors resolved
3. ✅ Server-API and CLI compile and run
4. ✅ All 465 TypeScript errors resolved
5. ✅ UI builds successfully with pnpm build
6. ✅ All 36 type validation tests pass
7. ✅ All 18 database tests pass
8. ✅ End-to-end test scenarios pass
9. ✅ Documentation is 100% accurate
10. ✅ CLAUDE.md includes API types usage guide

**Production deployment ready when:**

1. ✅ All above criteria met
2. ✅ Code reviewed by second developer
3. ✅ Smoke tests pass in staging environment
4. ✅ Performance benchmarks meet targets
5. ✅ Rollback plan tested

---

## Resources

**Key Documents:**
- PRD-02_VERIFICATION_REPORT.md (this verification)
- docs/PRD-02-COMPLETION-GUIDE.md (implementation steps)
- docs/VERSION_GUARANTEES.md (policy reference)
- docs/PRD-02-BLOCKERS.md (blocker details)

**Key Commands:**
```bash
# Database
cargo test -p adapteros-db

# API Types
cargo build -p adapteros-api-types
cargo test type_validation

# Lora-Worker
cargo build -p adapteros-lora-worker

# Server-API (after lora-worker fixed)
cargo build -p adapteros-server-api

# CLI (after lora-worker fixed)
cargo build -p adapteros-cli

# UI
cd ui && pnpm type-check && pnpm build
```

---

**Roadmap Created:** 2025-11-19
**Target Completion:** 2025-11-25 (5-6 work days)
**Owner:** Engineering Team
**Reviewer:** TBD

---

**END OF ROADMAP**
