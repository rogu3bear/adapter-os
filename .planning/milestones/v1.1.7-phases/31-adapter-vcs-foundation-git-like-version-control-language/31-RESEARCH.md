---
phase: 31-adapter-vcs-foundation-git-like-version-control-language
created: 2026-02-28
status: ready_for_planning
---

# Phase 31: Adapter VCS Foundation: Git-Like Language and Checkout UX - Research

**Researched:** 2026-02-28
**Domain:** Adapter version-control UX alignment with existing repository and dataset primitives
**Confidence:** HIGH

## Evidence Highlights

The repo already ships a dedicated git integration crate and branch/session lifecycle model, so "git-like" is not aspirational language: it is aligned with existing architecture. The adapter UI also already exposes version selectors and dataset lineage, which means we can convert operator mental model quickly by renaming operations and tightening flow entry points.

The smallest native path is to reframe existing operations as commit/branch history actions, not to introduce net-new storage, branching, or replay systems.

## Constraints and Guardrails

- Keep backend contracts stable during the language shift; preserve compatibility with existing rollback route and handlers.
- Reuse existing version selector and dataset lineage structures rather than adding duplicate state.
- Keep changes deterministic and scoped to adapter surfaces; avoid unrelated restore semantics in non-adapter domains (for example, flight recorder restore points).

## Planning Implications

Phase 31 should target minimal-diff, high-leverage UX/API naming alignment and a direct dataset feed CTA from adapter version controls. This gives immediate user value while preserving compatibility and sets up Phase 32 for deeper branch-aware feed semantics.

## Citations

- Git session lifecycle and branch naming substrate: `crates/adapteros-git/src/branch_manager.rs` lines 27-47 and 102-143.
- Git integration plugin surface and responsibilities: `crates/adapteros-git/src/lib.rs` lines 16-29 and 157-161.
- Version selector and resolve behavior in adapter UI: `crates/adapteros-ui/src/components/adapter_detail_panel.rs` lines 551-592 and 720-750.
- Dataset lineage UI and trust snapshots per version: `crates/adapteros-ui/src/components/adapter_detail_panel.rs` lines 842-1007.
- Dataset-fed adapter training endpoint usage in UI client and wizard: `crates/adapteros-ui/src/api/client.rs` lines 796-807 and `crates/adapteros-ui/src/pages/training/wizard.rs` lines 661-684.
- Existing route/query contract for wizard auto-open and return path: `crates/adapteros-ui/src/pages/training/mod.rs` lines 143-204.
