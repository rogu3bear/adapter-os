# Phase 26 Plan 01 Summary

**Completed:** 2026-02-26
**Requirements:** QA-23-01, QA-23-02, DOC-23-01
**Outcome:** gaps_found

## Scope
Executed a minimal experiment matrix and resumed spearhead pass to determine whether `/settings` and `/user` route-audit debt can be retired with route-local Playwright strategy changes only.

## Files Updated
- `tests/playwright/ui/routes.best_practices.audit.spec.ts`
- `tests/playwright/ui/utils.ts`
- `crates/adapteros-ui/src/components/auth.rs`
- `crates/adapteros-ui/src/signals/auth.rs`
- `.planning/phases/26-qa-settings-user-route-audit-experiment-matrix-and-root-cause-isolation/26-01-SUMMARY.md`
- `.planning/phases/26-qa-settings-user-route-audit-experiment-matrix-and-root-cause-isolation/26-VERIFICATION.md`
- `.planning/phases/26-qa-settings-user-route-audit-experiment-matrix-and-root-cause-isolation/26-UAT.md`
- `.planning/ROADMAP.md`
- `.planning/REQUIREMENTS.md`
- `.planning/STATE.md`

## Commands Executed (Key)
```bash
cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase26-exp1-chromium npm run test:ui -- --project=chromium ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /(settings|user)"
cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase26-exp1-webkit npm run test:ui -- --project=webkit ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /(settings|user)"
cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase26-exp2-chromium npm run test:ui -- --project=chromium ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /(settings|user)"
cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase26-exp2-webkit npm run test:ui -- --project=webkit ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /(settings|user)"
cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase26-spearhead9-chromium npm run test:ui -- --project=chromium ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /(settings|user)"
cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase26-spearhead9-webkit npm run test:ui -- --project=webkit ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /(settings|user)"
cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase26-spearhead11-chromium npm run test:ui -- --project=chromium ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /(settings|user)"
cd tests/playwright && PW_UNSKIP_SETTINGS_USER_AUDIT=1 PW_RUN_ID=phase26-spearhead11-webkit npm run test:ui -- --project=webkit ui/routes.best_practices.audit.spec.ts --grep "best-practices audit: /(settings|user)"
```

## Results
- **EXP1** (standard auth flow path): failed in both browsers for `/settings` and `/user`.
- **EXP2** (soft-route + auth bypass): failed in both browsers for `/settings` and `/user`.
- **SPEARHEAD9** (auth-recovery hardening + deadlock removal): failed in both browsers for `/settings` and `/user` with deterministic `expect.poll(...countPrimaryHeadingsOutsidePanicOverlay...) > 0` failures.
- **SPEARHEAD11** (auth stale-attempt forward progress + bounded visibility probes): failed in both browsers for `/settings` and `/user` with the same deterministic heading assertion signature.
- In all experiment and spearhead runs, global setup succeeded (`success=true`, login `200`, `/v1/auth/me` `200` in setup summary), confirming failures are route execution/transition blockers rather than setup seeding failures.
- Trace end-state remains the same spinner surface (`adapterOS` + `Signing you in`) for both routes, with no primary heading rendered.

## Behavior Changed
- No net contract change.
- Added explicit soft-route auth recovery path for `/settings` and `/user` in route-audit navigation to avoid hanging locator/readiness waits.
- Hardened auth helper transition handling so signing-in surfaces are not treated as resolved auth state.
- Removed blocking readiness waits from API fallback in auth bootstrap path to avoid 90s deadlocks in this targeted lane.
- Added ProtectedRoute loading self-heal retry and stale-success auth promotion in UI auth signal handling.
- Added bounded `safeIsVisible(...)` probes in Playwright auth surface reads to prevent 90s locator hangs.
- Scoped debt contract remains unchanged (default skip + env-gated unskip repro).

## Residual Risk
- `/settings` and `/user` route-audit retirement remains blocked.
- Remaining blocker appears to be app-level boot/auth transition behavior that can leave routes parked on the signing overlay after successful auth setup.
