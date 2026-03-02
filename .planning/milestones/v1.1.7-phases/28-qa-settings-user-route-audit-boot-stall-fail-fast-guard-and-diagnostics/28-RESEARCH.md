---
phase: 28-qa-settings-user-route-audit-boot-stall-fail-fast-guard-and-diagnostics
created: 2026-02-26
status: ready_for_planning
---

# Phase 28: QA Settings/User Route-Audit Boot Stall Fail-Fast Guard and Diagnostics - Research

**Researched:** 2026-02-26  
**Domain:** Playwright route boot/readiness deadlock instrumentation for `/settings` + `/user` targeted audit runs  
**Confidence:** HIGH

## Evidence Highlights

1. Phase 27 dual-browser targeted runs (`phase27-fix1-chromium`, `phase27-fix1-webkit`) both failed `/settings` and `/user` while global setup stayed healthy (`success=true`, login `200` in `debug/global-setup-summary.json`).
2. Prior traces showed route navigation happened, but readiness waited until timeout; blocker class remained post-navigation boot/readiness behavior, not pre-auth seeding/login.
3. Existing `waitForBoot` timeout reporting was not strict enough for quick blocker isolation and lacked route/stage-rich diagnostics in failure paths.
4. Current helper update in `tests/playwright/ui/utils.ts` introduces bounded boot snapshot polling, evaluate-timeout guardrails, and structured timeout diagnostics with stage snapshots + page errors.

## Planning Implications

- A minimal helper-level fail-fast contract can improve determinism and operator diagnosis without changing runtime behavior or CI selector contracts.
- Targeted dual-browser unskip reruns are sufficient to validate whether blocker posture changed or only became better instrumented.
- Scoped debt policy should remain unchanged unless `/settings` + `/user` pass in both browsers under unskip mode.
