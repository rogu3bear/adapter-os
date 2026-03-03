---
phase: "26"
name: "QA Settings/User Route-Audit Experiment Matrix and Root-Cause Isolation"
created: 2026-02-26
verified: "2026-02-26T07:30:10Z"
status: gaps_found
score: 1/3 requirements verified
verifier: gsd-full-suite
---

# Phase 26: QA Settings/User Route-Audit Experiment Matrix and Root-Cause Isolation — Verification

## Goal-Backward Verification

**Phase Goal:** Prove whether minimal route-local strategy changes can retire `/settings` + `/user` debt.

## Checks

| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 1 | QA-23-01 (Chromium unskip route pair passes) | NOT VERIFIED | `phase26-spearhead11-chromium` failed both `/settings` and `/user` (`var/playwright/runs/phase26-spearhead11-chromium/test-results/.last-run.json`). |
| 2 | QA-23-02 (WebKit unskip route pair passes) | NOT VERIFIED | `phase26-spearhead11-webkit` failed both `/settings` and `/user` (`var/playwright/runs/phase26-spearhead11-webkit/test-results/.last-run.json`). |
| 3 | DOC-23-01 (evidence completeness for decision) | VERIFIED | Spearhead run artifacts include setup summaries, status files, reports, videos, and traces under `var/playwright/runs/phase26-spearhead11-*/...`. |

## Validation Commands

1. `cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase26-spearhead11-chromium npm run test:ui -- --project=chromium ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /(settings|user)"`
2. `cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase26-spearhead11-webkit npm run test:ui -- --project=webkit ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /(settings|user)"`
3. `cargo check -p adapteros-ui`

## Required Artifacts

| Artifact | Expected | Status |
|----------|----------|--------|
| `var/playwright/runs/phase26-spearhead11-chromium/debug/global-setup-summary.json` | setup health | VERIFIED (`success=true`) |
| `var/playwright/runs/phase26-spearhead11-webkit/debug/global-setup-summary.json` | setup health | VERIFIED (`success=true`) |
| `var/playwright/runs/phase26-spearhead11-chromium/test-results/.last-run.json` | run status | VERIFIED (`failed`) |
| `var/playwright/runs/phase26-spearhead11-webkit/test-results/.last-run.json` | run status | VERIFIED (`failed`) |
| `var/playwright/runs/phase26-spearhead11-chromium/report/index.html` | report surface | VERIFIED |
| `var/playwright/runs/phase26-spearhead11-webkit/report/index.html` | report surface | VERIFIED |
| `cargo check -p adapteros-ui` | compile health | VERIFIED (`0` exit) |

## Root-Cause Signals

- Setup/login is healthy in both browsers (`global-setup-summary.json`: login `200`, auth-me `200`), so the lane is not failing from fixture seeding.
- Final route assertions fail because no primary heading is rendered outside panic overlay for `/settings` and `/user`.
- Trace end snapshots remain on the signing overlay (`adapterOS` + `Signing you in`) for both routes in both browsers.
- UI auth state handling was hardened to recover from stale successful auth attempts while still loading.
- Playwright auth-surface probes were bounded (`safeIsVisible`) to remove WebKit locator deadlock behavior; current failure mode in both browsers is deterministic assertion failure (~10s poll timeout).

## Result

Phase 26 is **verified as `gaps_found`**. Route-local auth/flow hardening improved determinism, but `/settings` + `/user` debt is still not retired in both browsers.
