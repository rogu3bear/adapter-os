---
phase: 23-qa-visual-full-blocking-run-and-deterministic-closure
created: 2026-02-26
status: ready_for_planning
---

# Phase 23: QA Visual Full Blocking Run and Deterministic Closure - Research

**Researched:** 2026-02-26
**Domain:** Full assertion execution closure for Playwright dual-browser blocking quality lane
**Confidence:** HIGH

## User Constraints

### Locked Decisions
- Resume next actionable phase after phase-22 completion.
- Keep existing Playwright/CI command contract; no new product surface.
- Execute full bundled lane on both blocking browsers.
- Capture deterministic evidence under run-scoped artifact directories.

### Claude's Discretion
- Run ordering and retry policy.
- Evidence packaging format in verification/UAT artifacts.

### Deferred Ideas
- Any lane expansion/harness refactor.

## Summary

Phase 22 verified selector and contract integrity but intentionally used `--list`, leaving residual risk around actual assertion execution. Phase 23 closes that gap by executing the same bundled commands without `--list` for Chromium and WebKit, then reconciling phase artifacts.

## Standard Stack

| Tool | Purpose |
|------|---------|
| `npm run test:gate:quality -- --project=<browser>` | Canonical full lane execution |
| `node scripts/check-visual-snapshot-contract.mjs` | Pre-run baseline contract guard |
| Run-scoped `var/playwright/runs/<run-id>/...` | Deterministic evidence outputs |

## Don't Hand-Roll

| Problem | Don't Build | Use Instead |
|---------|-------------|-------------|
| Custom lane command wrappers | New scripts | Existing `test:gate:quality` |
| Manual screenshot inventory checks | Ad hoc grep/file loops | `check-visual-snapshot-contract.mjs` |

## Validation Architecture

| Property | Value |
|----------|-------|
| Full Chromium run | `PW_RUN_ID=phase23-full-chromium npm run test:gate:quality -- --project=chromium` |
| Full WebKit run | `PW_RUN_ID=phase23-full-webkit npm run test:gate:quality -- --project=webkit` |
| Expected artifacts | `heartbeat.json`, `report/index.html`, `test-results/` |

### Phase Requirements -> Test Map

| Req ID | Behavior | Command |
|--------|----------|---------|
| QA-23-01 | Chromium full lane executes with deterministic artifacts | `PW_RUN_ID=phase23-full-chromium npm run test:gate:quality -- --project=chromium` |
| QA-23-02 | WebKit full lane executes with deterministic artifacts | `PW_RUN_ID=phase23-full-webkit npm run test:gate:quality -- --project=webkit` |
| DOC-23-01 | Verification/UAT evidence includes concrete run paths | `rg -n 'phase23-full-(chromium|webkit)' .planning/phases/23-qa-visual-full-blocking-run-and-deterministic-closure/23-VERIFICATION.md .planning/phases/23-qa-visual-full-blocking-run-and-deterministic-closure/23-UAT.md` |

## Planning Implications

- Single execution plan is sufficient for this phase.
- Plan output is binary: either full runs pass, or failure artifacts are captured and closure remains partial.
