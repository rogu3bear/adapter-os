# adapterOS

## What This Is

adapterOS is a single-node ML inference platform for Apple Silicon (M1-M4 Max) that manages LoRA adapters as auditable version history. It provides deterministic multi-adapter inference with cryptographic receipts, command-first adapter operations, and training orchestration that preserves dataset lineage.

## Core Value

Deterministic, verifiable LoRA inference on Apple Silicon so every operator action and model response is reproducible and auditable.

## Current State

**Latest shipped milestone:** v1.1.17 Production Cut Closure (shipped 2026-03-04)
**Current execution milestone:** v1.1.18 System Stabilization (completed 2026-03-05)

**Known blockers:**
- No active blockers in v1.1.18 scope. Residual debt remains tracked in roadmap backlogs only.

## Current Milestone: v1.1.18 System Stabilization (Completed)

**Goal:** Fix runtime blockers and close stabilization expansion phases for portability, UI quality, and security hardening.

**Target features:**
- Training worker binary resolution so it spawns on backend boot.
- Stale socket/marker cleanup on boot (SecD, degraded markers).
- Atomic commits for the 84-file dirty tree to establish a clean baseline.
- End-to-end adapter inference activation and round-trip training verification.
- Full portability hardening (root discovery, bootstrap, fresh-clone startup).
- UI harmony cleanup with consistent tokens/skeleton/empty states.
- Performance and security hardening (UMA budget, per-tier rate limits, eviction UX, model permission hardening, structured security audit trail).

## Requirements

### Validated

- `UX-41-01`: Adapter detail surfaces repository command timeline history — v1.1.15
- `TRN-46-01`: Training start fails closed with explicit API error — v1.1.16
- `DET-46-01`: Preflight validates dataset algorithm versions — v1.1.16
- `OPS-46-01`: Primary model resolution canonicalized — v1.1.16
- `DOC-46-01`: Terminal training failures expose actionable reasons — v1.1.16
- `REL-47-01`: Prod-mode release gate strict/no-skip — v1.1.17
- `API-47-01`: Route closure matrix and allowlist policy enforced — v1.1.17
- `SEC-47-01`: Release-safe auth posture blocking — v1.1.17
- `OPS-47-01`: Runbook drill evidence and signing checks release-required — v1.1.17

### Milestone Requirements (Completed)

- `WRK-01`: Training worker spawns successfully when backend starts (binary resolution fixed).
- `WRK-02`: Training worker reports healthy in service status after boot.
- `RTH-01`: Stale SecD socket is cleaned up on boot when no backing process exists.
- `RTH-02`: Training worker degraded marker is cleared when worker successfully starts.
- `RTH-03`: Backend restart counter reflects actual crash count, not dev-rebuild kickstarts.
- `GIT-01`: All modified files committed in logical, atomic commits.
- `GIT-02`: Working tree is clean after commit series.
- `INF-51-01`, `INF-51-02`, `TRN-51-01`, `TRN-51-02`: Adapter inference activation and training round-trip behavior verified.
- `PORT-52-01`, `PORT-52-02`, `PORT-52-03`: Portability and fresh-clone startup path verified.
- `UI-53-01`, `UI-53-02`, `UI-53-03`, `A11Y-53-01`: UI harmony and accessibility parity delivered.
- `PERF-54-01`, `PERF-54-02`, `SEC-54-01`, `SEC-54-02`: Performance and security hardening delivered with targeted checks.

## Grounding Anchors (Current Implementation)

- Training preflight and enqueue safety: `crates/adapteros-server-api/src/handlers/training.rs`.
- Model lifecycle/load-state consistency: `crates/adapteros-server-api/src/handlers/models.rs`.
- Worker-model projection and compatibility status normalization: `crates/adapteros-db/src/worker_model_state.rs`.
- Tenant active-model canonical state: `crates/adapteros-db/src/workspace_active_state.rs`.
- Prod cut scope + gates: `.planning/PROD_CUT.md`.

## Constraints

- **Tech stack:** Rust, MLX (C++ FFI), Leptos 0.7, SQLite (sqlx), Axum 0.8, Tokio.
- **Hardware:** Apple Silicon only (M1-M4 Max).
- **Build:** Prefer targeted crate checks/tests; avoid broad suite churn for scoped UX/API changes.
- **Determinism:** Preserve auditable lineage and avoid semantic drift between UI language and API outcomes.
- **Design system:** Keep existing UI primitives and page architecture; no parallel adapter-operations UI path.
- **Git strategy:** `.planning/config.json` enforces `branching_strategy: none` unless explicitly changed.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Reuse adapter detail Update Center for timeline visibility | Keeps command history where promote/checkout decisions are made | Active |
| Extend existing command deck context and execute handlers instead of introducing a new command system | Minimizes diff and preserves established behavior | Active |
| Move wizard submit path to typed training contract with dataset version pinning | Improves API correctness and provenance continuity | Active |
| Keep plain-language command vocabulary aligned across UI controls and command deck | Reduces operator ambiguity and improves assistive consistency | Active |

---
*Last updated: 2026-03-05 after phase 54 completion and milestone reconciliation.*
