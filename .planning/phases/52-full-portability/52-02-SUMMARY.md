---
phase: 52-full-portability
plan: 02
subsystem: infra
tags: [bootstrap, shell, mlx, wasm, developer-experience]

requires:
  - phase: none
    provides: standalone plan
provides:
  - ".adapteros-root marker file for project root detection"
  - "bootstrap.sh idempotent dependency installer for fresh clones"
affects: [52-01-path-utils, 52-03-env-config]

tech-stack:
  added: []
  patterns: [idempotent-shell-installer, project-root-marker]

key-files:
  created:
    - ".adapteros-root"
    - "bootstrap.sh"
  modified: []

key-decisions:
  - "Used `mlx` formula instead of deprecated `ml-explore/mlx/mlx` tap (tap repo removed from GitHub)"
  - "Simple for-loop arg parsing instead of getopts -- script has exactly two optional flags"

patterns-established:
  - "Project root marker: empty .adapteros-root file at repo root for find_project_root()"
  - "Idempotent install: check-before-install pattern for brew pkgs, rustup targets, cargo tools"

requirements-completed: [PORT-52-01]

duration: 2min
completed: 2026-03-05
---

# Phase 52 Plan 02: Bootstrap & Root Marker Summary

**Idempotent bootstrap.sh installer (mlx, wasm32 target, wasm-bindgen, trunk) and .adapteros-root project root marker for fresh-clone-to-build workflow**

## Performance

- **Duration:** 2 min
- **Started:** 2026-03-05T05:37:39Z
- **Completed:** 2026-03-05T05:39:44Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Empty `.adapteros-root` marker file committed for `find_project_root()` detection from any subdirectory
- `bootstrap.sh` with fail-fast prerequisite checks (brew, rustup, cargo) and idempotent dependency installation
- `--verify` flag chains `cargo check --workspace` and `scripts/ui-check.sh` after dependency setup
- Corrected MLX formula name from deprecated `ml-explore/mlx/mlx` tap to `mlx` (homebrew-mlx repo removed)

## Task Commits

Each task was committed atomically:

1. **Task 1: Create .adapteros-root marker file** - `4320fbb6f` (chore)
2. **Task 2: Create idempotent bootstrap.sh dependency installer** - `f8911eb90` (feat)

## Files Created/Modified
- `.adapteros-root` - Empty project root marker for path detection
- `bootstrap.sh` - Idempotent dependency installer (177 lines, executable)

## Decisions Made
- Used `mlx` formula instead of deprecated `ml-explore/mlx/mlx` tap -- the homebrew-mlx repo was removed from GitHub, but the `mlx` formula is available directly in homebrew-core
- Simple `for arg in "$@"` + `case` pattern for arg parsing -- only two flags (`--verify`, `--help`), no need for getopts

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed MLX brew formula name**
- **Found during:** Task 2 (bootstrap.sh creation)
- **Issue:** Plan specified `ml-explore/mlx/mlx` as the brew package, but the `homebrew-mlx` tap repo was removed from GitHub. `brew install ml-explore/mlx/mlx` fails with "repository not found"
- **Fix:** Changed to `brew install mlx` which is the correct formula in homebrew-core
- **Files modified:** `bootstrap.sh`
- **Verification:** Ran `bootstrap.sh` successfully -- all deps show `[ok]`
- **Committed in:** f8911eb90 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug fix)
**Impact on plan:** Essential fix -- script would fail on every run without this correction. No scope creep.

## Issues Encountered
None beyond the MLX formula name fix documented above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `.adapteros-root` marker is ready for `find_project_root()` in Plan 01 (path_utils)
- `bootstrap.sh` can be referenced in project README for onboarding instructions
- Plan 03 (env-config) can proceed independently

## Self-Check: PASSED

All files exist. All commits verified.

---
*Phase: 52-full-portability*
*Completed: 2026-03-05*
