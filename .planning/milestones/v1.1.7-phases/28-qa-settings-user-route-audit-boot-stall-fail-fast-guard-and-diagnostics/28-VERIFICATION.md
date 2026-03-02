---
phase: "28"
name: "QA Settings/User Route-Audit Boot Stall Fail-Fast Guard and Diagnostics"
created: 2026-02-26
verified: "2026-02-26T07:45:00Z"
status: gaps_found
score: 1/3 requirements verified
verifier: gsd-full-suite
---

# Phase 28: QA Settings/User Route-Audit Boot Stall Fail-Fast Guard and Diagnostics — Verification

## Goal-Backward Verification

**Phase Goal:** Convert route boot stalls into explicit fail-fast diagnostics and verify whether `/settings` + `/user` debt can be retired.

## Checks

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 1 | QA-23-01 (Chromium unskip route pair passes) | NOT VERIFIED | `phase28-fix1-chromium` failed both `/settings` and `/user` (`var/playwright/runs/phase28-fix1-chromium/test-results/.last-run.json`). |
| 2 | QA-23-02 (WebKit unskip route pair passes) | NOT VERIFIED | `phase28-fix1-webkit` failed both `/settings` and `/user` (`var/playwright/runs/phase28-fix1-webkit/test-results/.last-run.json`). |
| 3 | DOC-23-01 (evidence completeness) | VERIFIED | Both run roots include setup summaries, reports, traces, videos, and status files under `var/playwright/runs/phase28-fix1-{chromium,webkit}/...`. |

## Validation Commands

1. `cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase28-fix1-chromium npm run test:ui -- --project=chromium ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /(settings|user)"`
2. `cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase28-fix1-webkit npm run test:ui -- --project=webkit ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /(settings|user)"`

## Required Artifacts

| Artifact | Expected | Status |
|----------|----------|--------|
| `var/playwright/runs/phase28-fix1-chromium/debug/global-setup-summary.json` | setup health | VERIFIED (`success=true`) |
| `var/playwright/runs/phase28-fix1-webkit/debug/global-setup-summary.json` | setup health | VERIFIED (`success=true`) |
| `var/playwright/runs/phase28-fix1-chromium/test-results/.last-run.json` | run status | VERIFIED (`failed`) |
| `var/playwright/runs/phase28-fix1-webkit/test-results/.last-run.json` | run status | VERIFIED (`failed`) |
| `var/playwright/runs/phase28-fix1-chromium/report/index.html` | report surface | VERIFIED |
| `var/playwright/runs/phase28-fix1-webkit/report/index.html` | report surface | VERIFIED |
| `var/playwright/runs/phase28-fix1-chromium/test-results/routes.best_practices.audit-best-practices-audit-settings-chromium/trace.zip` | route trace | VERIFIED |
| `var/playwright/runs/phase28-fix1-webkit/test-results/routes.best_practices.audit-best-practices-audit-settings-webkit/trace.zip` | route trace | VERIFIED |

## Root-Cause Signals

- Both browsers now fail in `assertBestPractices(...)` with heading-readiness poll timeout (`10_000ms`) and `countPrimaryHeadingsOutsidePanicOverlay == 0`.
- Prior dominant 90s `waitForBoot` timeout signature did not occur in this targeted run pair.
- Setup/login remained healthy for both browsers, keeping the blocker route-level.

## Result

Phase 28 is **verified as `gaps_found`**. Diagnostics are improved and failure shape is tighter, but scoped debt retirement for `/settings` + `/user` remains blocked.
