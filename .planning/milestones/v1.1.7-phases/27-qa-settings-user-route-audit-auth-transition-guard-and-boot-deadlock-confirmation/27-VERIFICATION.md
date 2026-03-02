---
phase: "27"
name: "QA Settings/User Route-Audit Auth Transition Guard and Boot Deadlock Confirmation"
created: 2026-02-26
verified: "2026-02-26T06:28:04Z"
status: gaps_found
score: 1/3 requirements verified
verifier: gsd-full-suite
---

# Phase 27: QA Settings/User Route-Audit Auth Transition Guard and Boot Deadlock Confirmation — Verification

## Goal-Backward Verification

**Phase Goal:** Remove auth-surface churn mode and verify whether `/settings` + `/user` debt can be retired.

## Checks

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 1 | QA-23-01 (Chromium unskip route pair passes) | NOT VERIFIED | `phase27-fix1-chromium` failed both `/settings` and `/user` (`var/playwright/runs/phase27-fix1-chromium/test-results/.last-run.json`). |
| 2 | QA-23-02 (WebKit unskip route pair passes) | NOT VERIFIED | `phase27-fix1-webkit` failed both `/settings` and `/user` (`var/playwright/runs/phase27-fix1-webkit/test-results/.last-run.json`). |
| 3 | DOC-23-01 (evidence completeness) | VERIFIED | Fingerprint + dual-browser fix runs include setup summaries, reports, traces, and status files under `var/playwright/runs/phase27-*/...`. |

## Validation Commands

1. `cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase27-fingerprint-chromium npm run test:ui -- --project=chromium ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /settings"`
2. `cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase27-fix1-chromium npm run test:ui -- --project=chromium ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /(settings|user)"`
3. `cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase27-fix1-webkit npm run test:ui -- --project=webkit ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /(settings|user)"`

## Required Artifacts

| Artifact | Expected | Status |
|----------|----------|--------|
| `var/playwright/runs/phase27-fingerprint-chromium/debug/global-setup-summary.json` | setup health | VERIFIED (`success=true`) |
| `var/playwright/runs/phase27-fix1-chromium/debug/global-setup-summary.json` | setup health | VERIFIED (`success=true`) |
| `var/playwright/runs/phase27-fix1-webkit/debug/global-setup-summary.json` | setup health | VERIFIED (`success=true`) |
| `var/playwright/runs/phase27-fix1-chromium/test-results/.last-run.json` | run status | VERIFIED (`failed`) |
| `var/playwright/runs/phase27-fix1-webkit/test-results/.last-run.json` | run status | VERIFIED (`failed`) |
| `var/playwright/runs/phase27-fix1-chromium/report/index.html` | report surface | VERIFIED |
| `var/playwright/runs/phase27-fix1-webkit/report/index.html` | report surface | VERIFIED |

## Root-Cause Signals

- Chromium traces show login input detach during automatic redirect to `/chat`, then successful transition to `Navigate to "/settings"` / `"/user"` under updated helper logic.
- Chromium/WebKit traces still hit long post-navigation `waitForBoot` windows followed by `Test timeout of 90000ms exceeded`, indicating unresolved boot/readiness deadlock for these routes.

## Result

Phase 27 is **verified as `gaps_found`**. Auth transition handling improved, but scoped debt retirement for `/settings` + `/user` remains blocked.
