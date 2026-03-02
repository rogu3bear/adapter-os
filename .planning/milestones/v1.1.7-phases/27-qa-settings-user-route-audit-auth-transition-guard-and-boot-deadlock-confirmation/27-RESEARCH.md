---
phase: 27-qa-settings-user-route-audit-auth-transition-guard-and-boot-deadlock-confirmation
created: 2026-02-26
status: ready_for_planning
---

# Phase 27: QA Settings/User Route-Audit Auth Transition Guard and Boot Deadlock Confirmation - Research

**Researched:** 2026-02-26  
**Domain:** Playwright auth/bootstrap + route boot readiness for `/settings` and `/user`  
**Confidence:** HIGH

## Evidence Highlights

1. Chromium fingerprint (`phase27-fingerprint-chromium`) showed `attemptUiLogin` interacting with login inputs while the page auto-redirected to `/chat`, producing detached-input fill timeouts.
2. Chromium and WebKit targeted fix runs (`phase27-fix1-{chromium,webkit}`) still failed both `/settings` and `/user` with 90s test timeouts.
3. Updated traces confirm navigation progressed to target routes, but post-navigation readiness waits (`waitForBoot`) consumed budget and context teardown surfaced secondary closed-page errors.
4. Global setup remained healthy for all Phase 27 runs (`global-setup-summary.json: success=true`, login `200`).

## Planning Implications

- Auth transition guard can reduce one deterministic churn mode (login form detach during redirect).
- Route retirement remains blocked by deeper boot/readiness deadlock behavior after route navigation.
- Scoped debt policy should remain active pending a dedicated boot/readiness remediation pass.
