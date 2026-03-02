# Phase 13: Determinism Guard Hardening - Context

**Gathered:** 2026-02-24
**Status:** Planned (ready for execution)

<domain>
## Phase Boundary

Phase 13 hardens determinism guardrails that already exist in the repository but are not yet fully operationalized as one enforceable contract.

Scope is limited to:
- automated diagnostics freshness for determinism checks,
- deterministic replay guardrails in CI and health/readiness surfaces,
- telemetry and alert surfacing for determinism guard state.

This phase is hardening and closure work. It is not a new determinism feature lane.

</domain>

<decisions>
## Implementation Decisions

### Determinism diagnostics freshness is an explicit guard, not implicit metadata
- Use existing `determinism_checks.last_run` and related diagnostics records as the freshness source of truth.
- Surface freshness semantics directly in `GET /v1/diagnostics/determinism-status` as machine-consumable fields.
- Treat stale or missing freshness signals as degraded/blocked conditions for strict determinism operations.

### Replay guardrails must be enforced in CI and reflected in runtime health
- Reuse existing CI lanes in `.github/workflows/ci.yml` (`replay-harness`, `determinism-gate`, `determinism`) and keep deterministic thread constraints for replay-sensitive tests.
- Keep runtime surfacing in existing health/readiness surfaces (`/readyz`, `/healthz/all`), not a parallel endpoint.
- Ensure replay guard state is visible to operators and cannot silently regress.

### Telemetry and alerts must surface determinism state transitions end-to-end
- Reuse current SSE/event infrastructure (`SseStreamType::Telemetry`, `SseStreamType::Alerts`) instead of introducing a new transport path.
- Expose implemented alert stream handlers through protected routes with tenant-safe filtering and replay semantics.
- Extend existing payloads/contracts; do not fork new event schemas for determinism guard state.

### Three-plan decomposition and dependency model
- `13-01` closes `DET-07` (diagnostics freshness automation).
- `13-02` closes `DET-08` (deterministic replay guardrails in CI and health/readiness).
- `13-03` closes `DET-09` (telemetry/alert surfacing and stream route exposure).
- Wave model:
  - Wave 1: `13-01` and `13-02` in parallel.
  - Wave 2: `13-03` depends on `13-01` and `13-02`.

### Claude's Discretion
- Exact freshness threshold mechanism (fixed constant vs config-backed), as long as it is explicit and tested.
- Exact readiness payload placement for replay guard signals, as long as existing health contracts remain stable.
- Minimal targeted tests needed to protect against determinism guard regressions.

</decisions>

<specifics>
## Specific Ideas

- Requirements for this phase: `DET-07`, `DET-08`, `DET-09`.
- Existing anchors in repository:
  - diagnostics writer path: `crates/adapteros-cli/src/commands/diag.rs` inserts `determinism_checks`.
  - diagnostics status endpoint: `crates/adapteros-server-api/src/handlers/diagnostics.rs` (`/v1/diagnostics/determinism-status`).
  - determinism diagnostics schema: `migrations/20260217090000_determinism_checks.sql`.
  - replay guard CI lanes: `.github/workflows/ci.yml`.
  - readiness/health ownership: `crates/adapteros-server-api/src/handlers/health.rs`.
  - SSE stream primitives: `crates/adapteros-server-api/src/sse/types.rs`, `crates/adapteros-server-api/src/sse/mod.rs`.
  - alert stream implementation exists in `crates/adapteros-server-api/src/handlers/streams/mod.rs` but route exposure in `routes/mod.rs` is incomplete.
- Planning artifacts for this phase:
  - `13-CONTEXT.md`
  - `13-RESEARCH.md`
  - `13-01-PLAN.md`
  - `13-02-PLAN.md`
  - `13-03-PLAN.md`

</specifics>

<deferred>
## Deferred Ideas

- New determinism algorithms, replay protocol redesign, or receipt schema expansion.
- New observability platform/tooling beyond existing diagnostics + SSE + monitoring surfaces.
- Non-determinism alert product expansion.
- Runtime/observability follow-through reserved for Phase 14.

</deferred>

---

*Phase: 13-determinism-guard-hardening*
*Context gathered: 2026-02-24*
