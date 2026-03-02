---
phase: 32-adapter-vcs-dataset-feed-branching-and-checkout-operations
created: 2026-02-28
status: ready_for_planning
---

# Phase 32: Adapter VCS Dataset Feed, Branching, and Checkout Operations - Research

**Researched:** 2026-02-28
**Domain:** Backward-compatible checkout semantics and branch-aware dataset feed routing
**Confidence:** MEDIUM-HIGH

## Evidence Highlights

The system already has the pieces needed for branch-aware adapter version control: repository-scoped version listing, selector resolution, promote/rollback mutation routes, and dataset provenance snapshots. The principal gap is consistency across naming, route surface, and training-entry context.

A compatibility strategy is straightforward: introduce checkout naming as primary while keeping rollback endpoints functional until all clients migrate.

## Constraints and Guardrails

- Avoid API contract breaks without an explicit migration window.
- Keep OpenAPI and client semantics aligned whenever route aliases are introduced.
- Prefer minimal additions to existing training query contract, then wire through current wizard state flow.

## Planning Implications

Phase 32 should deliver a compatibility-first contract pass, then branch-aware dataset feed continuity, with targeted verification against adapter UI and API routes.

## Citations

- Current adapter routes and rollback action endpoint: `crates/adapteros-server-api/src/routes/adapters.rs` lines 58-68.
- Current UI client adapter version control methods: `crates/adapteros-ui/src/api/client.rs` lines 524-592.
- Existing dataset initialization and query ingestion in training page: `crates/adapteros-ui/src/pages/training/mod.rs` lines 160-204.
- Existing dataset-based training job construction path: `crates/adapteros-ui/src/pages/training/wizard.rs` lines 652-684.
- Existing lineage trust data model in version summaries: `crates/adapteros-ui/src/api/types.rs` lines 2250-2267.
