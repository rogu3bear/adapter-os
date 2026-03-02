# Phase 8: Observability and Runtime Evidence Closure - Context

**Gathered:** 2026-02-24
**Status:** Executed and reconciled (historical planning record)

## Reconciled Execution State (2026-02-24)

This context captures planning-time scope and constraints. Phase 08 execution is complete, and previously observed runtime blockers were resolved in post-closeout rectification; see `08-03-SUMMARY.md`, `08-VERIFICATION.md`, and `.planning/milestones/v1.1-MILESTONE-AUDIT.md` for current closure state.

<domain>
## Phase Boundary

Historical planning objective: close the remaining runtime-observability and evidence gaps left open by prior closeouts by fixing known `streaming_infer` regressions, completing deferred manual observability evidence capture, and revalidating runtime fidelity (`foundation-run` and live TUI/backend behavior) on then-current code. This phase was closure and verification work, not new observability feature development.

</domain>

<decisions>
## Implementation Decisions

### Streaming Regression Closure First (OBS-06, OBS-07)
- Start by reproducing the exact failing `streaming_infer` tests recorded in Phase 05 summary before making any code changes.
- Split fixes into two focused plans to avoid mixed-cause debugging:
  - `08-01` for drain semantics (`OBS-06`)
  - `08-02` for worker-unavailable error envelope stability (`OBS-07`)
- Keep fixes inside existing streaming + middleware + error mapping paths; no new routes, no parallel streaming paths.

### Deferred Manual Observability Evidence Must Be Closed
- Carry forward and close the deferred manual evidence from Phase 05 summary:
  - live `/metrics` scrape sampling
  - trace-chain continuity inspection
  - SIGTERM drain timeline capture
- Evidence must be materialized as concrete artifacts (logs/files) and linked in phase summary output.

### Runtime Fidelity Revalidation Is Mandatory (UX-05)
- Re-run `scripts/foundation-run.sh` on current code and retain fresh pass/fail evidence.
- Re-run live TUI/backend fidelity checks against current API payloads (`/api/metrics`, `/api/adapters`) and record concrete observations.
- Treat this as current-state revalidation, not a TUI redesign or endpoint expansion.

### Three-Plan Execution Topology
- Requirement mapping across plans is fixed:
  - `08-01-PLAN.md` -> `OBS-06`
  - `08-02-PLAN.md` -> `OBS-07`
  - `08-03-PLAN.md` -> `UX-05` plus closure of deferred manual evidence tasks
- `08-03` runs after `08-01` and `08-02` so revalidation evidence is collected on post-fix code.

### Claude's Discretion
- Exact patch shape to restore drain/error semantics, as long as it stays inside existing abstractions.
- Whether to add or tighten targeted assertions in existing tests when needed for stability.
- Evidence packaging details (single Phase 08 evidence folder vs split files) as long as required links remain explicit.

</decisions>

<specifics>
## Specific Ideas

- Requirements for this phase: `OBS-06`, `OBS-07`, `UX-05`.
- Carried-forward failure signals from Phase 05 summary:
  - `draining_rejects_new_requests_while_allowing_in_flight_stream_to_complete` expected `503`, got `200`
  - `streaming_infer_emits_structured_error_on_unavailable_resource` failed with worker-unavailable path
  - `streaming_infer_resolves_effective_adapters_from_session_stack` failed with worker-unavailable path
- Carried-forward deferred evidence from Phase 05 summary:
  - manual `/metrics` scrape
  - manual trace-chain inspection
  - manual SIGTERM drain timeline capture
- Carried-forward revalidation gap from Phase 07 summary:
  - `foundation-run` and live TUI fidelity were not re-run in closeout.
- Planning artifacts needed in this phase:
  - `08-CONTEXT.md`
  - `08-RESEARCH.md`
  - `08-01-PLAN.md`
  - `08-02-PLAN.md`
  - `08-03-PLAN.md`

</specifics>

<deferred>
## Deferred Ideas

- New observability features or metric families beyond fixing current requirement gaps.
- Streaming API redesign, alternate transport paths, or broad error taxonomy rework outside targeted failure closure.
- New TUI screens or UX refactors unrelated to fidelity revalidation.

</deferred>

---

*Phase: 08-observability-and-runtime-evidence-closure*
*Context gathered: 2026-02-24*
