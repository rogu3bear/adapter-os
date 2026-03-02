---
phase: 22-qa-visual-working-system-activation-macos-dual-browser-blocking
created: 2026-02-26
status: ready_for_planning
---

# Phase 22: QA Visual Working System Activation (macOS Dual-Browser Blocking) - Research

**Researched:** 2026-02-26
**Domain:** Playwright UI quality gate contract stabilization and visual baseline governance
**Confidence:** HIGH

## User Constraints

### Locked Decisions (from 22-CONTEXT.md)
- Open milestone `v1.1.6` now and treat `GOV-16` as accepted external debt.
- Dual-browser blocking gate is required.
- Scope is strict audit-anchor-rectify on existing Playwright infrastructure.
- macOS snapshots are canonical baselines.
- Blocking visual gate must execute on macOS runners.
- Bundled lane must include console regression, route audit, visual, and critical detail flows.

### Claude's Discretion (from 22-CONTEXT.md)
- Verification sequencing and evidence packaging.
- README integration style.
- Optional selector-only checks vs full lane runs when runtime cost is high.

### Deferred Ideas (from 22-CONTEXT.md)
- Harness rewrites or new test framework paths.
- Chat visual expansion.
- Governance debt retirement (`GOV-16`).

## Summary

Repository audit indicates the required Phase 22 technical surfaces already exist and align with the locked contract:
- `.github/workflows/ci.yml` already defines separate blocking `macos-14` quality jobs for Chromium and WebKit.
- `tests/playwright/package.json` already defines explicit `test:audit` and `test:gate:quality` scripts with bundled spec list.
- `tests/playwright/scripts/check-visual-snapshot-contract.mjs` already enforces active-baseline presence and orphan baseline rejection with `darwin` canonical naming.
- `tests/playwright/README.md` already documents bundled lane behavior and canonical baseline policy.

Phase 22 implementation therefore centers on artifact rollover, contract anchoring in current milestone docs, and deterministic verification evidence capture.

## Standard Stack

| Library/Tool | Version | Purpose | Why Standard |
|--------------|---------|---------|--------------|
| GitHub Actions | repo standard | Tier-2 CI gate execution | Existing dual-browser jobs already wired |
| Playwright | `@playwright/test` `^1.50.0` | UI quality lane execution | Existing test harness and reporters |
| Node scripts | repo standard | Snapshot contract pre-check + runner wrapper | Existing `pw-run.mjs` and contract checker |
| `rg` + shell | repo standard | Contract/path verification | Fast deterministic checks |

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Quality lane selector logic | New custom selector framework | `npm run test:gate:quality` with explicit spec list | Keeps CI/local contract identical |
| Snapshot contract validation | Manual file checklist | `scripts/check-visual-snapshot-contract.mjs` | Already enforces missing/orphan policies |
| CI browser gate fanout | New matrix workflow | Existing split jobs in `.github/workflows/ci.yml` | Minimal diff and already blocking |

## Validation Architecture

| Property | Value |
|----------|-------|
| Framework | Playwright UI config (`playwright.ui.config.ts`) + contract precheck |
| Selector truth command | `npm run test:gate:quality -- --project=chromium --list` and `--project=webkit --list` |
| Snapshot contract command | `node scripts/check-visual-snapshot-contract.mjs` |
| Full lane command | `npm run test:gate:quality -- --project=<browser>` |

### Phase Requirements -> Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| QA-22-01 | Dual-browser blocking lane uses explicit bundled command selectors | integration | `rg -n 'playwright-ui-quality-gate-(chromium|webkit)|npm run test:gate:quality -- --project=' .github/workflows/ci.yml` | yes |
| QA-22-02 | Shared local+CI gate command contract (`test:audit`, `test:gate:quality`) | integration | `rg -n 'test:audit|test:gate:quality' tests/playwright/package.json` | yes |
| QA-22-03 | Snapshot contract rejects missing/orphan baselines, darwin canonical policy | integration | `node tests/playwright/scripts/check-visual-snapshot-contract.mjs` | yes |
| DOC-22-01 | README/planning include canonical commands and baseline policy | integration | `rg -n 'test:gate:quality|darwin|canonical baselines|Blocking UI Quality Gate' tests/playwright/README.md README.md .planning/ROADMAP.md .planning/REQUIREMENTS.md` | yes |

## Planning Implications

- `22-01` should anchor CI and package selector contract without introducing alternate paths.
- `22-02` should enforce snapshot baseline contract and rectify only actual drift/orphans.
- `22-03` should verify selector truth, capture evidence paths, and reconcile docs/planning closure language.
