---
phase: 26-qa-settings-user-route-audit-experiment-matrix-and-root-cause-isolation
created: 2026-02-26
status: ready_for_planning
---

# Phase 26: QA Settings/User Route-Audit Experiment Matrix and Root-Cause Isolation - Research

**Researched:** 2026-02-26
**Domain:** Playwright route-audit determinism for `/settings` and `/user`
**Confidence:** HIGH

## User Constraints

### Locked Decisions
- Continue/resume next phase autonomously.
- Keep current CI and gate contracts unchanged.
- Preserve scoped debt behavior unless deterministic retirement is proven.

### Claude's Discretion
- Route-local experiment shape and sequencing.
- Minimal doc reconciliation needed for phase truth.

## Evidence Baseline

### Prior Blocker Confirmation
- `phase25-blocker-chromium`: failed (`/settings`, `/user`), setup success.
- `phase25-blocker-webkit`: failed (`/settings`, `/user`), setup success.

### Phase 26 Experiment Matrix

1. **EXP1**: disable special soft-route branch, force standard `gotoAndBootstrap(..., ui-only)`
   - `phase26-exp1-chromium`: failed `/settings`, `/user` (fast heading assertion failures).
   - `phase26-exp1-webkit`: failed `/settings`, `/user` (90s timeout/closure).

2. **EXP2**: keep soft-route set, bypass auth bootstrap for those routes (`mode: none`)
   - `phase26-exp2-chromium`: failed `/settings`, `/user` (fast heading assertion failures).
   - `phase26-exp2-webkit`: failed `/settings`, `/user` (90s timeout/closure).

### Setup Health
All four experiment runs report global setup success (`success=true`) and login status `200`.

## Planning Implications

- No route-local experiment in this phase retired debt in both browsers.
- Retirement remains blocked; default scoped skip contract should stay unchanged.
- Next retirement attempt should prioritize deeper auth/runtime transition instrumentation rather than additional selector tweaks.
