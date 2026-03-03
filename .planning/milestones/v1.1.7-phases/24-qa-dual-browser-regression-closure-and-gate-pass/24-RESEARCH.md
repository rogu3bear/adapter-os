---
phase: 24-qa-dual-browser-regression-closure-and-gate-pass
created: 2026-02-26
status: ready_for_planning
---

# Phase 24: QA Dual-Browser Regression Closure and Gate Pass - Research

**Researched:** 2026-02-26
**Domain:** Regression closure for macOS dual-browser blocking bundled lane
**Confidence:** HIGH

## User Constraints

### Locked Decisions
- Resume next phase autonomously.
- Preserve existing Playwright/CI contracts and bundled selector.
- Keep macOS baseline policy and blocking dual-browser execution.
- Use targeted verification (no broad unrelated sweeps).

### Claude's Discretion
- Choose smallest pattern-aligned fixes in tests/UI wiring.
- Sequence targeted reruns before final full gate rerun.

### Deferred Ideas
- Any harness redesign or lane expansion.

## Evidence Baseline (from Phase 23)

### Run IDs
- `var/playwright/runs/phase23-fix4-chromium`
- `var/playwright/runs/phase23-fix4-webkit`

### Proven Facts
- `debug/global-setup-summary.json` => `success=true` in both runs (seed/setup blocker resolved).
- Chromium last-run status: `failed` with 5 failed specs.
- WebKit last-run status: `failed` with 6 failed specs.

### Deterministic Failing Classes
- Route audit timeout (`/settings`, `/user`) in both browsers.
- `ui/runs.spec.ts` detail-flow expectation mismatches in both browsers.
- Visual diffs:
  - Chromium: `adapters.png`
  - WebKit: `training-detail.png`, `adapters.png`

## Native Pattern Targets

- Route audit assertions should follow existing UI bootstrap/readiness patterns before static DOM checks.
- `runs.spec.ts` should follow current product copy/state semantics (avoid stale string assumptions).
- Visual updates should stay within current baseline contract (`visual.spec.ts-snapshots/*-darwin.png`) and only after assertion intent is validated.

## Validation Architecture

| Property | Command |
|----------|---------|
| Targeted route audit debug | `cd tests/playwright && PW_RUN_ID=phase24-audit-chromium npm run test:gate:quality -- --project=chromium ui/routes.best_practices.audit.spec.ts` |
| Targeted runs flow debug | `cd tests/playwright && PW_RUN_ID=phase24-runs-chromium npm run test:gate:quality -- --project=chromium ui/runs.spec.ts` |
| Targeted visuals debug | `cd tests/playwright && PW_RUN_ID=phase24-visual-{chromium,webkit} npm run test:gate:quality -- --project=<browser> ui/visual.spec.ts` |
| Full closure run | `cd tests/playwright && PW_RUN_ID=phase24-full-<browser> npm run test:gate:quality -- --project=<browser>` |

## Planning Implications

- Single execution plan can close this phase if regressions are fixed and both browser runs pass.
- If failures remain, phase will close with `gaps_found` and concrete residual evidence.
