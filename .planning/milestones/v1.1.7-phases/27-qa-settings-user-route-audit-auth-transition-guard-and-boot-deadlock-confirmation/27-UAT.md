---
phase: "27"
name: "QA Settings/User Route-Audit Auth Transition Guard and Boot Deadlock Confirmation"
created: 2026-02-26
updated: "2026-02-26T06:28:04Z"
status: gaps_found
---

# Phase 27: QA Settings/User Route-Audit Auth Transition Guard and Boot Deadlock Confirmation — User Acceptance Testing

## Test Results

| # | Test | Status | Notes |
|---|------|--------|-------|
| 1 | Fingerprint Chromium (`/settings`) | failed | Reproduced login-form detachment during redirect to `/chat`; run path `phase27-fingerprint-chromium`. |
| 2 | Fix1 Chromium (`/settings` + `/user`) | failed | Both cases timed out; traces include target-route navigation plus boot/readiness stall. |
| 3 | Fix1 WebKit (`/settings` + `/user`) | failed | Both cases timed out with the same post-navigation stall profile. |
| 4 | Setup health across all Phase 27 runs | passed | Global setup summaries report `success=true` and login status `200`. |

## Operator Checklist

1. Confirm run statuses:
   - `var/playwright/runs/phase27-fix1-chromium/test-results/.last-run.json`
   - `var/playwright/runs/phase27-fix1-webkit/test-results/.last-run.json`
2. Confirm setup success:
   - `var/playwright/runs/phase27-fingerprint-chromium/debug/global-setup-summary.json`
   - `var/playwright/runs/phase27-fix1-chromium/debug/global-setup-summary.json`
   - `var/playwright/runs/phase27-fix1-webkit/debug/global-setup-summary.json`
3. Confirm reports exist:
   - `var/playwright/runs/phase27-fingerprint-chromium/report/index.html`
   - `var/playwright/runs/phase27-fix1-chromium/report/index.html`
   - `var/playwright/runs/phase27-fix1-webkit/report/index.html`
4. Confirm scoped skip debt remains explicit in `routes.best_practices.audit.spec.ts`.

## Exit Criteria

- **Pass:** `/settings` and `/user` pass under unskip mode for both Chromium and WebKit.
- **Fail:** either route fails in either browser.

## Summary

UAT result is **gaps_found**. Phase 27 improved auth-transition handling but did not retire `/settings` + `/user` route-audit debt.
