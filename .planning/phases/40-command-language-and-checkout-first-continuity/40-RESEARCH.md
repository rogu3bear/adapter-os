---
phase: 40-command-language-and-checkout-first-continuity
created: 2026-02-28
status: ready_for_planning
---

# Phase 40: Command Language and Checkout-First Continuity - Research

**Researched:** 2026-02-28
**Domain:** command-language parity and default-path wording continuity
**Confidence:** HIGH

## Evidence Highlights

- Adapter Detail already contains canonical command map and recommended-next-action framing.
- Dashboard and Update Center already use the same command sequence intent, but wording can still drift without an explicit parity pass.
- Existing nav keyword indexing includes command terms that should remain aligned with checkout-first language.

## Planning Implications

- Single execute plan is sufficient: unify wording and command order across dashboard/update/detail while preserving existing behavior semantics.
- Keep minimal diffs in established surfaces; no new UI subsystem.

## Citations

- `crates/adapteros-ui/src/components/adapter_detail_panel.rs`
- `crates/adapteros-ui/src/pages/dashboard.rs`
- `crates/adapteros-ui/src/pages/update_center.rs`
- `crates/adapteros-ui/src/components/layout/nav_registry.rs`
