---
phase: 01-compilation-and-ci-foundation
plan: 01
subsystem: infra
tags: [cargo, clippy, db-refactor, sqlx, compilation]
one-liner: "Clean 85-crate compile baseline with clippy lint debt resolved and foundation CI/smoke scripts established"

requires:
  - phase: none
    provides: existing working tree with ~95 modified files from DB refactor
provides:
  - clean compiling 85-crate workspace
  - committed baseline with DB refactor landed
  - all clippy lints resolved
  - foundation scripts and CI workflow
affects: [01-02, 01-03, all-subsequent-phases]

tech-stack:
  added: []
  patterns: [pool_result() over deprecated pool(), Error::other() over Error::new(ErrorKind::Other)]

key-files:
  created:
    - .github/workflows/foundation-smoke.yml
    - crates/adapteros-db/tests/foundation_receipt_invariants_tests.rs
    - crates/adapteros-server-api/tests/adapteros_receipts_tenant_isolation_tests.rs
    - crates/adapteros-server-api/tests/foundation_dataset_determinism_tests.rs
    - docs/foundation-run.md
    - scripts/foundation-run.sh
    - scripts/foundation-smoke.sh
    - scripts/functional-path-smoke.sh
  modified:
    - tests/benchmark/benches/kernel_performance.rs
    - crates/adapteros-telemetry/src/diagnostics/writer.rs
    - crates/adapteros-tui/src/main.rs
    - crates/adapteros-lora-worker/src/deadlock.rs
    - crates/adapteros-lora-worker/src/uds_server.rs
    - crates/adapteros-orchestrator/src/training/packaging.rs
    - crates/adapteros-server-api-admin/src/lib.rs
    - crates/adapteros-server/src/main.rs
    - crates/adapteros-lora-lifecycle/src/loader.rs
    - fuzz/fuzz_targets/receipt_verification.rs
    - fuzz/fuzz_targets/trace_encoding.rs
    - crates/adapteros-cli/src/commands/chat.rs
    - crates/adapteros-service-supervisor/src/service.rs

key-decisions:
  - "Binary main.rs for TUI changed from mod re-declaration to lib crate imports to resolve dead code warnings"
  - "too_many_arguments allow attributes on UDS server constructors -- config struct refactor deferred"
  - "result_large_err crate-level allow on server-api-admin -- idiomatic axum handler pattern"

patterns-established:
  - "Use Error::other() not Error::new(ErrorKind::Other, ...) for io errors"
  - "Use struct literal with ..Default::default() not mut+reassign for RetryPolicy"
  - "Binary crates should use lib crate exports, not redeclare mod"

requirements-completed: [COMP-01, COMP-02]

duration: 25min
completed: 2026-02-23
---

# Plan 01-01: Fix Compilation Errors and Commit Baseline Summary

**85-crate workspace compiles cleanly with zero cargo check, clippy, and test-compile errors after landing DB refactor and fixing 10+ clippy lints**

## Performance

- **Duration:** 25 min
- **Started:** 2026-02-23
- **Completed:** 2026-02-23
- **Tasks:** 2
- **Files modified:** 112

## Accomplishments
- All ~95 working tree changes from DB refactor committed as clean baseline
- Fixed benchmark IoBuffers missing session_id field
- Fixed clippy io_other_error, field_reassign_with_default, collapsible_if, map_identity, let_unit_value, too_many_arguments, result_large_err, dead_code lints
- `cargo check --workspace`, `cargo clippy --workspace -- -D warnings`, and `cargo test --workspace --no-run` all pass

## Task Commits

1. **Task 1: Fix compilation errors** + **Task 2: Commit baseline** - `a279c943` (fix)

## Files Created/Modified
- `tests/benchmark/benches/kernel_performance.rs` - Added session_id: None to IoBuffers initializers
- `crates/adapteros-telemetry/src/diagnostics/writer.rs` - Error::other() replacing Error::new(ErrorKind::Other)
- `crates/adapteros-tui/src/main.rs` - Use lib crate imports instead of mod redeclaration
- `crates/adapteros-lora-worker/src/deadlock.rs` - Struct literal with ..Default::default()
- `crates/adapteros-lora-worker/src/uds_server.rs` - Allow too_many_arguments on constructors
- `crates/adapteros-orchestrator/src/training/packaging.rs` - Collapse nested if statements
- `crates/adapteros-server-api-admin/src/lib.rs` - Allow result_large_err for axum handlers
- `crates/adapteros-server/src/main.rs` - Remove let_unit_value binding
- `crates/adapteros-lora-lifecycle/src/loader.rs` - Remove map_identity
- `fuzz/fuzz_targets/receipt_verification.rs` - Use pool_result() over deprecated pool()
- `fuzz/fuzz_targets/trace_encoding.rs` - Use pool_result() over deprecated pool()
- `crates/adapteros-cli/src/commands/chat.rs` - Allow too_many_arguments
- `crates/adapteros-service-supervisor/src/service.rs` - Struct literal with ..Default::default()
- `crates/adapteros-lora-mlx-ffi/tests/resilience_tests.rs` - Added FusedKernels trait import

## Decisions Made
- TUI binary changed from `mod app` re-declaration to `use adapteros_tui::app::App` lib imports to resolve dead code false positives
- Used `#[allow(clippy::too_many_arguments)]` on UDS server constructors rather than config struct refactor (deferred to Phase 2+)
- Used crate-level `#[allow(clippy::result_large_err)]` on server-api-admin since (StatusCode, Json<T>) is idiomatic axum

## Deviations from Plan

### Auto-fixed Issues

**1. Additional clippy lints beyond the 2 documented in plan**
- **Found during:** Task 1 (compilation fixes)
- **Issue:** Plan documented 2 errors (session_id, io_other_error) but 10+ additional clippy lints surfaced as earlier crates compiled
- **Fix:** Fixed all: field_reassign_with_default, collapsible_if, map_identity, let_unit_value, too_many_arguments, result_large_err, dead_code (binary mod), deprecated pool()
- **Files modified:** 13 additional files beyond plan
- **Verification:** `cargo clippy --workspace -- -D warnings` passes with zero errors
- **Committed in:** a279c943 (part of baseline commit)

---

**Total deviations:** 1 auto-fixed (additional clippy lints)
**Impact on plan:** All fixes necessary for clippy clean pass. No scope creep.

## Issues Encountered
None beyond the additional clippy lints documented above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Clean compiling workspace ready for dependency upgrades (Plan 01-02)
- All test targets compile, ready for CI gate verification (Plan 01-03)

---
*Phase: 01-compilation-and-ci-foundation*
*Completed: 2026-02-23*
