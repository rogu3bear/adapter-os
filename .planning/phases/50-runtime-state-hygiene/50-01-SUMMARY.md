---
phase: 50-runtime-state-hygiene
plan: 01
subsystem: infra
tags: [unix-sockets, boot, cleanup, runtime-state]

requires:
  - phase: none
    provides: standalone module
provides:
  - Boot-time stale UDS socket cleanup with liveness probing
  - system_ready and training-worker.degraded marker removal on boot
  - Heartbeat file cleanup alongside stale sockets
affects: [boot-sequence, service-binding, health-checks]

tech-stack:
  added: []
  patterns: [best-effort-cleanup, unix-socket-liveness-probing]

key-files:
  created:
    - crates/adapteros-server/src/boot/runtime_cleanup.rs
  modified:
    - crates/adapteros-server/src/boot/mod.rs
    - crates/adapteros-server/src/boot/config.rs

key-decisions:
  - "Cleanup runs before logging initialization — tracing macros are no-ops but cleanup is silent-before-logging by design"
  - "Socket liveness check uses UnixStream::connect — simple, no external dependencies"

patterns-established:
  - "Best-effort boot cleanup: warn and continue, never block boot"

requirements-completed: [RTH-01, RTH-02]

duration: 8min
completed: 2026-03-05
---

# Phase 50 Plan 01: Stale Socket and Marker Cleanup Summary

**Boot-time cleanup of stale UDS sockets via UnixStream liveness probing, plus system_ready and degraded marker removal**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-05T04:00:47Z
- **Completed:** 2026-03-05T04:09:00Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- New runtime_cleanup module with 5 known socket definitions and heartbeat associations
- UnixStream::connect-based liveness probing preserves live sockets while removing stale ones
- system_ready and training-worker.degraded markers cleared on every boot
- Cleanup wired into boot Phase 1 before config loading or service binding

## Task Commits

Each task was committed atomically:

1. **Task 1: Create runtime_cleanup module** - `e88a7602e` (feat)
2. **Task 2: Wire cleanup into boot Phase 1** - `d988306d4` (feat)

## Files Created/Modified
- `crates/adapteros-server/src/boot/runtime_cleanup.rs` - Stale socket cleanup with liveness probing and marker removal
- `crates/adapteros-server/src/boot/mod.rs` - Module declaration and re-export
- `crates/adapteros-server/src/boot/config.rs` - Call site in initialize_config after ensure_runtime_dir

## Decisions Made
None - followed plan as specified.

## Deviations from Plan
None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Runtime cleanup foundation is in place for Plan 50-02 (supervision state) to build on
- Both modules share mod.rs and config.rs call sites

---
*Phase: 50-runtime-state-hygiene*
*Completed: 2026-03-05*
