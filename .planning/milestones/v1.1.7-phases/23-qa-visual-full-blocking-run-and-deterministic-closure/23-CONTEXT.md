---
phase: "23"
name: "QA Visual Full Blocking Run and Deterministic Closure"
created: 2026-02-26
status: discussed
mode: delegated
---

# Phase 23: QA Visual Full Blocking Run and Deterministic Closure — Context

## Decisions

- Phase 23 exists to close the explicit residual risk from Phase 22 (`passed_with_risk`) by running full bundled assertions on both blocking browsers.
- No new product/API/runtime surface is added in Phase 23.
- Canonical bundled command remains `npm run test:gate:quality -- --project=<browser>`.
- Run IDs for this phase must be explicit and unique to evidence closure.
- Snapshot policy remains macOS canonical (`*-darwin.png`) and contract check remains mandatory pre-run.
- UAT and verification must reference concrete full-run artifact paths under `var/playwright/runs/<run-id>/...`.

## Discretion Areas

- Browser execution order (Chromium first or WebKit first).
- Whether to retry once on clearly transient infra flake after capturing first-failure artifacts.
- Exact wording for closure status (`passed` vs `passed_with_observation`) based on run outcomes.

## Deferred Ideas

- Expanding bundled lane membership beyond current five specs.
- Any Playwright harness rewrite.
- Chat visual expansion and governance debt retirement.
