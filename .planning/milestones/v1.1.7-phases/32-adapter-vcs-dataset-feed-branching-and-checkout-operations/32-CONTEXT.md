---
phase: "32"
name: "Adapter VCS Dataset Feed, Branching, and Checkout Operations"
created: 2026-02-28
status: discussed
mode: delegated
---

# Phase 32: Adapter VCS Dataset Feed, Branching, and Checkout Operations - Context

## Decisions

- We will keep backward compatibility with rollback route semantics while introducing explicit checkout semantics for new clients and docs.
- We will make dataset-fed evolution branch-aware by carrying branch/version context through training entry points where feasible.
- We will preserve deterministic auditability by ensuring each checkout/promotion/feed operation remains traceable via existing version and lineage surfaces.

## Baseline Entering Phase 32

After Phase 31, language and entry flow should read git-like, but server route naming and deeper branch-aware dataset feed contracts may still rely on rollback-era terms.

## Phase 32 Focus

Phase 32 moves from language alignment to operational contract clarity:

1. Checkout-oriented API and route semantics (with compatibility fallback).
2. Branch-aware dataset feed continuity.
3. Explicit evidence that adapter update workflows remain auditable and deterministic.

## Citations

- Current server route exposes rollback endpoint only: `crates/adapteros-server-api/src/routes/adapters.rs` lines 62-64.
- UI client currently exposes rollback method as mutating operation: `crates/adapteros-ui/src/api/client.rs` lines 573-592.
- Training query ingestion currently supports dataset and return path but not explicit branch context: `crates/adapteros-ui/src/pages/training/mod.rs` lines 143-204.
- Existing adapter version selector supports branch/tag patterns suitable for branch-aware wiring: `crates/adapteros-ui/src/components/adapter_detail_panel.rs` lines 556-575 and 748-750.
