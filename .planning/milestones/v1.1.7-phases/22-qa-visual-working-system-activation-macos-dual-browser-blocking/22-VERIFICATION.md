---
phase: "22"
name: "QA Visual Working System Activation (macOS Dual-Browser Blocking)"
created: 2026-02-26
verified: "2026-02-26T21:10:00Z"
status: passed_with_risk
score: 5/5 requirements verified
verifier: gsd-full-suite
---

# Phase 22: QA Visual Working System Activation (macOS Dual-Browser Blocking) — Verification

## Goal-Backward Verification

**Phase Goal:** Stabilize and enforce a deterministic, truthful dual-browser macOS visual QA gate using existing Playwright infrastructure.

## Checks

| # | Requirement | Status | Evidence |
|---|------------|--------|----------|
| 1 | QA-22-01 dual-browser blocking gate command contract | VERIFIED | `.github/workflows/ci.yml` (`playwright-ui-quality-gate-{chromium,webkit}` on `macos-14` with `npm run test:gate:quality -- --project=<browser>`) |
| 2 | QA-22-02 explicit bundled suite command contract | VERIFIED | `tests/playwright/package.json` (`test:audit`, `test:gate:quality` with bundled spec list) |
| 3 | QA-22-03 snapshot contract (missing/orphan guard + darwin canonical) | VERIFIED | `tests/playwright/scripts/check-visual-snapshot-contract.mjs`; command output `OK: 2 active screenshots, 9 total references, 10 baseline files (darwin canonical)` |
| 4 | DOC-22-01 documentation/planning evidence completeness | VERIFIED | `README.md` QA gate section + `tests/playwright/README.md` blocking gate/baseline policy + Phase 22 planning artifacts |
| 5 | AUTO-04 autopilot continuity | VERIFIED | `.planning/config.json` (`quality`, `auto_advance=true`, `plan_check=true`, `verifier=true`, `max_concurrent_agents=2`) |

## Validation Commands

1. `rg -n 'playwright-ui-quality-gate-(chromium|webkit)|runs-on: macos-14|npm run test:gate:quality -- --project=' .github/workflows/ci.yml`
2. `rg -n '"test:audit"|"test:gate:quality"|console\.regression|routes\.best_practices\.audit|ui/visual\.spec\.ts|ui/runs\.spec\.ts|ui/repositories\.spec\.ts' tests/playwright/package.json`
3. `cd tests/playwright && node scripts/check-visual-snapshot-contract.mjs`
4. `cd tests/playwright && PW_RUN_ID=phase22-gate-list-chromium npm run test:gate:quality -- --project=chromium --list`
5. `cd tests/playwright && PW_RUN_ID=phase22-gate-list-webkit npm run test:gate:quality -- --project=webkit --list`
6. `rg -n 'QA Visual Gate|test:gate:quality|canonical baselines|tests/playwright/README.md|darwin' README.md tests/playwright/README.md`

## Required Artifacts

| Artifact | Expected | Status |
|----------|----------|--------|
| `var/playwright/runs/phase22-gate-list-chromium/heartbeat.json` | Chromium gate run-scoped heartbeat | VERIFIED |
| `var/playwright/runs/phase22-gate-list-chromium/report/index.html` | Chromium gate run-scoped report artifact | VERIFIED |
| `var/playwright/runs/phase22-gate-list-webkit/heartbeat.json` | WebKit gate run-scoped heartbeat | VERIFIED |
| `var/playwright/runs/phase22-gate-list-webkit/report/index.html` | WebKit gate run-scoped report artifact | VERIFIED |
| `.github/workflows/ci.yml` | Dual-browser blocking quality jobs on macOS | VERIFIED |
| `tests/playwright/scripts/check-visual-snapshot-contract.mjs` | Missing/orphan guard with darwin canonical policy | VERIFIED |

## Gap Summary

- Full assertion execution for the bundled lane was not run in this pass (list-mode selector truth was used to stay targeted).
- `test-results/` and debug-trace outputs for the two run IDs above were not generated because `--list` does not execute tests.

## Result

Phase 22 verification passes for command contract integrity, selector truth, and snapshot policy enforcement, with residual risk that full assertion execution still requires CI or explicit local full gate runs.
