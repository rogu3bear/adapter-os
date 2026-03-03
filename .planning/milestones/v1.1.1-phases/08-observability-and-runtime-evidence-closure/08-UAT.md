---
status: passed
phase: 08-observability-and-runtime-evidence-closure
source: [08-01-SUMMARY.md, 08-02-SUMMARY.md, 08-03-SUMMARY.md]
started: 2026-02-24T09:30:00Z
updated: 2026-02-24T15:10:00Z
---

## Current Test

number: 6
name: UX-05 post-closeout rectification checkpoint
expected: |
  OBS-06 and OBS-07 remain green, and UX-05 runtime fidelity/SIGTERM evidence blockers are closed with runnable proof.
awaiting: none

## Tests

### 1. Drain semantics parity
expected: Existing in-flight request completes and new request is rejected with 503.
result: passed

### 2. Unavailable-resource stream envelope
expected: Stream emits stable error event with `NO_COMPATIBLE_WORKER`, `message`, `retryable`, and `correlation_id`.
result: passed

### 3. Session-stack unavailable-worker behavior
expected: Session lock resolution path returns canonical unavailable-worker envelope, not validation drift.
result: passed

### 4. Foundation-run rehearsal on current HEAD
expected: `scripts/foundation-run.sh --no-clean --headless` completes and logs evidence to `var/evidence/phase08/foundation-run.log`.
result: passed

### 5. Live TUI/backend fidelity
expected: TUI runtime API client aligns with live backend surfaces (`/v1/adapters` and metrics endpoints) while preserving legacy fallback compatibility.
result: passed

### 6. Deferred manual observability evidence closure
expected: `/metrics` scrape, trace propagation test output, and SIGTERM drain timeline artifacts are present and linked.
result: passed (`test_graceful_shutdown_sigterm` executes and exits cleanly under `extended-tests`)

## Summary

total: 6
passed: 6
issues: 0
pending: 0
skipped: 0

## Evidence Pointers

- `var/evidence/phase08/foundation-run.log`
- `var/evidence/phase08/api-metrics.json`
- `var/evidence/phase08/api-adapters.json`
- `var/evidence/phase08/metrics.prom`
- `var/evidence/phase08/trace-propagation.txt`
- `var/evidence/phase08/sigterm-drain-rerun.txt`
- `var/evidence/phase08/endpoint-status.txt`

## Gaps

- truth: "Drain semantics reject new requests while preserving in-flight completion"
  status: resolved
  reason: "Targeted tests and suite reruns are green."
  severity: none
  test: 1
  root_cause: "N/A"
  artifacts:
    - ".planning/phases/08-observability-and-runtime-evidence-closure/08-01-SUMMARY.md"
  missing: []
  debug_session: ""

- truth: "UX-05 runtime fidelity and deferred manual observability evidence are fully closed"
  status: resolved
  reason: "TUI endpoint/client alignment was patched, lifecycle test compile/runtime blockers were removed, and SIGTERM drain evidence is now runnable and passing."
  severity: none
  test: 5
  root_cause: "N/A"
  artifacts:
    - "crates/adapteros-tui/src/app/api.rs"
    - "tests/server_lifecycle_tests.rs"
    - "var/evidence/phase08/sigterm-drain-rerun.txt"
  missing: []
  debug_session: ""
