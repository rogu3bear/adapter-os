---
phase: "24"
name: "QA Dual-Browser Regression Closure and Gate Pass"
created: 2026-02-26
status: discussed
mode: delegated
---

# Phase 24: QA Dual-Browser Regression Closure and Gate Pass — Context

## Decisions

- Phase 24 continues the v1.1.6 QA closure path after Phase 23 restored full dual-browser execution.
- Scope is limited to existing blocking suite regressions observed in `phase23-fix4-{chromium,webkit}`.
- No new runtime/API/product surface; only stabilization/expectation alignment inside existing Playwright + UI surfaces.
- Canonical gate command remains `npm run test:gate:quality -- --project=<browser>`.
- macOS canonical baseline policy remains unchanged (`*-darwin.png`), and snapshot contract script stays mandatory.

## Explicit Blockers Entering Phase 24

- Route audit timeout on `/settings` and `/user` (`routes.best_practices.audit.spec.ts`).
- Detail-flow failures in `ui/runs.spec.ts` (`Flight Recorder` heading and `Chat unavailable` expectation).
- Visual diffs in `ui/visual.spec.ts`:
  - Chromium: `adapters.png`
  - WebKit: `training-detail.png`, `adapters.png`

## Discretion Areas

- Exact minimal fix strategy (test hardening vs UI readiness/wiring corrections) as long as behavior stays native.
- Whether visual baselines are updated after confirming intentional render drift.
- Retry policy for final dual-browser runs after first deterministic evidence capture.

## Deferred Ideas

- Test harness rewrites.
- New QA lane members.
- Product/UI feature expansion outside failing surfaces.
