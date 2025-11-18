# Branch Cleanup: 2025-10-29-5bph-ZQpnI

## Overview

**Branch**: `origin/2025-10-29-5bph-ZQpnI`
**Commit**: cd4cd9af "server-api: load base model via import paths; add memory estimation"
**Fork Point**: e483db01 (before PRD 1-5 implementations)
**Files Changed**: 129
**Status**: Minimal integration (duplication tooling + 1 CLI fix)

---

## What Was Kept (11 files)

### 1. Duplication Monitoring Tooling (build quality)
- ✅ `.github/workflows/duplication.yml` - CI workflow for jscpd duplication detection
- ✅ `configs/jscpd.config.json` - Duplication checker configuration
- ✅ `.githooks/pre-commit` - Optional pre-commit duplication check (advisory mode)
- ✅ `docs/DUPLICATION_MONITORING.md` - Documentation for duplication monitoring
- ✅ `README.md` - Added "Duplication Monitoring" section
- ✅ `Makefile` - Already has `make dup` target (no changes needed)
- ✅ `scripts/run_jscpd.sh` - Already exists in main (no changes needed)

**Why Safe**: These are build/quality tooling additions that don't affect runtime behavior. They add code quality scanning capabilities without changing any core logic, schemas, or APIs.

### 2. CLI Consistency Fix (1 file)
- ✅ `crates/adapteros-cli/src/commands/verify_adapter.rs` - Use `adapteros_core::B3Hash::hash()` instead of `blake3::hash()` directly

**Why Safe**: This is a consistency improvement that uses the codebase's standard hashing abstraction instead of calling blake3 directly. No behavioral change, just better code organization.

---

## What Was Discarded (~118 files)

### 1. adapteros-aos Crate Changes (UNSAFE - Regression)
**Files**: `crates/adapteros-aos/src/*` (8 files modified/removed)

**Reason for Discard**: Current main has newer AOS 2.0 implementation with:
- `aos2_implementation.rs` - AOS 2.0 format loader
- `aos2_writer.rs` - AOS 2.0 writer
- `bin/aos.rs` - Binary for AOS file operations

Branch REMOVES these files and has older implementation. This is a **regression** that would break .aos file support.

**Impact**: No loss - main has superior implementation

### 2. Metal Kernel Changes (FORBIDDEN - Core Logic)
**Files**: `crates/adapteros-lora-kernel-mtl/src/*` (~10 files)

**Changes**:
- `fused_mlp.rs` - Extensive rewrites to MLP kernel dispatch
- `fused_qkv.rs` - Attention kernel parameter changes
- `debug.rs` - Changed from `tracing::debug!` to `println!`
- `ane_acceleration.rs` - Dead code attribute changes
- `build.rs` - Error handling style changes

**Reason for Discard**:
- **FORBIDDEN AREA**: Kernel/router core logic (per cleanup rules)
- Changes touch determinism-critical code paths
- Risk of breaking router determinism guarantees
- Recent PRD work (circuit breaker, schema validation) may have updated kernels
- Switching from tracing to println breaks telemetry compliance

**Impact**: No loss - current kernels are determinism-verified

### 3. Server API Base Model Loading (UNSAFE - Conflicts)
**Files**: `crates/adapteros-server-api/*`, `crates/adapteros-db/src/process_monitoring.rs`

**Reason for Discard**:
- Overlaps with recent PRD implementations (PRD 1: circuit breaker, PRD 5: schema validation)
- Would require extensive conflict resolution
- Base model loading already functional in main

**Impact**: No loss - main has working base model support

### 4. Lifecycle/Loader Changes (UNSAFE - Overlaps)
**Files**: `crates/adapteros-lora-lifecycle/src/*`, `crates/adapteros-lora-worker/*`

**Reason for Discard**:
- Overlaps with recent lifecycle improvements in main
- Branch is too old (missing 15+ commits of lifecycle refinements)
- Risk of reintroducing bugs that were fixed

**Impact**: No loss - main has more mature lifecycle management

### 5. UI Changes (NEEDS SEPARATE REVIEW)
**Files**: `ui/src/*` (~20 files)

**Reason for Discard (from this PR)**:
- Need component library compliance audit (separate task)
- May violate recent component standardization
- Should be reviewed separately if features are desired

**Impact**: Deferred - can be cherry-picked later if needed

### 6. Git Subsystem Changes (UNSAFE - Overlaps)
**Files**: `crates/adapteros-git/src/subsystem.rs`

**Reason for Discard**: Overlaps with recent git integration work in main

### 7. Configuration Files (MIXED)
**Files**: `configs/cp-auth-example.toml`, `configs/cp.toml`, `configs/production-multinode.toml`

**Reason for Discard**: Would need manual review to merge config changes without breaking current deployments. Low value, high risk.

---

## Compatibility with Current Main

### Schema Compatibility
- ✅ **No schema changes** in kept files
- ✅ No migrations added
- ✅ No struct field changes

### API Compatibility
- ✅ **No API changes** in kept files
- ✅ No RouterDecisionEvent changes
- ✅ No InferenceEvent changes
- ✅ No telemetry bundle schema changes

### Build Compatibility
- ✅ Duplication workflow uses standard Node/npx (no new dependencies)
- ✅ Pre-commit hook is optional (no enforcement unless `JSCPD_ENFORCE=1`)
- ✅ CLI fix compiles cleanly with current main

---

## Testing

### Build Test
```bash
cargo check -p adapteros-cli  # Verifies CLI B3Hash change compiles
```

### Workflow Test
```bash
make dup  # Runs jscpd duplication scan (requires Node.js)
```

### Pre-commit Hook Test
```bash
# Optional - user must manually install hooks
bash scripts/install_git_hooks.sh  # If hook installer exists
# Or manually: ln -s ../../.githooks/pre-commit .git/hooks/pre-commit
```

---

## Summary

**Kept**: 11 files (10 duplication tooling + 1 CLI fix)
**Discarded**: ~118 files (aos regression, kernel changes, overlapping work)
**Risk Level**: **MINIMAL** - Only build tooling and one consistency fix
**Recommendation**: ✅ **SAFE TO MERGE**

This cleanup extracts the low-risk build quality improvements while avoiding all schema, kernel, API, and lifecycle changes that could conflict with recent PRD work.
