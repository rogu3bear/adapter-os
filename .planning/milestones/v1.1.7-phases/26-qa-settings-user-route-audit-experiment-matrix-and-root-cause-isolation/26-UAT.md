---
phase: "26"
name: "QA Settings/User Route-Audit Experiment Matrix and Root-Cause Isolation"
created: 2026-02-26
updated: "2026-02-26T07:30:10Z"
status: gaps_found
---

# Phase 26: QA Settings/User Route-Audit Experiment Matrix and Root-Cause Isolation — User Acceptance Testing

## Test Results

| # | Test | Status | Notes |
|---|------|--------|-------|
| 1 | EXP1 Chromium (`/settings` + `/user`) | failed | `phase26-exp1-chromium` failed both route cases. |
| 2 | EXP1 WebKit (`/settings` + `/user`) | failed | `phase26-exp1-webkit` failed both route cases. |
| 3 | EXP2 Chromium (`/settings` + `/user`) | failed | `phase26-exp2-chromium` failed both route cases. |
| 4 | EXP2 WebKit (`/settings` + `/user`) | failed | `phase26-exp2-webkit` failed both route cases. |
| 5 | SPEARHEAD9 Chromium (`/settings` + `/user`) | failed | `phase26-spearhead9-chromium` failed both route cases with deterministic heading assertions. |
| 6 | SPEARHEAD9 WebKit (`/settings` + `/user`) | failed | `phase26-spearhead9-webkit` failed both route cases with deterministic heading assertions. |
| 7 | SPEARHEAD11 Chromium (`/settings` + `/user`) | failed | `phase26-spearhead11-chromium` failed both route cases with deterministic heading assertions. |
| 8 | SPEARHEAD11 WebKit (`/settings` + `/user`) | failed | `phase26-spearhead11-webkit` failed both route cases with deterministic heading assertions. |
| 9 | Setup health across runs | passed | Spearhead11 runs report `success=true` and setup auth `200/200` in `global-setup-summary.json`. |

## Operator Checklist

1. Confirm run statuses:
   - `var/playwright/runs/phase26-exp1-chromium/test-results/.last-run.json`
   - `var/playwright/runs/phase26-exp1-webkit/test-results/.last-run.json`
   - `var/playwright/runs/phase26-exp2-chromium/test-results/.last-run.json`
   - `var/playwright/runs/phase26-exp2-webkit/test-results/.last-run.json`
2. Confirm setup success files:
   - `var/playwright/runs/phase26-exp1-chromium/debug/global-setup-summary.json`
   - `var/playwright/runs/phase26-exp1-webkit/debug/global-setup-summary.json`
   - `var/playwright/runs/phase26-exp2-chromium/debug/global-setup-summary.json`
   - `var/playwright/runs/phase26-exp2-webkit/debug/global-setup-summary.json`
3. Confirm reports exist for each run:
   - `var/playwright/runs/phase26-exp1-chromium/report/index.html`
   - `var/playwright/runs/phase26-exp1-webkit/report/index.html`
   - `var/playwright/runs/phase26-exp2-chromium/report/index.html`
   - `var/playwright/runs/phase26-exp2-webkit/report/index.html`
4. Confirm default debt contract remains explicit in route-audit spec.
5. Confirm deterministic failure mode for spearhead runs:
   - `var/playwright/runs/phase26-spearhead9-chromium/test-results/routes.best_practices.audit-best-practices-audit-settings-chromium/trace.zip`
   - `var/playwright/runs/phase26-spearhead9-webkit/test-results/routes.best_practices.audit-best-practices-audit-settings-webkit/trace.zip`
6. Confirm deterministic failure mode for latest spearhead runs:
   - `var/playwright/runs/phase26-spearhead11-chromium/test-results/routes.best_practices.audit-best-practices-audit-settings-chromium/trace.zip`
   - `var/playwright/runs/phase26-spearhead11-webkit/test-results/routes.best_practices.audit-best-practices-audit-settings-webkit/trace.zip`

## Exit Criteria

- **Pass:** any experiment path retires `/settings` + `/user` in both browsers.
- **Fail:** both experiments fail in at least one browser.

## Summary

UAT result is **gaps_found**. Latest spearhead pass preserved deterministic failures in both browsers but did not retire route-audit debt; scoped skip policy remains required.
