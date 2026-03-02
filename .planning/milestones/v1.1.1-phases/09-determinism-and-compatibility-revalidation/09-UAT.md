---
status: passed
phase: 09-determinism-and-compatibility-revalidation
source: [09-01-SUMMARY.md, 09-02-SUMMARY.md, 09-03-SUMMARY.md]
started: 2026-02-24T09:30:00Z
updated: 2026-02-24T11:44:37Z
---

## Current Test

number: 6
name: FFI-05 external governance blocker acceptance
expected: |
  Determinism and OpenAI parity are fully revalidated, and FFI required-check enforcement evidence is accepted as externally blocked by repository plan restrictions.
awaiting: none

## Tests

### 1. Determinism deferred-suite replay
expected: Canonical hashing, replay harness, receipt harness, and replay API tests pass on current workspace.
result: passed

### 2. Fast-math governance confirmation
expected: `scripts/check_fast_math_flags.sh` reports no forbidden flags.
result: passed

### 3. Full OpenAI compatibility matrix rerun
expected: All listed chat/streaming/embeddings/models/error-format/spec-compliance suites run and pass.
result: passed

### 4. OpenAPI drift and export parity
expected: Drift check passes and generated OpenAPI remains aligned with committed contract.
result: passed (canonical drift fix + recheck completed)

### 5. ASAN required-check enforcement proof
expected: Branch protection required checks include ASAN context (`ffi-asan` or equivalent).
result: issue accepted (blocked by GitHub API 403 plan/visibility restriction)

### 6. Governance sign-off checkpoint
expected: Human reviewer confirms FFI-05 closure evidence is sufficient for merge-gate policy.
result: passed (external blocker accepted for milestone closeout)

## Summary

total: 6
passed: 5
issues: 1
pending: 0
skipped: 0

## Evidence Pointers

- `.planning/phases/09-determinism-and-compatibility-revalidation/09-01-SUMMARY.md`
- `.planning/phases/09-determinism-and-compatibility-revalidation/09-02-SUMMARY.md`
- `.planning/phases/09-determinism-and-compatibility-revalidation/09-03-SUMMARY.md`
- `docs/api/openapi.json`
- `var/evidence/phase09/15-ffi-asan-governance-baseline.log`
- `var/evidence/phase09/16-ffi-asan-governance-enforcement-attempt.log`

## Gaps

- truth: "Deferred replay determinism and OpenAI compatibility suites are re-proven on current workspace state"
  status: resolved
  reason: "All listed command matrices passed with evidence captured in phase summaries."
  severity: none
  test: 1
  root_cause: "N/A"
  artifacts:
    - ".planning/phases/09-determinism-and-compatibility-revalidation/09-01-SUMMARY.md"
    - ".planning/phases/09-determinism-and-compatibility-revalidation/09-02-SUMMARY.md"
  missing: []
  debug_session: ""

- truth: "ASAN required-check merge-gate enforcement is provably active"
  status: accepted
  reason: "Repository plan/visibility prevents required-status-check API read/write, so enforcement evidence cannot be finalized from this environment."
  severity: medium
  test: 5
  root_cause: "External platform limitation (HTTP 403 from GitHub branch-protection API)."
  artifacts:
    - ".planning/phases/09-determinism-and-compatibility-revalidation/09-03-SUMMARY.md"
    - "var/evidence/phase09/15-ffi-asan-governance-baseline.log"
    - "var/evidence/phase09/16-ffi-asan-governance-enforcement-attempt.log"
  missing: []
  debug_session: ""
