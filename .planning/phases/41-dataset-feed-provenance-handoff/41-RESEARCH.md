---
phase: 41-dataset-feed-provenance-handoff
created: 2026-02-28
status: ready_for_planning
---

# Phase 41: Dataset Feed Provenance Handoff - Research

**Researched:** 2026-02-28
**Domain:** branch/version provenance continuity into training-entry flow
**Confidence:** HIGH

## Evidence Highlights

- Selected-version feed action already exposes branch-aware launch intent in adapter detail.
- Training page already ingests `repo_id`, `branch`, and `source_version_id` query params and opens the wizard from those params.
- Existing continuity is structurally present; phase work is to harden and verify invariants and operator messaging.

## Planning Implications

- Use one execution plan focused on continuity contract integrity, explicit messaging, and fallback behavior when context is partial.
- Validate no silent parameter drops between update/detail and training entry.

## Citations

- `crates/adapteros-ui/src/components/adapter_detail_panel.rs`
- `crates/adapteros-ui/src/pages/training/mod.rs`
- `crates/adapteros-ui/src/pages/update_center.rs`
