---
phase: "28"
name: "QA Settings/User Route-Audit Boot Stall Fail-Fast Guard and Diagnostics"
created: 2026-02-26
status: discussed
mode: delegated
---

# Phase 28: QA Settings/User Route-Audit Boot Stall Fail-Fast Guard and Diagnostics - Context

## Decisions

- Keep scope limited to Playwright UI helper behavior (`waitForBoot` diagnostics and timeout semantics).
- No CI selector contract, bundled-lane composition, or product/runtime API changes.
- Preserve default scoped debt policy for `/settings` and `/user` unless deterministic dual-browser unskip pass proves retirement.
- Use targeted unskip verification only (`/settings` + `/user`, Chromium + WebKit).

## Baseline Entering Phase 28

- Phase 27 reduced auth-surface churn, but both browsers still timed out for `/settings` and `/user` in post-navigation boot/readiness waits.
- Phase 27 setup remained healthy (`global-setup-summary.json` showed `success=true` for both browsers), so blocker class remained route-level.
- Existing failure mode consumed large timeout budgets with limited in-band diagnostics from `waitForBoot`.

## Phase 28 Focus

- Convert route-level boot stalls into explicit fail-fast outcomes with structured diagnostics.
- Re-run targeted `/settings` + `/user` unskip matrix in Chromium and WebKit using Phase 28 run IDs.
- Record concrete evidence to determine whether scoped debt can be retired or remains blocked.
