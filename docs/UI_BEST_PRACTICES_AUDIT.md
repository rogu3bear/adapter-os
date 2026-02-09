# UI Best-Practices Audit (Leptos)

This document is the human-readable acceptance gate for “full rectification” of the primary web UI
(`crates/adapteros-ui`). Automated Playwright checks are used to accelerate regressions, but the
deliverable is a per-route audit with before/after evidence.

## Runbook

### Canonical E2E UI Harness (Seeded)

```bash
cd tests/playwright
npm run test:ui:fast -- --grep @smoke
npx playwright test -c playwright.fast.config.ts ui/routes.best_practices.audit.spec.ts --grep @audit
```

Artifacts:
- Screenshots/traces/reports: `var/playwright/`

### Manual Screenshot Capture (Recommended)

Use Playwright headed mode for deterministic screenshots:

```bash
cd tests/playwright
npx playwright test -c playwright.fast.config.ts --headed ui/routes.best_practices.audit.spec.ts --grep @audit
```

## Checklist (Per Route)

- Exactly one page `<h1>` (panic overlay excluded); public pages still have a document-level heading.
- `<html lang>` present and document title set.
- Shell routes: skip link present and works; content is inside a main landmark.
- No console errors on initial load (excluding explicitly benign noise).
- No failed network requests for required resources (excluding benign aborts during navigation).
- Keyboard: tab order sane, focus visible, no traps; Escape closes dialogs/panels where applicable.

## Route Inventory

Source of truth: `crates/adapteros-ui/src/lib.rs`

For each route below, attach:
- Before screenshot path
- Findings
- Fix reference (commit/PR)
- After screenshot path

### Public
- `/login`
- `/safe`
- `/style-audit`

### Protected (Shell)
- `/`
- `/adapters`
- `/adapters/:id`
- `/chat`
- `/chat/:session_id`
- `/system`
- `/settings`
- `/user`
- `/models`
- `/policies`
- `/training`
- `/stacks`
- `/stacks/:id`
- `/collections`
- `/collections/:id`
- `/documents`
- `/documents/:id`
- `/datasets`
- `/datasets/:id`
- `/admin`
- `/audit`
- `/runs`
- `/runs/:id`
- `/diff`
- `/workers`
- `/workers/:id`
- `/monitoring`
- `/errors`
- `/routing`
- `/repositories`
- `/repositories/:id`
- `/reviews`
- `/reviews/:pause_id` (manual-only unless a pause fixture is added)
- `/welcome`
- `/agents`

