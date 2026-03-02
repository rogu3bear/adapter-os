---
phase: "27"
name: "QA Settings/User Route-Audit Auth Transition Guard and Boot Deadlock Confirmation"
created: 2026-02-26
status: discussed
mode: delegated
---

# Phase 27: QA Settings/User Route-Audit Auth Transition Guard and Boot Deadlock Confirmation - Context

## Decisions

- Keep scope limited to Playwright auth/bootstrap and route-audit helper logic.
- No CI command contract, gate selector, or product/runtime API changes.
- Preserve default scoped debt policy for `/settings` and `/user` unless deterministic dual-browser unskip pass is proven.
- Run targeted unskip matrix only; no broad sweep.

## Baseline Entering Phase 27

- Phase 26 (`EXP1`, `EXP2`) confirmed setup/login success but route retirement remained blocked in Chromium and WebKit.
- Scoped skip debt remained active in `routes.best_practices.audit.spec.ts`.

## Phase 27 Focus

- Remove auth-surface false negatives (login-form detachment during redirect to `/chat`) without widening scope.
- Re-run targeted `/settings` + `/user` matrix in both browsers.
- Record whether blockers remain in route boot/readiness transitions.
