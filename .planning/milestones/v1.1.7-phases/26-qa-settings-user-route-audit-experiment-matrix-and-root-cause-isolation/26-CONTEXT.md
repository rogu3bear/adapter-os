---
phase: "26"
name: "QA Settings/User Route-Audit Experiment Matrix and Root-Cause Isolation"
created: 2026-02-26
status: discussed
mode: delegated
---

# Phase 26: QA Settings/User Route-Audit Experiment Matrix and Root-Cause Isolation - Context

## Decisions

- Scope is strict root-cause isolation for `/settings` and `/user` route-audit failures under unskip mode.
- No runtime/API/product surface changes; only Playwright route-audit strategy experiments and evidence.
- Preserve existing default debt behavior (`AUDIT_ROUTE_SKIP` + `PW_UNSKIP_SETTINGS_USER_AUDIT=1` for repro).
- Optimize for smallest-change experiments first, then evidence-backed conclusion.

## Explicit Blockers Entering Phase 26

- Phase 25 blocker runs (`phase25-blocker-chromium`, `phase25-blocker-webkit`) fail deterministically on `/settings` and `/user`.
- Setup/login succeeds in both browsers (`global-setup-summary.json: success=true`), so failure is post-setup route execution.

## Discretion Areas

- Experiment matrix sequence and hypothesis order.
- Whether any route-local strategy can retire debt without changing global auth/runtime contracts.

## Deferred Ideas

- Harness rewrites.
- Broad global auth refactor without deterministic proof.
