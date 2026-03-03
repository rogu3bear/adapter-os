---
phase: "23"
name: "QA Visual Full Blocking Run and Deterministic Closure"
created: 2026-02-26
verified: "2026-02-26T03:32:39Z"
status: gaps_found
score: 1/3 requirements verified
verifier: gsd-full-suite
---

# Phase 23: QA Visual Full Blocking Run and Deterministic Closure — Verification

## Goal-Backward Verification

**Phase Goal:** Execute full dual-browser bundled quality lane assertions and close residual risk from list-mode validation.

## Checks

| # | Requirement | Status | Evidence |
|---|------------|--------|----------|
| 1 | QA-23-01 Chromium full run | GAP | `var/playwright/runs/phase23-fix4-chromium/debug/global-setup-summary.json` (`success=true`) and `test-results/.last-run.json` (`status=failed`, 5 failed tests) prove full assertion execution but lane does not pass. |
| 2 | QA-23-02 WebKit full run | GAP | `var/playwright/runs/phase23-fix4-webkit/debug/global-setup-summary.json` (`success=true`) and `test-results/.last-run.json` (`status=failed`, 6 failed tests) prove full assertion execution but lane does not pass. |
| 3 | DOC-23-01 full-run evidence mapping | VERIFIED | Verification/UAT cite concrete report/debug/test-results paths for both browsers and list concrete failing specs. |

## Validation Commands

1. `cargo check -p adapteros-server`
2. `cargo build -p adapteros-server`
3. `cd tests/playwright && npx playwright install chromium webkit`
4. `cd tests/playwright && PW_RUN_ID=phase23-fix4-chromium npm run test:gate:quality -- --project=chromium`
5. `cd tests/playwright && PW_RUN_ID=phase23-fix4-webkit npm run test:gate:quality -- --project=webkit`
6. `for rid in phase23-fix4-chromium phase23-fix4-webkit; do cat var/playwright/runs/$rid/debug/global-setup-summary.json; cat var/playwright/runs/$rid/test-results/.last-run.json; done`

## Required Artifacts

| Artifact | Expected | Status |
|----------|----------|--------|
| `var/playwright/runs/phase23-fix4-chromium/heartbeat.json` | Chromium run heartbeat | VERIFIED |
| `var/playwright/runs/phase23-fix4-chromium/report/index.html` | Chromium run report | VERIFIED |
| `var/playwright/runs/phase23-fix4-chromium/debug/global-setup-summary.json` | Chromium setup status | VERIFIED (`success=true`) |
| `var/playwright/runs/phase23-fix4-chromium/test-results/.last-run.json` | Chromium run status file | VERIFIED (`failed`, 5 failing specs) |
| `var/playwright/runs/phase23-fix4-webkit/heartbeat.json` | WebKit run heartbeat | VERIFIED |
| `var/playwright/runs/phase23-fix4-webkit/report/index.html` | WebKit run report | VERIFIED |
| `var/playwright/runs/phase23-fix4-webkit/debug/global-setup-summary.json` | WebKit setup status | VERIFIED (`success=true`) |
| `var/playwright/runs/phase23-fix4-webkit/test-results/.last-run.json` | WebKit run status file | VERIFIED (`failed`, 6 failing specs) |

## Gap Summary

- Seed/global-setup blocker is rectified: both browser runs now complete `global-setup` (`success=true`) and execute assertions.
- Bundled lane still fails on deterministic spec-level regressions:
  - Route audit timeout on `/settings` and `/user`.
  - `runs.spec.ts` failures (`Flight Recorder` heading not visible; expected `Chat unavailable` copy absent).
  - Visual diffs: Chromium (`adapters list`), WebKit (`training detail`, `adapters list`).
- Phase goal remains partially met: truthful dual-browser execution is restored, but merge-blocking suite pass is not yet achieved.

## Result

Phase 23 execution remains `gaps_found`: deterministic full-run execution is now healthy, but blocking quality regressions in route-audit/detail-flow/visual assertions still prevent closure.
