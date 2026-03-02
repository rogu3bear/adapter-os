---
phase: "25"
name: "QA Route-Audit Deadlock Retirement for Settings and User"
created: 2026-02-26
status: discussed
mode: delegated
---

# Phase 25: QA Route-Audit Deadlock Retirement for Settings and User - Context

## Decisions

- Phase 25 scope is strict debt retirement for route-audit coverage on `/settings` and `/user`.
- No new product/runtime/API surface; only Playwright route-audit determinism and gating truth.
- CI and package gate contracts remain unchanged (`test:gate:quality`, dual-browser blocking on macOS).
- Use a temporary env-gated unskip knob during reproduction (`PW_UNSKIP_SETTINGS_USER_AUDIT=1`) so default lane behavior stays stable while fixing.
- Closure target is removal of scoped skip debt from default blocking runs after deterministic proof in Chromium and WebKit.

## Explicit Blockers Entering Phase 25

- `tests/playwright/ui/routes.best_practices.audit.spec.ts` currently hard-skips `/settings` and `/user`.
- Historical failures (`phase23-fix4-{chromium,webkit}`) timed out in `assertBestPractices` while resolving document-level language metadata after navigation.
- Current gate pass truth is conditional on that scoped skip debt.

## Discretion Areas

- Minimal implementation shape for unskip control (must preserve default behavior until verified).
- Whether the existing route assertions now pass as-is once unskipped or require additional hardening.
- Exact evidence naming for targeted and full closure runs.

## Deferred Ideas

- Route-audit framework rewrites or broad expansion.
- New QA lane members unrelated to retiring this debt.
- Any UI feature work outside audit determinism.
