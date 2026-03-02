---
phase: 35-adapter-git-command-surface-and-feed-automation
created: 2026-02-28
status: ready_for_planning
---

# Phase 35: Adapter Git Command Surface and Feed Automation - Research

**Researched:** 2026-02-28
**Domain:** Command-oriented adapter UX and feed provenance continuity
**Confidence:** MEDIUM-HIGH

## Evidence Highlights

Current adapter surfaces already include selector resolution, promotion/checkout, and feed-entry continuity. A minimal-diff approach can improve operator command discoverability and natural-language hints without altering backend contracts.

## Planning Implications

A single execute plan can focus on command-surface affordances and wording updates in existing UI components, with targeted compile and artifact validation.

## Citations

- `crates/adapteros-ui/src/components/adapter_detail_panel.rs`
- `crates/adapteros-ui/src/pages/dashboard.rs`
- `crates/adapteros-ui/src/pages/update_center.rs`
