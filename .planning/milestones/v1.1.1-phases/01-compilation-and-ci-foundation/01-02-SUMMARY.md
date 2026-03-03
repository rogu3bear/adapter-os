---
phase: 01-compilation-and-ci-foundation
plan: 02
subsystem: infra
tags: [sqlx, safetensors, tokio, dependencies, ci]
one-liner: "P0 dependency upgrades (sqlx 0.8.6, safetensors 0.7, tokio 1.44) with safetensors API migrations and regenerated SQLx cache"

requires:
  - phase: 01-01
    provides: clean compiling workspace baseline
provides:
  - sqlx 0.8.6, safetensors 0.7, tokio 1.44 in workspace
  - synchronized CI sqlx-cli version
  - regenerated SQLx offline cache
affects: [01-03, all-subsequent-phases]

tech-stack:
  added: []
  patterns: [safetensors serialize takes Option by value not reference, names() returns Vec<&str> directly]

key-files:
  created: []
  modified:
    - Cargo.toml
    - Cargo.lock
    - .github/workflows/ci.yml
    - crates/adapteros-db/.sqlx/

key-decisions:
  - "safetensors 0.7 API changed serialize() signature from &Option to Option -- fixed all 12 call sites"
  - "safetensors 0.7 names() still returns Vec<&str> -- removed redundant as_str() calls that triggered unstable str_as_str"

patterns-established:
  - "safetensors::serialize(..., None) not &None"
  - "tensors.names() returns Vec<&str> directly -- no map needed"

requirements-completed: [COMP-05, COMP-06]

duration: 15min
completed: 2026-02-23
---

# Plan 01-02: P0 Dependency Upgrades Summary

**sqlx 0.8.6, safetensors 0.7, tokio 1.44 applied with 12 safetensors API migration fixes and SQLx offline cache regenerated**

## Performance

- **Duration:** 15 min
- **Started:** 2026-02-23
- **Completed:** 2026-02-23
- **Tasks:** 2
- **Files modified:** 17

## Accomplishments
- sqlx upgraded from 0.8.2 to 0.8.6
- safetensors upgraded from 0.4 to 0.7 with all breaking API changes fixed
- tokio upgraded from 1.35 to 1.44
- CI sqlx-cli install version synchronized to 0.8.6 (all 3 occurrences)
- SQLx offline cache regenerated and verified

## Task Commits

1. **Task 1: Apply P0 upgrades** + **Task 2: Update CI and regen cache** - `4286c21a` (feat)

## Files Created/Modified
- `Cargo.toml` - Updated sqlx 0.8.6, safetensors 0.7, tokio 1.44
- `Cargo.lock` - Regenerated with new versions
- `.github/workflows/ci.yml` - Updated sqlx-cli install to 0.8.6
- `crates/adapteros-db/.sqlx/` - Regenerated offline cache
- 12 source files - Fixed safetensors serialize() and names() API changes

## Decisions Made
None - followed plan as specified. safetensors breaking changes were anticipated in the plan.

## Deviations from Plan

### Auto-fixed Issues

**1. safetensors 0.7 serialize() signature change**
- **Found during:** Task 1 (dependency upgrade)
- **Issue:** serialize() changed from `&Option<HashMap>` to `Option<HashMap>` -- 12 call sites used `&None`
- **Fix:** Changed all `&None` to `None` and `&Default::default()` to `Default::default()`
- **Files modified:** 12 source files across 8 crates
- **Verification:** cargo check --workspace passes

**2. safetensors 0.7 names() redundant as_str()**
- **Found during:** Task 1
- **Issue:** names() returns Vec<&str>, calling as_str() on &str triggers unstable str_as_str feature
- **Fix:** Removed .into_iter().map(|s| s.as_str()).collect() chains, use names() directly
- **Files modified:** 3 files (coreml lib, mtl lib)
- **Verification:** cargo check --workspace passes

---

**Total deviations:** 2 auto-fixed (safetensors API migration)
**Impact on plan:** Expected API migration work. No scope creep.

## Issues Encountered
None.

## User Setup Required
None.

## Next Phase Readiness
- All dependencies at target versions
- Ready for CI gate verification (Plan 01-03)

---
*Phase: 01-compilation-and-ci-foundation*
*Completed: 2026-02-23*
