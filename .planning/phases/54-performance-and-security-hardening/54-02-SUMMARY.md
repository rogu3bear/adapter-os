---
phase: 54-performance-and-security-hardening
plan: 02
subsystem: security
tags: [rate-limiting, security-audit, secret-scanner, debug-redaction, contract-checks]

# Dependency graph
requires:
  - phase: 54-01
    provides: "RateLimitsConfig base struct and effective config wiring"
provides:
  - "Per-tier RPM rate limiting (health/public/internal/protected)"
  - "SecurityConfig custom Debug impl with jwt_secret redaction"
  - "check_security_audit.sh: auth, rate-limit, CSRF, input validation checks"
  - "check_secret_exposure.sh: hardcoded secret scanner with Debug-derive analysis"
affects: [api-security, ci-pipeline, operator-tooling]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Per-tier rate limiting: RouteTier enum classifies paths, tier_rpm_override passed to check_rate_limit"
    - "Custom Debug for sensitive structs: remove derive(Debug), impl manually with [REDACTED] for secrets"
    - "Contract check script pattern: count_matches helper for pipefail-safe rg -c | awk pipelines"

key-files:
  created:
    - "scripts/contracts/check_security_audit.sh"
    - "scripts/contracts/check_secret_exposure.sh"
  modified:
    - "crates/adapteros-config/src/types.rs"
    - "crates/adapteros-config/src/effective.rs"
    - "crates/adapteros-server-api/src/middleware_security.rs"
    - "crates/adapteros-server-api/src/security/rate_limiting.rs"
    - "crates/adapteros-server-api/tests/security_feature_tests.rs"
    - "configs/cp.toml"
    - "scripts/contracts/check_all.sh"

key-decisions:
  - "Tightened RATE_LIMIT_EXEMPT_PATHS: removed /v1/system/, /v1/models/, /v1/plans (mutation routes must be rate-limited)"
  - "Health tier bypasses rate limiting entirely (probes must never be throttled)"
  - "Per-tier override is Optional<u32> with fallback to global requests_per_minute"
  - "SecurityConfig redacts both jwt_secret and jwt_additional_hmac_secrets in Debug"
  - "Secret scanner excludes jwt_secret TOML config assignments (false positives in embedded config strings)"

patterns-established:
  - "RouteTier classification: Health/Public/Internal/Protected based on path prefix"
  - "Pipefail-safe count_matches() helper for contract scripts using rg + awk"

requirements-completed: [SEC-54-01, SEC-54-02]

# Metrics
duration: 14min
completed: 2026-03-05
---

# Phase 54 Plan 02: Per-Tier Rate Limiting and Security Audit Summary

**Per-tier RPM rate limiting (public=600, internal=1000, protected=300), SecurityConfig debug redaction, and two contract-check scripts covering auth enforcement, secret exposure, and input validation**

## Performance

- **Duration:** 14 min
- **Started:** 2026-03-05T06:36:02Z
- **Completed:** 2026-03-05T06:50:20Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments
- Extended RateLimitsConfig with per-tier RPM fields (health/public/internal/protected) wired through effective config
- Added RouteTier enum and route_tier() classifier in middleware; health tier unconditionally bypasses rate limiting
- Tightened RATE_LIMIT_EXEMPT_PATHS by removing overly broad /v1/system/, /v1/models/, /v1/plans exemptions
- Implemented custom Debug for SecurityConfig that redacts jwt_secret and hmac_secrets
- Created 192-line security audit script (auth, dev bypass, rate limiting, CSRF, input validation, cargo-audit)
- Created 209-line secret exposure scanner (hardcoded secrets, Debug-derived sensitive structs, config logging)

## Task Commits

Each task was committed atomically:

1. **Task 1: Per-tier rate limiting config and middleware** - `8cc987ff9` (feat)
2. **Task 2: Security audit and secret exposure scripts** - `5afc86e4f` (feat)

## Files Created/Modified
- `crates/adapteros-config/src/types.rs` - Per-tier RPM fields in RateLimitsConfig, custom Debug for SecurityConfig
- `crates/adapteros-config/src/effective.rs` - Per-tier fields in RateLimitsSection, build_rate_limits_section wiring
- `crates/adapteros-server-api/src/middleware_security.rs` - RouteTier enum, route_tier(), tier-aware rate limiting
- `crates/adapteros-server-api/src/security/rate_limiting.rs` - tier_rpm_override parameter in check_rate_limit
- `crates/adapteros-server-api/tests/security_feature_tests.rs` - Updated check_rate_limit call sites
- `configs/cp.toml` - Per-tier RPM defaults (public=600, internal=1000, protected=300)
- `scripts/contracts/check_security_audit.sh` - Security audit contract check
- `scripts/contracts/check_secret_exposure.sh` - Secret exposure scanner
- `scripts/contracts/check_all.sh` - Registered both new scripts

## Decisions Made
- Tightened RATE_LIMIT_EXEMPT_PATHS: removed /v1/system/, /v1/models/, /v1/plans because mutation routes on those paths must not bypass rate limiting
- Health tier unconditionally bypasses rate limiting -- health probes must never be throttled
- Per-tier override is `Option<u32>` with fallback to global `requests_per_minute` for backward compatibility
- SecurityConfig redacts both `jwt_secret` and `jwt_additional_hmac_secrets` in Debug impl
- Secret scanner excludes `jwt_secret` TOML config field assignments to avoid false positives from embedded config strings in test/config code
- Narrowed sensitive struct name check to Secret/Password only (Token/Key too noisy -- legitimate structs like ApiKeyConfig)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed pipefail-incompatible rg | awk patterns in scripts**
- **Found during:** Task 2 (script execution)
- **Issue:** `rg -c | awk` pipelines fail under `set -euo pipefail` when rg returns exit 1 (no matches)
- **Fix:** Wrapped in `count_matches()` helper that runs the pipeline in a subshell with `|| true`
- **Files modified:** scripts/contracts/check_security_audit.sh, scripts/contracts/check_secret_exposure.sh
- **Verification:** Both scripts exit 0 on current codebase
- **Committed in:** 5afc86e4f (Task 2 commit)

**2. [Rule 3 - Blocking] Updated check_rate_limit call sites in test file**
- **Found during:** Task 1 (signature change)
- **Issue:** `check_rate_limit` signature gained `tier_rpm_override` parameter; 8 call sites in security_feature_tests.rs needed updating
- **Fix:** Added `None` as third argument to all test call sites
- **Files modified:** crates/adapteros-server-api/tests/security_feature_tests.rs
- **Verification:** cargo check -p adapteros-server-api passes
- **Committed in:** 8cc987ff9 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 bug, 1 blocking)
**Impact on plan:** Both fixes necessary for correctness. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Per-tier rate limiting ready for production tuning via cp.toml
- Security audit and secret exposure scripts ready for CI integration
- Plan 03 can proceed independently

---
*Phase: 54-performance-and-security-hardening*
*Completed: 2026-03-05*
