# Phase 2 Integration Plan - Complete with Citations

**Date**: 2025-10-17  
**Status**: READY FOR EXECUTION  
**Phase**: 2 of 2  
**PRs to Integrate**: 6 branches (4 clean, 1 needs review, 1 needs fixes)

---

## Executive Summary

Integrate 6 Phase 2 PRs following the same standards that made Phase 1 successful:
- Incremental integration with verification at each step
- Compilation checks before merge
- Standards compliance per CLAUDE.md
- Evidence-based approach with full audit trail
- Anti-hallucination framework enforcement

**Expected Outcome**: +1,750 lines of production-ready functionality

---

## Standards & Best Practices (Citations)

### CLAUDE.md Compliance

**Build & Test Requirements** (CLAUDE.md L78-85):
```bash
cargo build --release          # Build all crates
cargo test --workspace         # Run all tests
cargo clippy --workspace       # Lint checks
cargo fmt --all               # Format verification
```

**Integration Guidelines** (CLAUDE.md L90-97):
- Check for duplicates before implementing
- Implement enforcement in appropriate crate
- Add telemetry for violations
- Update compliance matrix if needed

**Anti-Hallucination Framework** (.cursor/rules/global.mdc L1-50):
- Pre-implementation duplicate checks using `codebase_search` and `grep`
- Post-operation verification: re-read files, grep confirmation, compilation
- Evidence-based claims with file paths and line numbers
- No duplicate implementations across crates

---

## Phase 2 Branch Inventory

### Branch 1: Batch Inference API ✅
**Branch**: `auto/add-batch-inference-api-endpoint`  
**Prompt Match**: CODEX_PROMPTS_PHASE2.md L201-247 (Prompt 7)  
**Changes**: 5 files, +573 lines, 0 deletions  
**Files Modified**:
- `crates/adapteros-server-api/src/handlers/batch.rs` (NEW)
- `crates/adapteros-server-api/src/types.rs` (+37 lines)
- `crates/adapteros-server-api/tests/batch_infer.rs` (NEW, 312 lines)

**Assessment**: ✅ Clean additive implementation with comprehensive tests  
**Risk Level**: LOW - New module, no conflicts  
**Standards Compliance**: 
- ✅ Under 500 lines per CODEX_PROMPTS_PHASE2.md L11
- ✅ Includes tests per CLAUDE.md L82
- ✅ Additive only per Phase2 anti-pattern L430

---

### Branch 2: Enhanced Error Context ✅
**Branch**: `auto/add-enhanced-error-context-in-codebase`  
**Prompt Match**: CODEX_PROMPTS_PHASE2.md L161-199 (Prompt 5)  
**Changes**: 5 files, +295 lines, -17 lines (net +278)  
**Files Modified**:
- `crates/adapteros-core/src/error.rs` (enhanced)
- `crates/adapteros-db/src/postgres.rs` (+22 lines)
- `crates/adapteros-db/src/postgres/adapters.rs` (+20 lines)

**Assessment**: ✅ Minimal changes to existing code, proper error enhancement  
**Risk Level**: LOW - Core infrastructure improvement  
**Standards Compliance**:
- ✅ Follows anyhow/eyre patterns per CODEX_PROMPTS_PHASE2.md L176
- ✅ Zero-cost when not triggered per L174
- ✅ Extends existing AosError per L168

---

### Branch 3: Production Monitoring Telemetry ✅
**Branch**: `auto/add-production-monitoring-telemetry-module`  
**Prompt Match**: CODEX_PROMPTS_PHASE2.md L87-133 (Prompt 3)  
**Changes**: 2 files, +401 lines, 0 deletions  
**Files Modified**:
- `crates/adapteros-telemetry/src/monitoring.rs` (NEW, 395 lines)
- `crates/adapteros-telemetry/src/lib.rs` (+6 lines export)

**Assessment**: ✅ New module, no modifications to existing code  
**Risk Level**: LOW - Additive only  
**Standards Compliance**:
- ✅ Uses canonical JSON per CLAUDE.md L161 (Telemetry Ruleset)
- ✅ Integrates with TelemetryWriter per CODEX_PROMPTS_PHASE2.md L100
- ✅ Additive only per L108

**Policy Pack Alignment**:
- Telemetry Ruleset (CLAUDE.md L161): Canonical JSON, sampling rules ✅
- Performance Ruleset (CLAUDE.md L165): Latency monitoring ✅

---

### Branch 4: Adapter Activation Tracking ✅
**Branch**: `auto/implement-adapter-activation-tracking`  
**Prompt Match**: CODEX_PROMPTS_PHASE2.md L135-159 (Prompt 4)  
**Changes**: 3 files, +275 lines, 0 deletions  
**Files Modified**:
- `crates/adapteros-lora-lifecycle/src/activation_tracker.rs` (NEW)
- `crates/adapteros-lora-lifecycle/src/lib.rs` (export)
- `crates/adapteros-lora-worker/src/lib.rs` (integration)

**Assessment**: ✅ Implements Policy Pack 19 requirement  
**Risk Level**: LOW - New functionality  
**Standards Compliance**:
- ✅ Evicts adapters below 2% per CLAUDE.md L377 (Policy 19)
- ✅ Tracks activation_pct per migration schema L152
- ✅ Uses existing database schema per CODEX_PROMPTS_PHASE2.md L150-153

**Policy Pack Alignment**:
- Adapter Lifecycle Ruleset (CLAUDE.md L376-387): min_activation_pct enforcement ✅

---

### Branch 5: Adapter Performance Profiling ⚠️
**Branch**: `auto/implement-adapter-specific-performance-profiling`  
**Prompt Match**: CODEX_PROMPTS_PHASE2.md L249-289 (Prompt 8)  
**Changes**: 2 files, +322 lines, -239 lines (net +83)  
**Files Modified**:
- `crates/adapteros-profiler/src/adapter_profiler.rs` (NEW, 317 lines)
- `crates/adapteros-profiler/src/lib.rs` (-239 lines refactored)

**Assessment**: ⚠️ **REFACTORS existing profiler** - needs careful review  
**Risk Level**: MEDIUM - Modifies existing functionality  
**Concerns**:
- Removes 239 lines from existing lib.rs
- May break existing profiler integrations
- Need to verify no regression in profiling functionality

**Standards Compliance**:
- ✅ Under 500 lines per prompt
- ⚠️ Not purely additive - conflicts with Phase2 guideline L430
- ⚠️ Need to verify existing profiler still works

---

### Branch 6: CLI Table Method ❌
**Branch**: `auto/implement-table-method-for-cli-output-writer`  
**Prompt Match**: CODEX_PROMPTS_PHASE2.md L45-85 (Prompt 2)  
**Changes**: 3 files, +195 lines, -136 lines (net +59)  
**Files Modified**:
- `crates/adapteros-cli/src/output.rs` (refactored)
- `crates/adapteros-cli/src/commands/list_adapters.rs` (updated)
- `crates/adapteros-cli/src/commands/pin.rs` (updated)

**Assessment**: ❌ **2 COMPILATION ERRORS**  
**Risk Level**: HIGH - Does not compile  
**Errors**:
```
error[E0308]: mismatched types
 --> crates/adapteros-cli/src/output.rs:187:28
  |
187 |             table.add_row(row);
    |                   ------- ^^^ expected `Vec<comfy_table::Cell>`, found `Vec<String>`
```

**Standards Compliance**:
- ❌ Fails compilation per CLAUDE.md L78 requirement
- ❌ Cannot integrate without fixes per Anti-Hallucination Framework

---

## Integration Strategy

### Approach: Incremental with Verification

Per CLAUDE.md L90-97 and Phase 1 success pattern:
1. One branch at a time
2. Compilation verification after each
3. Test verification after each  
4. Git commit after each
5. Document evidence for each

### Order of Integration (Recommended)

**Phase 2.1: Foundation (2 PRs)**
1. **Enhanced Error Context** - Foundation for better debugging
2. **Production Monitoring** - Observability infrastructure

**Phase 2.2: Features (2 PRs)**
3. **Activation Tracking** - Policy compliance (required by Policy 19)
4. **Batch Inference API** - Feature enhancement

**Phase 2.3: Review & Fix (2 PRs)**
5. **Performance Profiling** - Review refactoring, verify no regression
6. **CLI Table Method** - Fix compilation errors, then integrate

---

## Detailed Integration Plan

### Phase 2.1: Foundation

#### Step 1: Enhanced Error Context

**Branch**: `auto/add-enhanced-error-context-in-codebase`

**Pre-Integration Checks**:
```bash
# 1. Check for duplicates
grep -r "with_context\|add_context" crates/adapteros-core/src/

# 2. Checkout branch
git checkout auto/add-enhanced-error-context-in-codebase

# 3. Verify no conflicts
git diff main --name-only

# 4. Check compilation
cargo check --package adapteros-core --package adapteros-db

# 5. Run tests
cargo test --package adapteros-core --lib
cargo test --package adapteros-db --lib
```

**Integration**:
```bash
# 6. Merge to main
git checkout main
git merge --no-ff auto/add-enhanced-error-context-in-codebase

# 7. Verify workspace compilation
cargo check --workspace

# 8. Commit
git commit -m "feat: Add enhanced error context with structured fields

- Add context() and with_context() methods to AosError
- Implement error chain tracking
- Update critical error sites in db layer
- Zero-cost abstraction when not triggered

Refs: CODEX_PROMPTS_PHASE2.md Prompt 5
Standards: CLAUDE.md L90-97"
```

**Post-Integration Verification**:
```bash
# 9. Re-read modified files
cat crates/adapteros-core/src/error.rs | grep -A 5 "with_context"

# 10. Verify error context in db layer
grep "context\|with_context" crates/adapteros-db/src/postgres.rs

# 11. Full workspace test
cargo test --workspace --lib
```

**Evidence Required**:
- [ ] File contents verified with grep
- [ ] Compilation successful
- [ ] Tests passing
- [ ] No duplicate implementations
- [ ] Follows existing patterns

**Success Criteria**:
- ✅ AosError has context methods
- ✅ Database layer uses new context
- ✅ Zero compilation errors
- ✅ All tests pass

---

#### Step 2: Production Monitoring

**Branch**: `auto/add-production-monitoring-telemetry-module`

**Pre-Integration Checks**:
```bash
# 1. Check for duplicates
find crates/adapteros-telemetry/src -name "*monitoring*"
grep -r "health_check\|performance_alert" crates/adapteros-telemetry/

# 2. Checkout branch
git checkout auto/add-production-monitoring-telemetry-module

# 3. Verify it's a new file
git diff main --name-only | grep monitoring.rs

# 4. Check compilation
cargo check --package adapteros-telemetry

# 5. Run tests
cargo test --package adapteros-telemetry --lib
```

**Integration**:
```bash
# 6. Merge to main
git checkout main
git merge --no-ff auto/add-production-monitoring-telemetry-module

# 7. Verify workspace compilation
cargo check --workspace

# 8. Commit
git commit -m "feat: Add production monitoring telemetry module

- Add health check event types
- Add performance threshold monitoring  
- Add alert event types for policy violations
- Integrate with existing TelemetryWriter
- Use canonical JSON format per Telemetry Ruleset

Refs: CODEX_PROMPTS_PHASE2.md Prompt 3
Policy: CLAUDE.md L161 (Telemetry Ruleset)
Standards: CLAUDE.md L90-97"
```

**Post-Integration Verification**:
```bash
# 9. Verify new module exists
ls -la crates/adapteros-telemetry/src/monitoring.rs

# 10. Verify exports
grep "pub mod monitoring" crates/adapteros-telemetry/src/lib.rs

# 11. Verify canonical JSON usage
grep "canonical_json\|serde_json" crates/adapteros-telemetry/src/monitoring.rs

# 12. Full workspace test
cargo test --workspace --lib
```

**Evidence Required**:
- [ ] New module created (no existing file modified)
- [ ] Exported from lib.rs
- [ ] Uses canonical JSON
- [ ] Integrates with TelemetryWriter
- [ ] All tests pass

**Success Criteria**:
- ✅ New monitoring.rs module exists
- ✅ Event types defined (health_check, performance_alert, etc.)
- ✅ Canonical JSON format used
- ✅ Zero compilation errors
- ✅ All tests pass

---

### Phase 2.2: Features

#### Step 3: Activation Tracking

**Branch**: `auto/implement-adapter-activation-tracking`

**Pre-Integration Checks**:
```bash
# 1. Check for duplicates
find crates/adapteros-lora-lifecycle/src -name "*activation*"
grep -r "activation_pct\|ActivationTracker" crates/

# 2. Verify database schema exists
grep "activation_pct" migrations/*.sql

# 3. Checkout branch
git checkout auto/implement-adapter-activation-tracking

# 4. Check compilation
cargo check --package adapteros-lora-lifecycle --package adapteros-lora-worker

# 5. Run tests
cargo test --package adapteros-lora-lifecycle --lib
```

**Integration**:
```bash
# 6. Merge to main
git checkout main
git merge --no-ff auto/implement-adapter-activation-tracking

# 7. Verify workspace compilation
cargo check --workspace

# 8. Commit
git commit -m "feat: Implement adapter activation tracking

- Create ActivationTracker struct
- Track adapter selection frequency
- Calculate rolling activation percentages
- Update database with activation_pct
- Evict adapters below 2% per Policy 19
- Integrate with router decisions

Refs: CODEX_PROMPTS_PHASE2.md Prompt 4
Policy: CLAUDE.md L377 (Adapter Lifecycle Ruleset)
Standards: CLAUDE.md L90-97"
```

**Post-Integration Verification**:
```bash
# 9. Verify new tracker module
ls -la crates/adapteros-lora-lifecycle/src/activation_tracker.rs

# 10. Verify integration with router
grep "ActivationTracker\|activation_pct" crates/adapteros-lora-worker/src/lib.rs

# 11. Verify 2% threshold
grep "min_activation_pct.*2" crates/adapteros-lora-lifecycle/src/activation_tracker.rs

# 12. Full workspace test
cargo test --workspace --lib
```

**Evidence Required**:
- [ ] ActivationTracker struct created
- [ ] Database integration verified
- [ ] 2% threshold enforced
- [ ] Router integration confirmed
- [ ] All tests pass

**Success Criteria**:
- ✅ Tracks adapter activation percentages
- ✅ Updates database activation_pct column
- ✅ Evicts adapters below 2% threshold
- ✅ Complies with Policy Pack 19
- ✅ Zero compilation errors

---

#### Step 4: Batch Inference API

**Branch**: `auto/add-batch-inference-api-endpoint`

**Pre-Integration Checks**:
```bash
# 1. Check for duplicate batch handlers
find crates/adapteros-server-api/src/handlers -name "*batch*"
grep -r "batch.*inference" crates/adapteros-server-api/

# 2. Checkout branch
git checkout auto/add-batch-inference-api-endpoint

# 3. Check compilation
cargo check --package adapteros-server-api

# 4. Run tests (including new batch tests)
cargo test --package adapteros-server-api --test batch_infer
```

**Integration**:
```bash
# 5. Merge to main
git checkout main
git merge --no-ff auto/add-batch-inference-api-endpoint

# 6. Verify workspace compilation
cargo check --workspace

# 7. Commit
git commit -m "feat: Add batch inference API endpoint

- Create batch inference handler
- Accept array of inference requests
- Process requests efficiently with shared model state
- Return array of responses with request IDs
- Implement max batch size limit (32 requests)
- Add batch timeout handling
- Add comprehensive integration tests (312 lines)

Refs: CODEX_PROMPTS_PHASE2.md Prompt 7
Standards: CLAUDE.md L90-97"
```

**Post-Integration Verification**:
```bash
# 8. Verify new handler
ls -la crates/adapteros-server-api/src/handlers/batch.rs

# 9. Verify route registration
grep "batch" crates/adapteros-server-api/src/routes.rs

# 10. Verify test coverage
cargo test --package adapteros-server-api --test batch_infer -- --nocapture

# 11. Verify max batch size
grep "max.*batch.*32" crates/adapteros-server-api/src/handlers/batch.rs

# 12. Full workspace test
cargo test --workspace --lib
```

**Evidence Required**:
- [ ] New batch handler created
- [ ] Routes registered
- [ ] Tests comprehensive (312 lines)
- [ ] Max batch size enforced (32)
- [ ] Timeout handling implemented

**Success Criteria**:
- ✅ Batch API endpoint functional
- ✅ Handles up to 32 requests per batch
- ✅ Timeout handling in place
- ✅ Integration tests pass
- ✅ Zero compilation errors

---

### Phase 2.3: Review & Fix

#### Step 5: Performance Profiling (REQUIRES REVIEW)

**Branch**: `auto/implement-adapter-specific-performance-profiling`

**CAUTION**: This branch refactors existing code (-239 lines)

**Pre-Integration Analysis**:
```bash
# 1. Checkout branch
git checkout auto/implement-adapter-specific-performance-profiling

# 2. Review what was deleted
git diff main crates/adapteros-profiler/src/lib.rs | grep "^-" | head -50

# 3. Verify new module doesn't break existing functionality
cargo check --package adapteros-profiler

# 4. Run ALL profiler tests
cargo test --package adapteros-profiler

# 5. Check for usages of deleted functions
grep -r "adapteros_profiler::" crates/ tests/ | grep -v "adapter_profiler"
```

**Decision Point**: ⚠️ **STOP and REVIEW**

Before integrating, answer:
1. Does existing profiler functionality still work?
2. Are there breaking changes to the profiler API?
3. Do all existing tests pass?
4. Are there any references to deleted functions?

**If YES to all**:
```bash
# Proceed with integration
git checkout main
git merge --no-ff auto/implement-adapter-specific-performance-profiling

git commit -m "feat: Add adapter-specific performance profiling

- Create AdapterProfiler struct
- Track per-adapter inference latency
- Track per-adapter memory usage
- Track adapter selection frequency
- Generate performance reports
- Refactor existing profiler module for better organization

Refs: CODEX_PROMPTS_PHASE2.md Prompt 8
Standards: CLAUDE.md L90-97

BREAKING: Refactored profiler module structure"
```

**If NO to any**:
```bash
# Reject and document why
git checkout main
echo "Branch needs revision - breaks existing functionality" >> PHASE2_PR_REVIEW.md
```

**Post-Integration Verification** (if integrated):
```bash
# Verify no regressions
cargo test --workspace --lib

# Check all profiler usages still work
cargo check --workspace
```

---

#### Step 6: CLI Table Method (REQUIRES FIXES)

**Branch**: `auto/implement-table-method-for-cli-output-writer`

**BLOCKED**: ❌ **2 compilation errors must be fixed first**

**Error Analysis**:
```
error[E0308]: mismatched types
 --> crates/adapteros-cli/src/output.rs:187:28
  |
187 |             table.add_row(row);
    |                   ------- ^^^ expected `Vec<comfy_table::Cell>`, found `Vec<String>`
```

**Fix Required**:
```rust
// BEFORE (broken):
table.add_row(row);  // row is Vec<String>

// AFTER (fixed):
table.add_row(row.into_iter().map(|s| Cell::new(s)).collect());
// OR
use comfy_table::Cell;
table.add_row(row.iter().map(|s| Cell::new(s)));
```

**Fix Strategy**:

Option A: **Fix locally and create new commit**
```bash
git checkout auto/implement-table-method-for-cli-output-writer
# Edit crates/adapteros-cli/src/output.rs to fix type conversions
cargo check --package adapteros-cli  # Verify fix
git add crates/adapteros-cli/src/output.rs
git commit -m "fix: Convert Vec<String> to Vec<Cell> for comfy_table"
git checkout main
git merge --no-ff auto/implement-table-method-for-cli-output-writer
```

Option B: **Request Codex to fix**
```bash
# Document the errors and request fix
# Wait for updated branch
```

**Post-Fix Integration**:
```bash
# After errors fixed
cargo check --package adapteros-cli
cargo test --package adapteros-cli --lib

git checkout main
git merge --no-ff auto/implement-table-method-for-cli-output-writer

git commit -m "feat: Implement table() method for CLI output writer

- Add table() method to OutputWriter struct
- Support both human-readable and JSON output modes
- Use comfy_table for formatted tables
- Support column alignment and formatting options
- Add helper methods: table_row(), table_header(), table_footer()
- Update list_adapters and pin commands to use new table method

Refs: CODEX_PROMPTS_PHASE2.md Prompt 2
Standards: CLAUDE.md L90-97
Fix: Convert Vec<String> to Vec<Cell> for comfy_table compatibility"
```

---

## Anti-Hallucination Verification Checklist

Per `.cursor/rules/global.mdc` and Phase 1 success pattern:

### Before Each Integration:
- [ ] Search for existing implementations (`codebase_search`)
- [ ] Check for duplicate symbols (`grep`)
- [ ] Read existing files to understand patterns
- [ ] Document findings with evidence

### After Each Integration:
- [ ] Re-read all modified files
- [ ] Use `grep` to verify specific changes
- [ ] Run `cargo check` for compilation
- [ ] Run `cargo test` for integration validation
- [ ] Check for duplicate implementations across crates
- [ ] Verify no conflicts with existing code

### Documentation Required:
- [ ] File paths and line numbers cited
- [ ] Compilation output captured
- [ ] Test results documented
- [ ] Git commit history preserved
- [ ] Evidence trail maintained

---

## Success Metrics

### Per Integration:
- [ ] Compiles without errors
- [ ] All tests pass
- [ ] Under line limit (varies per prompt)
- [ ] No conflicts with Phase 1 work
- [ ] Follows CLAUDE.md standards
- [ ] Policy packs enforced where applicable

### Overall Phase 2:
- [ ] 6 PRs integrated (or documented rejections)
- [ ] Full workspace compilation
- [ ] All tests passing
- [ ] ~1,750 net new lines added
- [ ] Zero regressions from Phase 1
- [ ] All CODEX_PROMPTS_PHASE2.md prompts addressed

---

## Risk Mitigation

### High Risk Items:
1. **Performance Profiling** - Refactors existing code
   - Mitigation: Thorough review, verify no API breaking changes
   
2. **CLI Table Method** - Has compilation errors
   - Mitigation: Fix errors before integration, verify all CLI commands work

### Medium Risk Items:
- None identified - all other branches are additive

### Low Risk Items:
- Batch Inference API (new endpoint)
- Enhanced Error Context (minimal changes)
- Production Monitoring (new module)
- Activation Tracking (new module)

---

## Rollback Plan

If any integration causes issues:

```bash
# 1. Identify problematic commit
git log --oneline -10

# 2. Revert the specific integration
git revert <commit-hash>

# 3. Document why it was reverted
echo "Reverted: <reason>" >> PHASE2_INTEGRATION_ISSUES.md

# 4. Continue with remaining integrations
```

---

## Timeline Estimate

**Phase 2.1 (Foundation)**: ~30 minutes
- Step 1: Enhanced Error Context (15 min)
- Step 2: Production Monitoring (15 min)

**Phase 2.2 (Features)**: ~30 minutes
- Step 3: Activation Tracking (15 min)
- Step 4: Batch Inference API (15 min)

**Phase 2.3 (Review & Fix)**: ~45 minutes
- Step 5: Performance Profiling review (20 min)
- Step 6: CLI Table Method fix + integrate (25 min)

**Total Estimated Time**: ~105 minutes (1.75 hours)

---

## Final Verification

After all integrations complete:

```bash
# 1. Full workspace build
cargo build --release

# 2. Full test suite
cargo test --workspace

# 3. Linting
cargo clippy --workspace -- -D warnings

# 4. Format check
cargo fmt --all -- --check

# 5. Git status
git log --oneline -15
git status

# 6. Push to origin
git push origin main
```

---

## Documentation Updates

After Phase 2 completion:

1. Update `INTEGRATION_PHASE1_COMPLETE.md` → `INTEGRATION_PHASE2_COMPLETE.md`
2. Update `PR_REVIEW_CRITICAL_ANALYSIS.md` with Phase 2 results
3. Create `PHASE2_COMPLETION_REPORT.md` with metrics
4. Archive Phase 2 prompts to `docs/archive/integration-2025-10/`

---

**Status**: ✅ PLAN READY FOR EXECUTION

This plan follows all standards from Phase 1 success:
- Incremental integration with verification
- Evidence-based approach
- Anti-hallucination framework
- Policy pack compliance
- Full audit trail

**Ready to begin Phase 2.1: Foundation** 🚀

