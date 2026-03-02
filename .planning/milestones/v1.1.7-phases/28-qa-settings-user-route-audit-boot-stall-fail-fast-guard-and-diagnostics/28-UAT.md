---
phase: "28"
name: "QA Settings/User Route-Audit Boot Stall Fail-Fast Guard and Diagnostics"
created: 2026-02-26
updated: "2026-02-26T07:45:00Z"
status: gaps_found
---

# Phase 28: QA Settings/User Route-Audit Boot Stall Fail-Fast Guard and Diagnostics — User Acceptance Testing

## Test Results

| # | Test | Status | Notes |
|---|------|--------|-------|
| 1 | Fix1 Chromium (`/settings` + `/user`) | failed | Both routes failed at best-practices heading readiness poll (`10s`) in `assertBestPractices`. |
| 2 | Fix1 WebKit (`/settings` + `/user`) | failed | Same failure shape as Chromium (`countPrimaryHeadingsOutsidePanicOverlay == 0`). |
| 3 | Setup health across both Phase 28 runs | passed | `global-setup-summary.json` reports `success=true` and login `200` in both browsers. |
| 4 | Evidence completeness (reports + traces + videos + status files) | passed | Artifacts present under both `phase28-fix1-*` run roots. |

## Operator Checklist

1. Confirm run statuses:
   - `var/playwright/runs/phase28-fix1-chromium/test-results/.last-run.json`
   - `var/playwright/runs/phase28-fix1-webkit/test-results/.last-run.json`
2. Confirm setup success:
   - `var/playwright/runs/phase28-fix1-chromium/debug/global-setup-summary.json`
   - `var/playwright/runs/phase28-fix1-webkit/debug/global-setup-summary.json`
3. Confirm reports exist:
   - `var/playwright/runs/phase28-fix1-chromium/report/index.html`
   - `var/playwright/runs/phase28-fix1-webkit/report/index.html`
4. Confirm route traces exist:
   - `var/playwright/runs/phase28-fix1-chromium/test-results/routes.best_practices.audit-best-practices-audit-settings-chromium/trace.zip`
   - `var/playwright/runs/phase28-fix1-webkit/test-results/routes.best_practices.audit-best-practices-audit-settings-webkit/trace.zip`
5. Confirm scoped skip debt remains explicit in `tests/playwright/ui/routes.best_practices.audit.spec.ts`.

## Exit Criteria

- **Pass:** `/settings` and `/user` pass under unskip mode for both Chromium and WebKit.
- **Fail:** either route fails in either browser.

## Summary

UAT result is **gaps_found**. Phase 28 improved fail-fast diagnostics and narrowed failure signature, but did not retire `/settings` + `/user` route-audit debt.
