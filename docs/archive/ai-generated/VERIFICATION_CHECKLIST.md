# Documentation Verification Checklist

This checklist tracks what needs to be tested and verified to ensure documentation accuracy.

**Created**: 2025-01-18
**Status**: All items pending verification
**Purpose**: Make it obvious what claims need validation

---

## Build Commands (Priority: HIGH)

### macOS Builds

- [ ] `cargo build --release --locked --offline`
  - **Status**: ⏳ Untested
  - **Platform**: macOS 13.0+ with Apple Silicon
  - **Expected**: Success (clean build)
  - **Documented in**: README.md, docs/LOCAL_BUILD.md

- [ ] `cargo build --release --features metal-backend`
  - **Status**: ⏳ Untested
  - **Platform**: macOS 13.0+ with Apple Silicon
  - **Expected**: Success
  - **Documented in**: docs/FEATURE_FLAGS.md

- [ ] `cargo build --release --features "metal-backend,full"`
  - **Status**: ⏳ Untested
  - **Platform**: macOS 13.0+ with Apple Silicon
  - **Expected**: Success
  - **Documented in**: docs/FEATURE_FLAGS.md

### Linux/CI Builds

- [ ] `cargo build --release --no-default-features`
  - **Status**: ⏳ Untested
  - **Platform**: Linux (any)
  - **Expected**: Success (no Metal dependencies)
  - **Documented in**: README.md, docs/LOCAL_BUILD.md

- [ ] `cargo build --release --features full`
  - **Status**: ⏳ Untested
  - **Platform**: Linux (any)
  - **Expected**: Success
  - **Documented in**: docs/FEATURE_FLAGS.md

### Validation Commands

- [ ] `cargo check --workspace`
  - **Status**: ✅ Verified (has pre-existing errors in adapteros-lora-worker)
  - **Platform**: macOS (tested)
  - **Expected**: 70 errors in adapteros-lora-worker (known issue)
  - **Documented in**: README.md

- [ ] `cargo test --workspace --exclude adapteros-lora-mlx-ffi`
  - **Status**: ⏳ Untested
  - **Platform**: All
  - **Expected**: Tests pass (or document known failures)
  - **Documented in**: tests/README.md

---

## Environment Variables (Priority: HIGH)

- [ ] Verify `DATABASE_URL` is actually required
  - **Status**: 💭 Inferred (cargo output shows "SQLX validation disabled")
  - **Test**: Build without setting DATABASE_URL
  - **Expected**: May work if sqlx offline mode is enabled
  - **Documented in**: docs/LOCAL_BUILD.md

- [ ] Verify `AOS_MANIFEST_PATH` is optional
  - **Status**: ⏳ Untested
  - **Test**: Run inference without setting variable
  - **Expected**: Should fall back to CLI flag
  - **Documented in**: docs/LOCAL_BUILD.md, CLAUDE.md

---

## Performance Measurements (Priority: MEDIUM)

### Hot-Swap Latencies

- [ ] Measure preload latency
  - **Status**: 💭 Estimated ~500ms (not benchmarked)
  - **Test**: Instrument `AdapterTable::preload()` with timing
  - **Documented in**: docs/HOT_SWAP.md:570

- [ ] Measure swap latency
  - **Status**: 💭 Estimated <5ms (not benchmarked)
  - **Test**: Instrument `AdapterTable::swap()` with timing
  - **Documented in**: docs/HOT_SWAP.md:571

- [ ] Measure rollback latency
  - **Status**: 💭 Estimated <5ms (not benchmarked)
  - **Test**: Instrument `AdapterTable::rollback()` with timing
  - **Documented in**: docs/HOT_SWAP.md:572

- [ ] Measure retirement wake latency
  - **Status**: 💭 Estimated <5ms (not benchmarked)
  - **Test**: Instrument retirement task wake-up
  - **Documented in**: docs/HOT_SWAP.md:574

- [ ] Measure swap throughput
  - **Status**: 💭 Estimated 200+ swaps/second (not benchmarked)
  - **Test**: Benchmark loop calling `swap()` repeatedly
  - **Documented in**: docs/HOT_SWAP.md:577

---

## Test Coverage (Priority: MEDIUM)

- [ ] Run `cargo tarpaulin --workspace`
  - **Status**: ⏳ Not run
  - **Purpose**: Measure actual code coverage
  - **Documented in**: tests/README.md:418-440

- [ ] Run `cargo llvm-cov --workspace --html`
  - **Status**: ⏳ Not run
  - **Purpose**: Alternative coverage measurement
  - **Documented in**: tests/README.md:442-456

---

## Fresh Environment Testing (Priority: HIGH)

- [ ] Test docs/LOCAL_BUILD.md on clean macOS system
  - **Status**: ⏳ Never tested
  - **Platform**: macOS 13.0+ (fresh install)
  - **Expected**: All commands work as documented
  - **Steps**:
    1. Follow environment setup
    2. Run canonical build command
    3. Document any issues or missing steps

- [ ] Test docs/LOCAL_BUILD.md on clean Linux system
  - **Status**: ⏳ Never tested
  - **Platform**: Ubuntu 22.04 or similar
  - **Expected**: CPU-only build works
  - **Steps**:
    1. Follow environment setup
    2. Run `--no-default-features` build
    3. Document any issues

---

## Feature Flag Functionality (Priority: MEDIUM)

- [ ] Verify `mock-backend` flag behavior
  - **Status**: ⚠️ Known issue - flag exists but non-functional
  - **Test**: Check if MockKernels compilation is gated
  - **Expected**: Currently always compiled (not gated)
  - **Documented in**: Cargo.toml:90-92, docs/FEATURE_FLAGS.md:645

- [ ] Verify `metal-backend` compiles on macOS
  - **Status**: ⏳ Untested
  - **Test**: `cargo build --features metal-backend`
  - **Expected**: Success on macOS, failure on Linux

- [ ] Verify `mlx-backend` fails as documented
  - **Status**: ⏳ Untested
  - **Test**: `cargo build --features experimental-backends`
  - **Expected**: PyO3 linker errors
  - **Documented in**: docs/FEATURE_FLAGS.md:647

---

## Link Verification (Priority: LOW)

- [ ] Verify all internal links in README.md
  - **Status**: ⏳ Not checked
  - **Test**: Click all links, verify files exist

- [ ] Verify all internal links in docs/README.md
  - **Status**: ⏳ Not checked
  - **Test**: Click all links, verify files exist

- [ ] Verify all internal links in docs/LOCAL_BUILD.md
  - **Status**: ⏳ Not checked
  - **Test**: Click all links, verify files exist

- [ ] Verify all internal links in docs/FEATURE_FLAGS.md
  - **Status**: ⏳ Not checked
  - **Test**: Click all links, verify files exist

- [ ] Verify all internal links in docs/HOT_SWAP.md
  - **Status**: ⏳ Not checked
  - **Test**: Click all links, verify files exist

---

## Troubleshooting Validation (Priority: LOW)

### Documented Solutions

- [ ] Verify "No such file or directory: DATABASE_URL" solution works
  - **Documented in**: docs/LOCAL_BUILD.md:298-311
  - **Test**: Trigger error, apply fix, verify resolution

- [ ] Verify "Cannot find -lMetal" solution works
  - **Documented in**: docs/LOCAL_BUILD.md:317-324
  - **Test**: Build on Linux, verify error and fix

- [ ] Verify RUSTC_WRAPPER solution works
  - **Documented in**: docs/LOCAL_BUILD.md:460-471
  - **Test**: Set invalid wrapper, apply fix

---

## Test Suite Validation (Priority: MEDIUM)

- [ ] Run hot-swap tests: `cargo test --test adapter_hotswap --features extended-tests`
  - **Status**: ⏳ Untested (merge conflicts fixed, compilation verified)
  - **Expected**: Tests pass
  - **Documented in**: tests/README.md:55-72

- [ ] Run determinism tests: `cargo test --test determinism_tests`
  - **Status**: ⏳ Untested
  - **Expected**: Tests pass
  - **Documented in**: tests/README.md:129-148

- [ ] Run schema tests: `cargo test -p adapteros-db schema_consistency_tests`
  - **Status**: ⏳ Untested
  - **Expected**: Tests pass
  - **Documented in**: tests/README.md:44-53

---

## Summary Statistics

**Total Items**: 35
**Verified (✅)**: 1 (3%)
**Untested (⏳)**: 31 (89%)
**Known Issues (⚠️)**: 1 (3%)
**Estimated (💭)**: 2 (6%)

**Priority Breakdown**:
- HIGH: 8 items (23%)
- MEDIUM: 13 items (37%)
- LOW: 14 items (40%)

---

## How to Contribute

Help verify these claims:

1. **Pick an item** from the checklist above
2. **Test it** on your system
3. **Update status**:
   - ✅ if it works as documented
   - ❌ if it fails (document failure)
   - ⚠️ if it partially works
4. **Submit PR** with updated checklist and any doc fixes

**Report format**:
```markdown
## Verification Report: [Item Name]

**Tested by**: [Your name]
**Date**: YYYY-MM-DD
**Platform**: macOS 14.0 / Apple M2
**Result**: ✅ Success / ❌ Failed / ⚠️ Partial

**Notes**: [Any observations, issues, or corrections needed]
```

---

**Maintained by**: James KC Auchterlonie
**Copyright**: © 2025 JKCA / James KC Auchterlonie. All rights reserved.
