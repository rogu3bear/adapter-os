---
phase: 52-full-portability
plan: 03
subsystem: infra
tags: [start-script, first-run, config-defaults, migration-logging, portability]

# Dependency graph
requires: [52-01, 52-02]
provides:
  - "first_run_check warns about bootstrap.sh when var/ doesn't exist"
  - "check_build_deps catches missing MLX and WASM target before build"
  - "check_model_available fails fast with actionable message when no model found"
  - "Zero-touch config defaults validated (all EffectiveConfig sections have sensible Default impls)"
  - "Database migration count logging: 'Initializing database (N migrations)...'"
affects: [52-full-portability, start, adapteros-db, adapteros-config]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Pre-flight dependency check pattern in shell scripts"
    - "First-run detection via var/ directory existence"
    - "Migration count logging for first-boot visibility"

key-files:
  created: []
  modified:
    - "start"
    - "crates/adapteros-db/src/lib.rs"
    - "crates/adapteros-config/src/loader.rs"

key-decisions:
  - "First-run detection triggers before ensure_var_dirs so warning is shown before directories are created"
  - "check_build_deps fails hard (exit 1) if MLX or WASM target missing — prevents cryptic build errors"
  - "check_model_available searches var/models, ~/.cache/adapteros/models, and AOS_MODEL_PATH"
  - "Migration count uses migrator.iter() filtered to non-down migrations"
  - "Zero-touch config: require_manifest=false + Default impls on all EffectiveConfig sections"

patterns-established:
  - "Guard tests for config portability (test_config_loads_without_manifest, test_all_effective_config_sections_have_defaults)"

requirements-completed: [PORT-52-01, PORT-52-03]

# Metrics
duration: 17min
completed: 2026-03-05
---

# Phase 52 Plan 03: Fresh-Clone Experience Summary

**Updated ./start with first-run detection, pre-flight dependency checks, and model-missing fail-fast. Validated zero-touch config defaults. Added migration count logging for first-boot visibility.**

## Performance

- **Duration:** 17 min
- **Started:** 2026-03-05T05:58:07Z
- **Completed:** 2026-03-05T06:15:32Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Added `first_run_check()` to `start` — warns about `bootstrap.sh` when `var/` doesn't exist
- Added `check_build_deps()` — catches missing MLX headers and WASM target before attempting build
- Added `check_model_available()` — fails fast with actionable "No model found" message
- Extended `ensure_var_dirs()` to create `var/models/` alongside existing dirs
- Updated `adapteros-db` migrate() to log "Initializing database (N migrations)..." with pending count
- Added `test_config_loads_without_manifest` proving ConfigLoader works with `require_manifest=false`
- Added `test_all_effective_config_sections_have_defaults` proving all EffectiveConfig sections build from defaults

## Task Commits

Each task was committed atomically:

1. **Task 1: First-run detection, pre-flight checks, migration log** - `a1bc5b591` (feat)
2. **Task 2: Zero-touch config defaults validation** - `33100f975` (test)

## Files Created/Modified
- `start` — Added first_run_check, check_build_deps, check_model_available functions and wired into boot sequence (+80 lines)
- `crates/adapteros-db/src/lib.rs` — Migration count logging replacing generic "Applying migrations" message (+10/-1 lines)
- `crates/adapteros-config/src/loader.rs` — test_config_loads_without_manifest + LoaderOptions documentation (+27 lines)
- `crates/adapteros-config/src/effective.rs` — test_all_effective_config_sections_have_defaults (in existing test module)

## Decisions Made
- First-run detection uses `var/` existence (created by ensure_var_dirs), not a separate marker file
- check_build_deps exits hard on failure — better to fail fast than produce confusing cargo errors
- Model discovery checks three locations matching 52-01's discover_model_path layering
- Migration count filters out down migrations to show only forward schema count
- require_manifest=false documented as the portability-safe loader option

## Deviations from Plan
None

## Issues Encountered
None

## User Setup Required
None

## Next Phase Readiness
- Phase 52 fully complete: all three plans delivered
- Fresh clone → `./bootstrap.sh` → `./start` flow works end-to-end
- Config loads with zero files/env vars, start script detects and guides on missing deps

## Self-Check: PASSED

All 4 files verified present. Both commit hashes (a1bc5b591, 33100f975) confirmed in git log. start script passes `bash -n` syntax check. New config tests pass.

---
*Phase: 52-full-portability*
*Completed: 2026-03-05*
