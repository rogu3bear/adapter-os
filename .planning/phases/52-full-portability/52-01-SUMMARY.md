---
phase: 52-full-portability
plan: 01
subsystem: infra
tags: [path-resolution, portability, model-discovery, project-root]

# Dependency graph
requires: []
provides:
  - "find_project_root with .adapteros-root marker and AOS_ROOT override"
  - "Relative DEV_MODEL_PATH (portability bug fix)"
  - "discover_model_path with layered resolution (var/models > ~/.cache/adapteros/models)"
affects: [52-full-portability, adapteros-config, adapteros-core]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Layered model discovery: ENV override > project-local var/models > shared user cache"
    - "Project root detection via marker walk (.adapteros-root > Cargo.lock > .git)"
    - "Guard tests preventing absolute /var/ paths in defaults"

key-files:
  created: []
  modified:
    - "crates/adapteros-core/src/path_utils.rs"
    - "crates/adapteros-core/src/defaults.rs"
    - "crates/adapteros-core/src/lib.rs"
    - "crates/adapteros-config/src/path_resolver.rs"
    - "crates/adapteros-config/Cargo.toml"
    - "crates/adapteros-config/src/model.rs"

key-decisions:
  - "AOS_ROOT env var takes absolute priority over marker walk for project root detection"
  - "Marker check order: .adapteros-root > Cargo.lock > .git (most specific first)"
  - "discover_model_path integrated as fallback in resolve_model_path between config and dev fixture"

patterns-established:
  - "Layered model discovery: AOS_MODEL_PATH override > var/models/{id} > ~/.cache/adapteros/models/{id}"
  - "Guard test pattern: assert no defaults use absolute system paths"

requirements-completed: [PORT-52-02]

# Metrics
duration: 12min
completed: 2026-03-05
---

# Phase 52 Plan 01: Path Resolution and Model Discovery Summary

**Portable project root detection via .adapteros-root marker with AOS_ROOT override, fixed DEV_MODEL_PATH absolute path bug, and layered model discovery searching var/models then ~/.cache/adapteros/models**

## Performance

- **Duration:** 12 min
- **Started:** 2026-03-05T05:38:12Z
- **Completed:** 2026-03-05T05:51:05Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- Hardened project root detection with `.adapteros-root` marker, `Cargo.lock`, `.git` in priority order, plus `AOS_ROOT` env var override
- Fixed portability bug: `DEV_MODEL_PATH` changed from absolute `/var/models/Qwen3.5-27B` to relative `var/models/Qwen3.5-27B`
- Added `discover_model_path` function with layered resolution: AOS_MODEL_PATH override > project-local var/models > shared user cache ~/.cache/adapteros/models
- Integrated layered discovery into `resolve_model_path` as fallback before dev fixture
- Added guard test preventing future absolute `/var/` paths in defaults constants

## Task Commits

Each task was committed atomically:

1. **Task 1: Harden project root detection and fix DEV_MODEL_PATH** - `703cb25e8` (feat)
2. **Task 2: Add layered model discovery to path resolver** - `30eb2a2eb` (feat)

## Files Created/Modified
- `crates/adapteros-core/src/path_utils.rs` - Renamed repo_root_from to pub find_project_root with .adapteros-root marker and AOS_ROOT override, added unit tests
- `crates/adapteros-core/src/defaults.rs` - Fixed DEV_MODEL_PATH from absolute to relative, added guard test
- `crates/adapteros-core/src/lib.rs` - Re-exported find_project_root
- `crates/adapteros-config/src/path_resolver.rs` - Added discover_model_path with layered resolution, integrated into resolve_model_path, added tests
- `crates/adapteros-config/Cargo.toml` - Added adapteros-storage dependency for PlatformUtils::aos_user_cache_dir
- `crates/adapteros-config/src/model.rs` - Fixed validate() to match both relative and rebased dev placeholder paths

## Decisions Made
- AOS_ROOT env var takes absolute priority over marker walk (consistent with ENV > default precedence)
- Marker check order: .adapteros-root > Cargo.lock > .git (most specific first, tarballs without .git still work)
- discover_model_path checks AOS_MODEL_PATH first (explicit override), then local var/models, then shared cache -- consistent with existing ENV > defaults precedence

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed model validate() dev placeholder comparison after DEV_MODEL_PATH change**
- **Found during:** Task 1 (fixing DEV_MODEL_PATH)
- **Issue:** ModelConfig::validate() compared self.path against raw PathBuf::from(DEV_MODEL_PATH). After changing DEV_MODEL_PATH to relative, from_env() stores the rebased (absolute) path, which no longer matches the raw relative placeholder. This would cause validate() to reject the dev fixture path in debug builds.
- **Fix:** Added rebase_var_path(DEV_MODEL_PATH) as additional comparison target in validate()
- **Files modified:** crates/adapteros-config/src/model.rs
- **Verification:** cargo test -p adapteros-config -- --test-threads=1 passes
- **Committed in:** 703cb25e8 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Essential for correctness after DEV_MODEL_PATH change. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Path resolution hardened, ready for bootstrap.sh (plan 02) and start script improvements (plan 03)
- .adapteros-root marker already committed to repo root (from prior commit 4320fbb6f)
- All tests pass, no clippy warnings

## Self-Check: PASSED

All 7 files verified present. Both commit hashes (703cb25e8, 30eb2a2eb) confirmed in git log.

---
*Phase: 52-full-portability*
*Completed: 2026-03-05*
