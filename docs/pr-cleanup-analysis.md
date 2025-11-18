# Phase 4 Cleanup Branch Analysis

## Executive Summary

Analyzed 4 branches for safe, non-breaking changes compatible with current `main`:
- **2025-10-29-5bph-ZQpnI**: 1 commit, 129 files, forked at e483db01
- **2025-10-29-8roq-DLX4w**: 5 commits, 137 files, forked at e483db01
- **feat-adapter-packaging-2c34c**: 11 commits, 91 files, forked at fac9eec
- **pr/database-audit-reliability**: 1 commit, 3 files, forked at f6e44c6

**Recommendation**: These branches are quite old (missing 15+ commits from main) and contain extensive changes that overlap with or contradict recent PRD work. Most changes must be dropped per safety rules.

---

## Branch 1: origin/2025-10-29-5bph-ZQpnI

**Commit**: cd4cd9af "server-api: load base model via import paths; add memory estimation"
**Status**: Mostly UNSAFE - extensive kernel/core changes

### Safe Changes (can keep):
1. **Duplication Monitoring** (build quality tooling)
   - `.github/workflows/duplication.yml` - CI workflow
   - `configs/jscpd.config.json` - Duplication checker config
   - `.githooks/pre-commit` - Optional pre-commit duplication check
   - `docs/DUPLICATION_MONITORING.md` - Documentation
   - README.md addition (duplication section)
   - Makefile: Add `make dup` target

2. **CLI Improvements**
   - `crates/adapteros-cli/src/commands/verify_adapter.rs` - Use B3Hash::hash() instead of blake3::hash() directly (consistency fix)

### Unsafe Changes (must drop):
1. **adapteros-aos crate** - Main has newer AOS 2.0 implementation with aos2_implementation.rs and bin/aos.rs that this branch removes (regression)
2. **Metal kernels** (adapteros-lora-kernel-mtl) - Extensive changes to fused_mlp.rs, fused_qkv.rs, debug.rs (FORBIDDEN: touches core router/determinism logic)
3. **Server API base model loading** - Would conflict with recent PRD implementations
4. **Lifecycle/loader changes** - Overlap with recent PRD work

**Files Changed**: 129
**Keep**: ~10 files (duplication tooling + 1 CLI fix)
**Drop**: ~119 files (kernel, aos, api, lifecycle changes)

---

## Branch 2: origin/2025-10-29-8roq-DLX4w

**Commits**:
- 01b250de "feat(ui): add IT Admin Dashboard, User Reports, and Single-File Adapter Trainer"
- b8a4931c "fix: complete sqlx macro conversions in system-metrics"
- d0101e99 "feat: comprehensive implementation updates across codebase"
- 9ea53a0f "fix: resolve compilation errors in lifecycle, system-metrics, and kernel-mtl"
- 61cbba4a "feat(ui): Add IT Admin Dashboard, User Reports, and Single-File Adapter Trainer"

**Status**: NEEDS DETAILED ANALYSIS (UI/test changes likely safe, core changes likely unsafe)

### Preliminary Assessment:
- UI components (IT Admin Dashboard, User Reports) - Likely SAFE (additive UI features)
- sqlx macro conversions - Need to check if they match current schema
- Compilation fixes in lifecycle/kernel-mtl - UNSAFE (touches core)
- Single-File Adapter Trainer - Need to check for schema/API changes

**Next Step**: Detailed file-by-file analysis required

---

## Branch 3: origin/feat-adapter-packaging-2c34c

**Commits**: 11 commits over 91 files
**Fork Point**: fac9eec (newer than branches 1-2)

**Commits Include**:
- Formatting fixes (`cargo fmt`)
- README updates
- Integration tests
- UI refactoring
- API enhancements
- CLI inference command
- Menu bar app modularization

**Status**: NEEDS DETAILED ANALYSIS

### Preliminary Assessment:
- `cargo fmt` changes - SAFE (formatting only)
- README/docs updates - Likely SAFE
- Tests - SAFE (additive)
- UI refactoring - Need to check for component library violations
- API enhancements - Need to check for RouterDecisionEvent/InferenceEvent changes (FORBIDDEN)
- CLI additions - Need to check for breaking changes

**Next Step**: Check for schema, telemetry bundle, or API contract changes

---

## Branch 4: origin/pr/database-audit-reliability

**Commit**: de61afcb "fix: database and audit system reliability"
**Status**: PARTIALLY SAFE

### Safe Changes (can keep):
1. **Audit Helper Constants** (`crates/adapteros-server-api/src/audit_helper.rs`)
   - Add `SSE_AUTHENTICATION` action constant
   - Add `STREAM_ENDPOINT` resource constant
   - No behavioral changes, just new constants for future use

### Unsafe Changes (must drop):
1. **Migration 0066** - `migrations/0066_add_frameworks_to_repositories.sql` (FORBIDDEN: schema change)
2. **Repository struct** - `crates/adapteros-db/src/repositories.rs` - Adds `frameworks_json` field (depends on migration 0066)

**Files Changed**: 3
**Keep**: 1 file (audit_helper.rs constants only)
**Drop**: 2 files (migration + repository changes)

---

## Integration Strategy

### Immediate Action (Minimal Risk):
Create single cleanup PR with:
1. Duplication monitoring tooling (branch 1) - 10 files
2. Audit helper constants (branch 4) - 2 lines
3. CLI B3Hash fix (branch 1) - 1 small change

**Total**: ~10-15 files, all build/quality/constants

### Future Work (Requires Deeper Analysis):
- Branch 2: UI components (after checking component library compliance)
- Branch 3: After verifying no telemetry/API contract changes
- None of the kernel/lifecycle/aos/core changes (all UNSAFE)

---

## Safety Rules Applied

### Forbidden Changes (All Dropped):
- ❌ Database migrations (0066)
- ❌ Telemetry bundle schemas
- ❌ RouterDecisionEvent/InferenceEvent fields
- ❌ Adapter/stack metadata structs
- ❌ Kernel/router core logic (Metal kernel changes)
- ❌ Lifecycle manager changes (overlap with recent PRDs)

### Allowed Changes (Candidates for Keeping):
- ✅ Documentation (*.md files)
- ✅ Build configuration (Makefile, workflows)
- ✅ Code quality tooling (duplication checks)
- ✅ Tests (additive only)
- ✅ Constants/enums (non-breaking additions)
- ✅ Formatting fixes

---

## Recommendation

**Phase 1 (Now)**: Create minimal cleanup PR with ~15 files (duplication tooling + audit constants + CLI fix)
**Phase 2 (Later)**: Manually port UI components from branch 2 if needed (new PR, careful review)
**Phase 3 (Later)**: Cherry-pick safe tests/docs from branch 3 if needed (new PR)

**Discard**: 90%+ of changes from all branches (outdated, overlapping with recent PRD work, or touching forbidden areas)
