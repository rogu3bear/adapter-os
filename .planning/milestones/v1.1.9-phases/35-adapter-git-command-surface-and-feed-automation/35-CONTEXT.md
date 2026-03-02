---
phase: "35"
name: "Adapter Git Command Surface and Feed Automation"
created: 2026-02-28
status: discussed
mode: delegated
---

# Phase 35: Adapter Git Command Surface and Feed Automation - Context

## Decisions

- Continue command ergonomics in existing adapter surfaces; no parallel workflow trees.
- Keep checkout-first semantics as canonical language.
- Preserve branch/version feed provenance as first-class operator context.

## Baseline Entering Phase 35

v1.1.8 completed assistive guidance and accessibility hardening. The remaining opportunity is command-surface ergonomics and feed continuity cues that reduce operator decision friction.

## Phase 35 Focus

1. Add command-oriented affordances for high-frequency adapter operations.
2. Improve natural-language hints around recommended workflows.
3. Harden feed continuity cues for branch/version provenance.

## Citations

- `crates/adapteros-ui/src/components/adapter_detail_panel.rs`
- `crates/adapteros-ui/src/pages/dashboard.rs`
- `crates/adapteros-ui/src/pages/update_center.rs`
