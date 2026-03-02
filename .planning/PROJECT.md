# adapterOS

## What This Is

adapterOS is a single-node ML inference platform for Apple Silicon (M1-M4 Max) that manages LoRA adapters as auditable version history. It provides deterministic multi-adapter inference with cryptographic receipts, command-first adapter operations, and training orchestration that preserves dataset lineage.

## Core Value

Deterministic, verifiable LoRA inference on Apple Silicon so every operator action and model response is reproducible and auditable.

## Current State

**Latest shipped milestone:** v1.1.16 Training Pipeline Execution Hardening (shipped 2026-02-28)
**Current execution milestone:** v1.1.17 Production Cut Closure (in progress; canonical spec in `.planning/PROD_CUT.md`)
**Current go/no-go posture (2026-03-02):** local release governance preflight is optional and defaults to disabled for local packaging.

v1.1.16 is complete in top-level planning artifacts with phase-46 outputs under `.planning/phases/46-training-pipeline-execution-hardening/`.

**Accepted external debt posture:**
- `GOV-16` remains accepted external debt (`blocked_external`, `HTTP 403`) and is out of scope for v1.1.15.
- `GOV-16` remains accepted external debt (`blocked_external`, `HTTP 403`) and remains out of scope for v1.1.16.
- For v1.1.17 local execution, governance preflight is configurable (`LOCAL_RELEASE_GOVERNANCE_MODE=off|warn|enforce`) and defaults to `off`.

## Current Milestone: v1.1.17 Production Cut Closure (In Progress)

**Goal:** Deliver one production cut with strict no-skip release gating, route contract closure governance, hardened startup/determinism/security controls, runbook drill evidence requirements, and signed release artifacts.

**Target features:**
- Route closure artifacts and strict route/openapi contract checks.
- Prod-mode release gate (`scripts/ci/local_release_gate_prod.sh`) with inference + full smoke enforcement.
- Startup negative-path, determinism allowlist governance, and release security assertions in required checks.
- SBOM/provenance/signing enforcement with verification-log output.
- Runbook drill evidence validation in `.planning/prod-cut/evidence/runbooks/`.

**Current receipt state (2026-03-02):**
- Runbook strict evidence: pass.
- Signed release artifacts + verification log: pass.
- Prod required checks (prod profile, all-targets clippy): pass.
- Governance preflight: optional in local release path (default `off`), with `warn` and `enforce` modes available.
- Final receipt: `.planning/prod-cut/evidence/final-go-no-go.md`.

## Requirements

### Active Requirements

- `UX-41-01`: Adapter detail surfaces repository command timeline history to support command-aware decisions.
- `TRN-46-01`: Training start fails closed with explicit API error when no healthy training worker is available.
- `DET-46-01`: Preflight validates dataset algorithm versions before enqueue.
- `OPS-46-01`: Primary model resolution is canonicalized across training and model-status paths.
- `DOC-46-01`: Terminal training failures expose actionable reasons and are citation-grounded.
- `REL-47-01`: Prod-mode release gate is strict/no-skip and blocks governance `blocked_external` outcomes.
- `API-47-01`: Runtime/OpenAPI route closure matrix and strict allowlist policy are enforced.
- `SEC-47-01`: Release-safe auth posture and tenant-isolation assertions are blocking.
- `OPS-47-01`: Runbook drill evidence and release artifact signing/provenance checks are release-required.

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
*Last updated: 2026-03-02 after prod-cut rehearsal receipts and final no-go publication.*
