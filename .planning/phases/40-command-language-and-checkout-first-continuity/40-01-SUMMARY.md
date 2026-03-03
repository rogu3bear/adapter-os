---
phase: 40-command-language-and-checkout-first-continuity
plan: 01
status: completed
outcome: passed
completed_at: 2026-02-28
---

# Phase 40 Plan 01 Summary

## Outcome

Phase 40 is complete and passed. Command guidance now follows one checkout-first default path across dashboard, update center, and adapter detail surfaces.

## What Changed

1. Normalized default-path language to `run checkout or promote, then feed-dataset` across dashboard guided flow, update center command framing, and adapter detail recommendations.
2. Updated update-center subtitle and navigation discoverability keywords to reinforce checkout-first command language.
3. Preserved command-map snippets and operator flow semantics while tightening wording for lower ambiguity.

## Code Evidence

- `crates/adapteros-ui/src/pages/dashboard.rs`
- `crates/adapteros-ui/src/pages/update_center.rs`
- `crates/adapteros-ui/src/components/adapter_detail_panel.rs`
- `crates/adapteros-ui/src/components/layout/nav_registry.rs`

## Requirement Mapping

- `UX-40-01`: satisfied.
- `NL-40-01`: satisfied.
