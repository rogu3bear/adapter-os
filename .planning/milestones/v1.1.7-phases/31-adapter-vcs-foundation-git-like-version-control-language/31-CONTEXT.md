---
phase: "31"
name: "Adapter VCS Foundation: Git-Like Language and Checkout UX"
created: 2026-02-28
status: discussed
mode: delegated
---

# Phase 31: Adapter VCS Foundation: Git-Like Language and Checkout UX - Context

## Decisions

- We will keep the existing adapter version-control substrate and avoid parallel implementation paths.
- We will shift adapter UX language from "restore" to git-like "checkout" semantics while preserving existing backend compatibility.
- We will make dataset evolution first-class in the same workflow by exposing a direct "feed dataset" path from adapter version controls to training.

## Baseline Entering Phase 31

The current adapter surfaces already contain strong version-control mechanics: selector resolution (`tag:...`, `branch@vN`, branch), promotion, rollback endpoint wiring, and dataset lineage per version. The gap is interface language and flow cohesion, not missing primitives.

## Phase 31 Focus

Phase 31 is the language and workflow foundation pass. It should make adapter operations feel like version control in practice:

1. Resolve/select concrete versions.
2. Promote or checkout explicitly.
3. Inspect dataset lineage as evidence.
4. Feed fresh datasets into the next training version without leaving the workflow.

## Citations

- Existing selector, promotion, checkout, and lineage UX in adapter detail: `crates/adapteros-ui/src/components/adapter_detail_panel.rs` lines 567-607, 648-689, 877-1043.
- Update Center language surface for checkout-first operations: `crates/adapteros-ui/src/pages/update_center.rs` lines 1-6 and 103-107.
- Existing API primitives for version list/resolve/promote plus checkout compatibility: `crates/adapteros-ui/src/api/client.rs` lines 524-607.
- Existing training wizard query/return-flow ingestion for dataset-fed starts: `crates/adapteros-ui/src/pages/training/mod.rs` lines 132-204.
- Existing adapter-from-dataset path used by training wizard submit: `crates/adapteros-ui/src/pages/training/wizard.rs` lines 652-684.
- Existing provenance fields already exposed in API types: `crates/adapteros-ui/src/api/types.rs` lines 2250-2255.
