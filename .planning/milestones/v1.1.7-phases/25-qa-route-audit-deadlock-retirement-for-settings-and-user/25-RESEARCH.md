---
phase: 25-qa-route-audit-deadlock-retirement-for-settings-and-user
created: 2026-02-26
status: ready_for_planning
---

# Phase 25: QA Route-Audit Deadlock Retirement for Settings and User - Research

**Researched:** 2026-02-26
**Domain:** Route-audit determinism closure for `/settings` and `/user`
**Confidence:** HIGH

## User Constraints

### Locked Decisions
- Resume next phase autonomously.
- Keep current CI/package gate selectors and dual-browser macOS blocking policy.
- No harness rewrite; retire debt inside existing Playwright stack.
- Prefer targeted verification first, then full bundled lane confirmation.

### Claude's Discretion
- Smallest viable route-audit change to expose and retire debt safely.
- Repro sequencing (targeted first, full lane after debt removal).

### Deferred Ideas
- Any non-route-audit QA expansion.

## Evidence Baseline

### Existing Debt Surface
- `tests/playwright/ui/routes.best_practices.audit.spec.ts` has:
  - `AUDIT_ROUTE_SKIP = new Set(['/settings', '/user'])`
  - unconditional `test.skip(...)` for those routes.

### Historical Failure Signature (pre-skip)
- Phase 23 failing traces in both browsers:
  - `var/playwright/runs/phase23-fix4-chromium/report/data/d40f4b4ab5a42a4c196449b4c5739313313403a1.zip`
  - `var/playwright/runs/phase23-fix4-chromium/report/data/87adff4025bd53da48f24991806870c9c5aa2c7c.zip`
  - `var/playwright/runs/phase23-fix4-webkit/report/data/f609f7a1824be5ea2a842fffdc4c4fa864511c27.zip`
  - `var/playwright/runs/phase23-fix4-webkit/report/data/39992414a40669d3a59955eaaad86051a59ee69b.zip`
- Common fingerprint: `locator('html').getAttribute('lang')` timing out at the route-audit assertion step for `/settings` and `/user`.

## Validation Architecture

| Property | Command |
|----------|---------|
| Chromium route-audit repro (unskipped) | `cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase25-audit-repro-chromium npm run test:gate:quality -- --project=chromium ui/routes.best_practices.audit.spec.ts` |
| WebKit route-audit repro (unskipped) | `cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase25-audit-repro-webkit npm run test:gate:quality -- --project=webkit ui/routes.best_practices.audit.spec.ts` |
| Chromium bundled closure | `cd tests/playwright && PW_RUN_ID=phase25-full-chromium npm run test:gate:quality -- --project=chromium` |
| WebKit bundled closure | `cd tests/playwright && PW_RUN_ID=phase25-full-webkit npm run test:gate:quality -- --project=webkit` |

## Planning Implications

- Phase can close in one plan if unskipped route audits pass deterministically in both browsers and full bundled lane remains green after debt retirement.
- If deadlock persists, phase closes with `gaps_found` and explicit blocker evidence, but skip logic must remain controlled and documented.
