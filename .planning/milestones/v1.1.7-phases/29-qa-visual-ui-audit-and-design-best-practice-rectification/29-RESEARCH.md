---
phase: 29-qa-visual-ui-audit-and-design-best-practice-rectification
created: 2026-02-26
status: ready_for_planning
---

# Phase 29: QA Visual UI Audit and Design Best-Practice Rectification - Research

**Researched:** 2026-02-26  
**Domain:** Playwright visual audit/capture + UI design-system alignment  
**Confidence:** HIGH

## Evidence Highlights

1. Initial Chromium capture run (`phase29-capture-chromium`) produced 8/9 screenshots; only `/runs` failed due stale heading matcher in capture spec.
2. Failed `/runs` screenshot confirmed live heading text is `System Restore Points`, not `Flight Recorder`.
3. Explorer audit (static scan) identified concrete best-practice gaps in adapters/training/layout focus handling without requiring broad refactor.
4. After targeted fixes, dual-browser capture runs passed fully:
   - `phase29-capture3-chromium` (`9 passed`)
   - `phase29-capture3-webkit` (`9 passed`)
5. Canonical visual baselines remained stable after rectification:
   - `phase29-visual2-chromium` (`2 passed, 6 skipped`)
   - `phase29-visual2-webkit-r2` (`2 passed, 6 skipped`)

## Planning Implications

- Screenshot capture should remain explicit and deterministic per route copy contract.
- UI updates should reuse existing shared primitives/classes (`table-row-interactive`, `tab-nav`, `btn` variants, focus-visible tokens) instead of page-local overrides.
- Visual contract remains healthy without baseline regeneration; phase can close as passed with existing scoped `/settings` + `/user` debt unchanged.
