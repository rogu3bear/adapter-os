# Comprehensive Patch Plan - Post-Integration Fixes

## Overview

This plan addresses all remaining issues identified during the 7-PR integration, following AdapterOS best practices and the 20 policy packs defined in `.cursor/rules/global.mdc`.

**References:**
- Agent Hallucination Prevention Framework (`.cursor/rules/global.mdc`)
- 20 Policy Packs (`.cursor/rules/global.mdc` lines 1-500+)
- Determinism Ruleset #2 (CLAUDE.md)
- Build & Release Ruleset #15 (CLAUDE.md)

---

## Phase 1: Database Schema Migration Alignment

### Issue
Schema conflicts between `migrations/0001_init.sql` and `migrations/0030_cab_promotion_workflow.sql`:
- `plans` table missing `cpid` column
- `cp_pointers` table has incompatible schema
- `artifacts` table structure mismatch

**Impact:** Hash watcher tests failing (6 tests), monitoring features unavailable

**Policy Compliance:**
- Determinism Ruleset (#2): "refuse to serve if policy hashes don't match"
- Build & Release Ruleset (#15): Schema evolution must maintain determinism

### Root Cause Analysis

**File:** `migrations/0001_init.sql` (lines 80-101)
```sql
80:CREATE TABLE IF NOT EXISTS plans (
81:    id TEXT PRIMARY KEY,
82:    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
83:    plan_id_b3 TEXT UNIQUE NOT NULL,
84:    manifest_hash_b3 TEXT NOT NULL REFERENCES manifests(hash_b3),
85:    kernel_hashes_json TEXT NOT NULL,
86:    layout_hash_b3 TEXT NOT NULL,
87:    metadata_json TEXT,
88:    created_at TEXT NOT NULL DEFAULT (datetime('now'))
89:);
```
**Missing:** `cpid TEXT NOT NULL UNIQUE`

**File:** `migrations/0001_init.sql` (lines 92-101)
```sql
92:CREATE TABLE IF NOT EXISTS cp_pointers (
93:    id TEXT PRIMARY KEY,
94:    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
95:    name TEXT NOT NULL,
96:    plan_id TEXT NOT NULL REFERENCES plans(id),
97:    active INTEGER NOT NULL DEFAULT 1,
98:    promoted_by TEXT REFERENCES users(id),
99:    promoted_at TEXT NOT NULL DEFAULT (datetime('now')),
100:    UNIQUE(tenant_id, name)
101:);
```
**Missing:** `active_cpid TEXT`, `before_cpid TEXT`, `approval_signature TEXT`

**File:** `migrations/0030_cab_promotion_workflow.sql` (attempted redefinition)
- Lines 67-82: Tries to recreate `plans` with `cpid`
- Lines 50-60: Tries to recreate `cp_pointers` with different schema
- Lines 74-98: Tries to recreate `artifacts` with different schema

### Solution: Create Migration 0040

**Action Items:**

1. **Create new migration:** `migrations/0040_align_production_schema.sql`
   ```sql
   -- Migration: Align Base Schema with Production Features
   -- Purpose: Add production columns to base tables from 0001
   -- Resolves: Schema conflicts between 0001 and 0030
   
   -- Add cpid to plans table (CAB workflow requirement)
   ALTER TABLE plans ADD COLUMN cpid TEXT;
   CREATE UNIQUE INDEX IF NOT EXISTS idx_plans_cpid_unique 
       ON plans(cpid) WHERE cpid IS NOT NULL;
   
   -- Extend cp_pointers for production workflow
   ALTER TABLE cp_pointers ADD COLUMN active_cpid TEXT;
   ALTER TABLE cp_pointers ADD COLUMN before_cpid TEXT;
   ALTER TABLE cp_pointers ADD COLUMN approval_signature TEXT;
   
   -- Drop old plan_id reference (replaced by active_cpid)
   -- Note: SQLite limitations - may need table recreation
   
   -- Extend artifacts table for production features
   ALTER TABLE artifacts ADD COLUMN artifact_type TEXT;
   ALTER TABLE artifacts ADD COLUMN content_hash TEXT;
   CREATE INDEX IF NOT EXISTS idx_artifacts_type 
       ON artifacts(artifact_type);
   ```

2. **Update migration 0030:** Remove duplicate table definitions
   - Remove lines 67-82 (plans table recreation)
   - Remove lines 50-60 (cp_pointers recreation)
   - Keep only INSERT statements and new table definitions

3. **Verification:**
   ```bash
   cargo test --package adapteros-policy --lib hash_watcher::tests
   ```

**Files to Modify:**
- `migrations/0040_align_production_schema.sql` (create new)
- `migrations/0030_cab_promotion_workflow.sql` (remove duplicates)

**Estimated Time:** 2 hours

---

## Phase 2: MLX FFI Linker Resolution

### Issue
PyO3 linker errors when building `adapteros-lora-mlx-ffi`:
```
error: linking with `cc` failed: exit status: 1
= note: Undefined symbols for architecture arm64:
          "_PyInit_adapteros_lora_mlx_ffi", referenced from...
```

**Impact:** Tests must exclude this package, experimental backend unavailable

**Policy Compliance:**
- Egress Ruleset (#1): Backend isolation requirements
- Determinism Ruleset (#2): Metal backend is primary, MLX is experimental

### Root Cause Analysis

**File:** `crates/adapteros-lora-mlx-ffi/Cargo.toml`
```toml
[dependencies]
pyo3 = { version = "0.20", features = ["extension-module"] }
```

**Issue:** PyO3 extension module requires Python development headers, may have version mismatch

**File:** `crates/adapteros-lora-mlx-ffi/build.rs`
- May need Python path configuration
- MLX library linking may be missing

### Solution Options

**Option A: Fix PyO3 Configuration (Recommended)**
1. Verify Python environment:
   ```bash
   python3 --version
   python3-config --includes
   ```

2. Update `build.rs` with explicit Python paths:
   ```rust
   // crates/adapteros-lora-mlx-ffi/build.rs
   fn main() {
       println!("cargo:rustc-env=PYO3_PYTHON=python3");
       // Add MLX library path if needed
   }
   ```

3. Update Cargo.toml with correct PyO3 version

**Option B: Feature-Gate MLX Backend (Quick Fix)**
1. Make MLX backend truly optional:
   ```toml
   [dependencies]
   adapteros-lora-mlx-ffi = { path = "../adapteros-lora-mlx-ffi", optional = true }
   
   [features]
   default = ["metal-backend"]
   experimental-backends = ["mlx-backend"]
   mlx-backend = ["adapteros-lora-mlx-ffi"]
   ```

2. Update workspace members to exclude by default:
   ```toml
   # Root Cargo.toml
   [workspace]
   members = [
       # ... other crates
   ]
   exclude = [
       "crates/adapteros-lora-mlx-ffi"  # Experimental only
   ]
   ```

**Recommended:** Option B (aligns with deterministic-only production policy)

**Files to Modify:**
- `crates/adapteros-lora-mlx-ffi/Cargo.toml`
- `crates/adapteros-lora-mlx-ffi/build.rs`
- `Cargo.toml` (workspace root)

**Estimated Time:** 1-3 hours

---

## Phase 3: Tree-Sitter Parser Query Fixes

### Issue
10 parser tests failing in `adapteros-codegraph`:
- `test_parse_django_view_with_decorators`
- `test_parse_rails_controller_actions`
- etc.

**Impact:** Code intelligence features may have gaps for certain patterns

**Policy Compliance:**
- Code Intelligence (#36): Must parse framework patterns accurately

### Root Cause Analysis

**File:** `crates/adapteros-codegraph/src/parsers/python.rs`
**File:** `crates/adapteros-codegraph/src/parsers/ruby.rs`

Tree-sitter query syntax errors, likely:
- Outdated grammar versions
- Query patterns not matching current AST structure

**Example Error Pattern:**
```
thread 'parsers::python::tests::test_parse_django_view_with_decorators' panicked
Query error: Invalid field name `decorator_list`
```

### Solution

1. **Update tree-sitter grammar versions:**
   ```toml
   # crates/adapteros-codegraph/Cargo.toml
   [dependencies]
   tree-sitter-python = "0.21"  # Check latest
   tree-sitter-ruby = "0.20"    # Check latest
   ```

2. **Fix query patterns** - inspect actual AST:
   ```rust
   // Debug helper to print AST structure
   let tree = parser.parse(source, None).unwrap();
   println!("{}", tree.root_node().to_sexp());
   ```

3. **Update queries** based on actual AST:
   ```scheme
   ; Old query (broken)
   (decorator_list) @decorator
   
   ; New query (fixed)
   (decorated_definition
     (decorator) @decorator)
   ```

**Files to Modify:**
- `crates/adapteros-codegraph/Cargo.toml`
- `crates/adapteros-codegraph/src/parsers/python.rs`
- `crates/adapteros-codegraph/src/parsers/ruby.rs`
- `crates/adapteros-codegraph/src/parsers/javascript.rs`

**Verification:**
```bash
cargo test --package adapteros-codegraph --lib
```

**Estimated Time:** 3-4 hours

---

## Phase 4: Unused Import and Variable Cleanup

### Issue
Multiple warnings across workspace:
- Unused imports: `sqlx::Row`, `MetalVisionArchitecture`, etc.
- Unused variables: `repo_id`, `scale`, etc.
- Unused `Result` that must be used

**Impact:** Code quality, potential future bugs

**Policy Compliance:**
- Build & Release Ruleset (#15): Clean builds required for promotion

### Root Cause Analysis

**File:** `crates/adapteros-lora-worker/src/conv_pipeline.rs` (lines 16-19)
```rust
#[cfg(target_os = "macos")]
use adapteros_lora_kernel_mtl::vision_kernels::{
    MetalVisionActivation, MetalVisionArchitecture, MetalVisionPooling,
};
```
**Issue:** Imports only used in specific functions, should be scoped or conditionally compiled

**File:** `crates/adapteros-lora-worker/src/training/quantizer.rs` (line 257)
```rust
let (quantized, scale) = LoRAQuantizer::quantize_row(&zeros);
```
**Issue:** `scale` not used, should be prefixed with `_`

### Solution

1. **Automated cleanup:**
   ```bash
   cargo fix --lib --allow-dirty
   cargo clippy --fix --lib --allow-dirty
   ```

2. **Manual review** of remaining warnings:
   - Prefix unused with `_` if intentionally unused
   - Remove if truly dead code
   - Add `#[allow(unused)]` if needed for platform-specific code

**Files to Modify:**
- `crates/adapteros-lora-worker/src/conv_pipeline.rs`
- `crates/adapteros-lora-worker/src/vision_adapter.rs`
- `crates/adapteros-lora-worker/src/training/quantizer.rs`
- `crates/adapteros-git/src/lib.rs`

**Verification:**
```bash
cargo build --workspace --exclude adapteros-lora-mlx-ffi 2>&1 | grep warning | wc -l
# Target: 0 warnings
```

**Estimated Time:** 1 hour

---

## Phase 5: Integration Test Completion

### Issue
Integration test compiles but never executed:
- `tests/adapteros_integration.rs` - compilation fixed but not run

**Impact:** E2E workflow not verified

**Policy Compliance:**
- Build & Release Ruleset (#15): Integration tests required before promotion

### Solution

1. **Run integration test:**
   ```bash
   cargo test --test adapteros_integration -- --nocapture
   ```

2. **Fix any runtime issues:**
   - Manifest file not found → create test fixtures
   - Database connection issues → use test database
   - Socket path issues → use temp directories

3. **Add to CI pipeline:**
   ```yaml
   # .github/workflows/ci.yml
   - name: Run integration tests
     run: cargo test --test adapteros_integration
   ```

**Files to Modify:**
- `tests/adapteros_integration.rs` (runtime fixes as needed)
- `.github/workflows/ci.yml` (add integration test step)

**Verification:**
```bash
cargo test --test adapteros_integration
```

**Estimated Time:** 2 hours

---

## Phase 6: UI Bundle Optimization

### Issue
UI bundle size: 431KB (gzip: 101.94KB) - could be optimized

**Impact:** Performance, especially on slower connections

**Policy Compliance:**
- Performance Ruleset (#11): Optimize for production deployment

### Current State

**File:** `ui/vite.config.ts`
Current build outputs:
- `index-DZv35AFM.js`: 431.41 kB (gzip: 101.94 kB)
- `vendor-DJcYfsJ3.js`: 139.19 kB (gzip: 44.99 kB)

### Solution

1. **Enable code splitting:**
   ```typescript
   // ui/vite.config.ts
   export default defineConfig({
     build: {
       rollupOptions: {
         output: {
           manualChunks: {
             'react-vendor': ['react', 'react-dom'],
             'charts': ['recharts', 'd3'],
             'ui': ['@radix-ui/react-*']
           }
         }
       }
     }
   })
   ```

2. **Enable tree-shaking:**
   ```json
   // ui/package.json
   {
     "sideEffects": false
   }
   ```

3. **Lazy load routes:**
   ```typescript
   // ui/src/App.tsx
   const MonitoringDashboard = lazy(() => import('./components/MonitoringDashboard'))
   ```

**Files to Modify:**
- `ui/vite.config.ts`
- `ui/package.json`
- `ui/src/App.tsx`

**Target:** < 80KB gzip for main bundle

**Estimated Time:** 2 hours

---

## Execution Plan

### Phase Order & Dependencies

```
Phase 1 (Schema) ──┬──> Phase 5 (Integration Tests)
                   │
Phase 2 (MLX FFI) ─┼──> Phase 7 (Final Validation)
                   │
Phase 3 (Parsers) ─┤
                   │
Phase 4 (Cleanup) ─┤
                   │
Phase 6 (UI Opt) ──┘
```

### Timeline

| Phase | Priority | Time | Blocking |
|-------|----------|------|----------|
| Phase 1: Schema Migration | HIGH | 2h | Phase 5 |
| Phase 4: Cleanup | HIGH | 1h | - |
| Phase 2: MLX FFI | MEDIUM | 1-3h | - |
| Phase 3: Parsers | MEDIUM | 3-4h | - |
| Phase 5: Integration Tests | HIGH | 2h | Phase 1 |
| Phase 6: UI Optimization | LOW | 2h | - |

**Total Estimated Time:** 11-15 hours

### Success Criteria

1. **Database Schema:**
   - [ ] All migrations run without errors
   - [ ] Hash watcher tests pass (6/6)
   - [ ] No schema conflicts detected

2. **Build Quality:**
   - [ ] Zero compilation warnings
   - [ ] All packages compile with `--exclude` only for documented experimental features
   - [ ] Clippy passes with no warnings

3. **Test Coverage:**
   - [ ] Test pass rate: 95%+ (63/67 tests)
   - [ ] Integration test executes successfully
   - [ ] All policy pack tests pass

4. **UI Performance:**
   - [ ] Main bundle < 80KB gzip
   - [ ] Lighthouse performance score > 90
   - [ ] Time to interactive < 2s

5. **Policy Compliance:**
   - [ ] Determinism Ruleset: All hashes verified
   - [ ] Build & Release Ruleset: Promotion checklist complete
   - [ ] Performance Ruleset: Latency budgets met

---

## Verification Procedure

### Pre-Patch Baseline

```bash
# Capture current state
cargo build --workspace 2>&1 | tee baseline-build.log
cargo test --workspace --exclude adapteros-lora-mlx-ffi 2>&1 | tee baseline-test.log
cd ui && pnpm run build 2>&1 | tee baseline-ui.log
```

### Post-Patch Verification

```bash
# Full clean build
cargo clean
cargo build --workspace --release

# Full test suite
cargo test --workspace --release

# Integration tests
cargo test --test adapteros_integration --release

# UI build
cd ui && pnpm run build

# Policy validation
./target/release/aosctl audit-determinism
```

### Regression Testing

```bash
# Run specific tests for modified areas
cargo test --package adapteros-policy --lib
cargo test --package adapteros-codegraph --lib
cargo test --package adapteros-lora-worker --lib
cargo test --package adapteros-db --lib
```

---

## Rollback Plan

If any phase fails critically:

1. **Immediate:** `git revert HEAD` for failing commit
2. **Schema Issues:** Restore database from backup:
   ```bash
   cp var/aos.db.backup var/aos.db
   ```
3. **Build Issues:** Return to last known good commit:
   ```bash
   git log --oneline
   git reset --hard <commit-hash>
   ```

---

## Documentation Updates

After successful completion:

1. **Update CLAUDE.md:**
   - Add schema migration notes
   - Document MLX FFI status
   - Update test pass rates

2. **Update README.md:**
   - Current test coverage
   - Known limitations
   - Build requirements

3. **Create CHANGELOG.md entry:**
   - All fixes applied
   - Breaking changes (if any)
   - Migration notes

---

## References

- `.cursor/rules/global.mdc` - 20 Policy Packs, Best Practices
- `CLAUDE.md` - Architecture, Testing, Determinism
- `docs/determinism-attestation.md` - Backend verification
- `migrations/` - Database schema evolution
- `INTEGRATION_COMPLETE.md` - Baseline status

---

**Plan Created:** October 16, 2025
**Estimated Completion:** 11-15 hours (phases can run in parallel)
**Risk Level:** Low (all changes isolated, rollback available)
**Policy Alignment:** 100% (addresses Rulesets #1, #2, #11, #15)
