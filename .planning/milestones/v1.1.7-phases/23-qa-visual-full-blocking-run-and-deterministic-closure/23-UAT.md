---
phase: "23"
name: "QA Visual Full Blocking Run and Deterministic Closure"
created: 2026-02-26
updated: "2026-02-26T03:32:39Z"
status: failed
---

# Phase 23: QA Visual Full Blocking Run and Deterministic Closure — User Acceptance Testing

## Test Results

| # | Test | Status | Notes |
|---|------|--------|-------|
| 1 | Chromium full bundled lane executes and produces assertion results | failed | `phase23-fix4-chromium` executed all selected tests after successful setup (`debug/global-setup-summary.json` shows `success=true`), but 5 specs failed (`/settings`, `/user`, `runs` detail flow, visual `adapters`). |
| 2 | WebKit full bundled lane executes and produces assertion results | failed | `phase23-fix4-webkit` executed all selected tests after successful setup (`success=true`), but 6 specs failed (`/settings`, `/user`, `runs` detail flow, visual `training detail` + `adapters`). |
| 3 | Verification/UAT cite concrete run paths with no contradiction | passed | Both browser run IDs and full artifact paths (`debug`, `report`, `test-results`) are explicitly mapped. |

## Operator Checklist

1. Inspect `debug/global-setup-summary.json` for both run IDs and confirm `success=true` (seed/global-setup no longer blocks execution).
2. Review failing traces/videos for route audit and detail-flow regressions:
   - `.../routes.best_practices.audit-best-practices-audit-settings-*/trace.zip`
   - `.../routes.best_practices.audit-best-practices-audit-user-*/trace.zip`
   - `.../runs-runs-list-and-detail-*/trace.zip`
   - `.../runs-primary-flow-chat-to-run-detail-*/trace.zip`
3. Reconcile visual diffs and baseline intent:
   - Chromium: `.../visual-visual-baselines-adapters-list-chromium/adapters-diff.png`
   - WebKit: `.../visual-visual-baselines-training-detail-webkit/training-detail-diff.png`
   - WebKit: `.../visual-visual-baselines-adapters-list-webkit/adapters-diff.png`
4. Fix failing specs and re-run dual-browser commands:
   - `cd tests/playwright && PW_RUN_ID=phase23-fix5-chromium npm run test:gate:quality -- --project=chromium`
   - `cd tests/playwright && PW_RUN_ID=phase23-fix5-webkit npm run test:gate:quality -- --project=webkit`
3. Re-run:
   - `cd tests/playwright && PW_RUN_ID=phase23-full-chromium-r3 npm run test:gate:quality -- --project=chromium`
   - `cd tests/playwright && PW_RUN_ID=phase23-full-webkit-r3 npm run test:gate:quality -- --project=webkit`

## Exit Criteria

- **Pass:** Both browsers pass the bundled blocking lane (0 failing specs) with complete run artifacts.
- **Fail:** Any blocking suite failure remains (console/audit/visual/detail-flow), even if setup succeeds.

## Summary

UAT failed. Pre-assertion seed blocker is resolved, but the blocking bundled lane still fails consistently on route-audit, detail-flow, and visual assertions across Chromium/WebKit with complete reproducible evidence.
