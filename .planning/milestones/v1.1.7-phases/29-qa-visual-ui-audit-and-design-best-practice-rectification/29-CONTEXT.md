---
phase: "29"
name: "QA Visual UI Audit and Design Best-Practice Rectification"
created: 2026-02-26
status: discussed
mode: delegated
---

# Phase 29: QA Visual UI Audit and Design Best-Practice Rectification - Context

## Decisions

- Run a screenshots-first visual audit using Playwright before any UI code changes.
- Keep scope to UI polish and Playwright visual-audit selector correctness only.
- No runtime API, schema, CI gate composition, or seeding contract changes.
- Preserve existing scoped route-audit debt policy for `/settings` and `/user` (unchanged in this phase).

## Baseline Entering Phase 29

- Existing visual regression baselines passed in both browsers (`training-detail`, `adapters`).
- A new capture harness existed (`ui/visual.audit.capture.spec.ts`) but used a stale `/runs` heading selector (`Flight Recorder`) inconsistent with current UI copy (`System Restore Points`).
- Explorer audit identified alignment gaps with shared UI best-practice primitives:
  - focus-visible affordances on interactive adapter rows,
  - tab-nav consistency in training detail,
  - breadcrumb/main focus-visible polish.

## Phase 29 Focus

- Produce deterministic dual-browser screenshot capture evidence for core UI surfaces.
- Rectify best-practice visual/accessibility alignment with minimal diffs.
- Re-verify canonical visual snapshot suite and capture harness end-to-end in Chromium and WebKit.
