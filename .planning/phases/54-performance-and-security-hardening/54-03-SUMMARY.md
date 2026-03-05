---
phase: 54-performance-and-security-hardening
plan: 03
subsystem: performance-security
tags: [eviction-notifications, sse, ui-toast, permissions, security-audit]

# Dependency graph
requires:
  - phase: 54-01
    provides: "UMA memory budget + eviction policy baseline"
  - phase: 54-02
    provides: "Security middleware baseline + contract scripts"
provides:
  - "Live adapter eviction notifications from backend alerts stream to UI warning toast"
  - "Structured auth/rate/input audit events for security trail"
  - "Boot-time model weight permission hardening for var/models (0700 dirs, 0600 files)"
  - "Security contract checks updated for structured events and model permissions"
affects: [api-security, memory-lifecycle, ui-observability, boot-hardening]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Broadcast channel in AppState for lifecycle event fanout into alerts SSE"
    - "Structured tracing event fields (event/source_ip/endpoint/reason/auth_mode)"
    - "Fail-safe permission hardening at boot with warning-on-failure behavior"

key-files:
  created:
    - ".planning/phases/54-performance-and-security-hardening/54-03-SUMMARY.md"
    - ".planning/phases/54-performance-and-security-hardening/54-VERIFICATION.md"
  modified:
    - "crates/adapteros-server-api/src/sse/lifecycle_events.rs"
    - "crates/adapteros-server-api/src/state.rs"
    - "crates/adapteros-server-api/src/handlers/adapter_lifecycle.rs"
    - "crates/adapteros-server-api/src/handlers/streams/mod.rs"
    - "crates/adapteros-server-api/src/middleware/mod.rs"
    - "crates/adapteros-server-api/src/middleware_security.rs"
    - "crates/adapteros-server-api/src/handlers/streaming_infer.rs"
    - "crates/adapteros-ui/src/api/types.rs"
    - "crates/adapteros-ui/src/api/sse.rs"
    - "crates/adapteros-ui/src/signals/refetch.rs"
    - "crates/adapteros-server/src/boot/app_state.rs"
    - "crates/adapteros-server/src/boot/database.rs"
    - "crates/adapteros-server/src/boot/runtime.rs"
    - "scripts/contracts/check_security_audit.sh"

key-decisions:
  - "Hook eviction notifications into the existing alerts stream in `crates/adapteros-server-api/src/handlers/streams/mod.rs` rather than creating a parallel SSE path."
  - "Emit structured auth failure events in middleware auth flow (`crates/adapteros-server-api/src/middleware/mod.rs`) to ensure path/mode visibility for all auth branches."
  - "Keep model permission enforcement fail-safe: set permissions at boot, warn on failure, and enforce via contract script."

patterns-established:
  - "Memory lifecycle events can be pushed immediately over alerts SSE while keeping periodic alert polling intact."
  - "Security contract scripts gate both static posture checks and runtime filesystem permission checks."

requirements-completed: [PERF-54-02, SEC-54-02]

# Metrics
duration: ~2h (implementation + verification + planning reconciliation)
completed: 2026-03-05
---

# Phase 54 Plan 03: Eviction Notifications, Permission Hardening, and Audit Trail Summary

**Completed end-to-end eviction visibility (alerts SSE + UI warning toast), structured security audit events, and model weight permission hardening with passing targeted checks.**

## Performance

- **Completed:** 2026-03-05
- **Tasks:** 2 (implementation + verification/reconciliation)
- **Files modified:** 17 code/script files + planning closeout artifacts

## Accomplishments

- Added `AdapterEvicted` system health event + `MemoryEvictionEvent` transport type.
- Added `memory_eviction_tx` broadcast sender to `AppState`, initialized at boot.
- Emitted memory eviction event from adapter unload path and forwarded it through alerts SSE.
- Updated UI alerts SSE handling to parse eviction events, trigger warning toast, and refetch health/model state.
- Added structured security logging for auth failures, rate-limit hits, and input validation rejects.
- Added recursive boot-time permission hardening for `var/models` (`0700` dirs, `0600` files).
- Extended `check_security_audit.sh` to validate structured event logging and model permission posture.

## Task Commits

No new commits were created during this execution pass; changes are present in the working tree and validated by targeted checks.

## Files Created/Modified

- `crates/adapteros-server-api/src/sse/lifecycle_events.rs` - Added eviction event variants.
- `crates/adapteros-server-api/src/state.rs` - Added `memory_eviction_tx` channel.
- `crates/adapteros-server-api/src/handlers/adapter_lifecycle.rs` - Emits eviction event on unload.
- `crates/adapteros-server-api/src/handlers/streams/mod.rs` - Alerts stream now forwards live eviction events.
- `crates/adapteros-ui/src/api/types.rs` - Added `AdapterEvicted` transition event.
- `crates/adapteros-ui/src/api/sse.rs` - Alerts SSE subscription + warning toast dispatch.
- `crates/adapteros-ui/src/signals/refetch.rs` - Refetch models/health on eviction event.
- `crates/adapteros-server-api/src/middleware/mod.rs` - Structured auth failure logs across auth branches.
- `crates/adapteros-server-api/src/middleware_security.rs` - Structured rate/input security logs.
- `crates/adapteros-server-api/src/handlers/streaming_infer.rs` - Transparent reload behavior comment.
- `crates/adapteros-server/src/boot/app_state.rs` - Recursive model permission hardening.
- `crates/adapteros-server/src/boot/database.rs` - Rate-limits config compatibility fields.
- `crates/adapteros-server/src/boot/runtime.rs` - Rate-limits config compatibility fields.
- `scripts/contracts/check_security_audit.sh` - Added structured event and permission checks.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Optional-auth compile regression (missing `path` binding)**
- **Found during:** `cargo check -p adapteros-server-api`
- **Issue:** two `validate_access_token_with_session(...)` calls in optional auth branch used `path` without a local binding.
- **Fix:** added a single `let path = req.uri().path().to_string();` in optional auth scope and removed duplicate bindings introduced during patching.
- **Verification:** `cargo check -p adapteros-server-api` passes.

**2. [Rule 3 - Blocking] RateLimitsConfig initializer drift in server boot**
- **Found during:** `cargo check -p adapteros-server`
- **Issue:** `RateLimitsConfig` gained `health_rpm/public_rpm/internal_rpm/protected_rpm`; three boot initializers were missing fields.
- **Fix:** populated new fields (`None` for synthetic/default configs, propagated configured values where available).
- **Verification:** `cargo check -p adapteros-server` passes.

---

**Total deviations:** 2 auto-fixed (1 bug, 1 blocking)
**Impact on plan:** No scope creep; both fixes were required for build correctness and verification closure.

## Issues Encountered

- Initial security audit run failed on local `var/models` file modes; corrected permissions and re-ran successfully.

## User Setup Required

None. Model permission checks are now enforced by both boot hardening and contract script.

## Next Phase Readiness

Phase 54 is complete (3/3 plans). Planning artifacts were reconciled to reflect milestone closure and requirement traceability.
