---
phase: 50-runtime-state-hygiene
plan: 02
subsystem: infra
tags: [supervision, json, crash-detection, launchd, service-manager]

requires:
  - phase: 50-runtime-state-hygiene/01
    provides: runtime cleanup call site in config.rs and mod.rs structure
provides:
  - SupervisionState JSON struct with crash-vs-rebuild discrimination
  - Legacy key=value format migration to JSON
  - Binary mtime tracking for rebuild detection
  - JSON-based guardian restart recording with jq
  - service-manager status with crash count vs total count
affects: [boot-sequence, launchd-guardian, service-status]

tech-stack:
  added: []
  patterns: [atomic-file-write, legacy-format-migration, crash-vs-rebuild-detection]

key-files:
  created:
    - crates/adapteros-server/src/boot/supervision_state.rs
  modified:
    - crates/adapteros-server/src/boot/mod.rs
    - crates/adapteros-server/src/boot/config.rs
    - scripts/launchd/aos-launchd-ensure.sh
    - scripts/service-manager.sh

key-decisions:
  - "Supervision state update placed after logging initialization so mtime comparisons are logged"
  - "Guardian script uses jq with no-op fallback if jq is unavailable"
  - "Legacy key=value restart_count maps to total_restart_count with crash_restart_count=0 (unknown)"

patterns-established:
  - "Atomic JSON file write via tmp+rename for crash safety"
  - "Binary mtime comparison for crash-vs-rebuild discrimination"

requirements-completed: [RTH-03]

duration: 9min
completed: 2026-03-05
---

# Phase 50 Plan 02: Supervision State JSON Migration Summary

**SupervisionState JSON struct with binary mtime-based crash-vs-rebuild detection, guardian jq migration, and service-manager JSON reader**

## Performance

- **Duration:** 9 min
- **Started:** 2026-03-05T04:09:00Z
- **Completed:** 2026-03-05T04:17:50Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- SupervisionState struct with JSON serde, legacy key=value parsing, and atomic write
- Binary mtime comparison distinguishes crash restarts from dev rebuilds
- crash_restart_count resets to 0 on rebuild detection (solves inflated 311+ restart count)
- Guardian script migrated to JSON with jq, backward-compatible with legacy files
- service-manager.sh status shows "X crashes (Y total)" format with JSON/legacy fallback

## Task Commits

Each task was committed atomically:

1. **Task 1: Create SupervisionState struct** - `fc5333fec` (feat)
2. **Task 2: Migrate guardian and service-manager** - `3022ecbfb` (feat)

## Files Created/Modified
- `crates/adapteros-server/src/boot/supervision_state.rs` - SupervisionState with JSON serde, legacy migration, rebuild detection
- `crates/adapteros-server/src/boot/mod.rs` - Module declaration and re-exports
- `crates/adapteros-server/src/boot/config.rs` - update_supervision_state_on_boot call site after logging init
- `scripts/launchd/aos-launchd-ensure.sh` - JSON-based restart event recording with jq
- `scripts/service-manager.sh` - JSON supervision state reader with legacy fallback

## Decisions Made
None - followed plan as specified.

## Deviations from Plan
None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required. jq should already be available on macOS via Homebrew.

## Next Phase Readiness
- Supervision state infrastructure complete
- Next boot will automatically migrate legacy key=value files to JSON
- crash_restart_count will accurately reflect actual crashes going forward

---
*Phase: 50-runtime-state-hygiene*
*Completed: 2026-03-05*
