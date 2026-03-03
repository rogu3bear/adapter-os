---
phase: "34"
name: "Assistive UI Foundation and Guided Operator Flow"
created: 2026-02-28
status: discussed
mode: delegated
---

# Phase 34: Assistive UI Foundation and Guided Operator Flow - Context

## Decisions

- Keep the assistive pass inside existing dashboard/update-center/detail surfaces.
- Use explicit guidance plus recommended-next-action messaging instead of adding new routing complexity.
- Improve accessibility by adding descriptive `aria_label` values to high-impact controls.

## Baseline Entering Phase 34

The adapter VCS substrate and dataset-feed continuity were already complete in v1.1.7. The remaining gap was operator assistiveness: better in-flow guidance, clearer natural-language resume cues, and stronger assistive-technology discoverability.

## Phase 34 Focus

1. Add stepwise operator guidance and dynamic next-action messaging in adapter detail Update Center.
2. Improve dashboard guided-flow language for resume scenarios.
3. Harden action-level accessibility labels across key control points.

## Citations

- Adapter detail Update Center controls and workflow messaging: `crates/adapteros-ui/src/components/adapter_detail_panel.rs`.
- Dashboard guided flow step content and CTA structure: `crates/adapteros-ui/src/pages/dashboard.rs`.
- Update Center top-level actions and list actions: `crates/adapteros-ui/src/pages/update_center.rs`.
