---
phase: "30"
name: "QA Settings/User Route-Audit Debt Retirement Closure"
created: 2026-02-28
status: executed
mode: delegated
---

# Phase 30: QA Settings/User Route-Audit Debt Retirement Closure - Context

## Decisions

- Keep scope strictly on Playwright route-audit behavior (`tests/playwright/ui/routes.best_practices.audit.spec.ts`).
- Reuse the existing soft-route navigation helpers in `tests/playwright/ui/utils.ts`; do not introduce parallel navigation abstractions.
- Retire scoped skip debt only after deterministic default-mode evidence is captured in both Chromium and WebKit.
- Preserve canonical gate contracts and artifact layout defined in `tests/playwright/README.md`.

## Baseline Entering Phase 30

- Phase 29 closed `passed_with_scoped_debt`; `/settings` and `/user` remained excluded by default route-audit skip policy.
- Prior targeted retirement phases (25-28) captured deterministic blocker evidence, but recent helper hardening enabled successful unskip validation.

## Phase 30 Focus

- Remove temporary skip gating for `/settings` and `/user` in default route-audit execution.
- Prove dual-browser determinism for `/settings` + `/user` without `PW_UNSKIP_SETTINGS_USER_AUDIT`.
- Validate full route-audit surface in both browsers to ensure no regression from debt retirement.
