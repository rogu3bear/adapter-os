# Phase 9: Determinism and Compatibility Revalidation - Context

**Gathered:** 2026-02-24
**Status:** Executed and reconciled (historical planning record)

## Reconciled Execution State (2026-02-24)

This context captures planning-time intent. Phase 09 execution is complete; determinism and OpenAI revalidation passed, and governance evidence was captured with explicit external API capability limits later reconciled in Phase 11 accounting.

<domain>
## Phase Boundary

Historical planning objective: re-prove deferred determinism and OpenAI compatibility claims on then-current workspace state, and close the remaining FFI ASAN governance gap by documenting and enforcing required-check policy. This phase was evidence and governance revalidation, not net-new feature development.

</domain>

<decisions>
## Implementation Decisions

### Scope Discipline and Duplication Guard
- Reuse existing determinism/OpenAI/CI test suites and scripts; do not create parallel harnesses.
- Favor verification-first execution with minimal corrective edits only when a deferred suite fails.
- Keep changes localized to existing ownership files for determinism, OpenAI compatibility, and CI governance evidence.

### Three-Plan Decomposition for Phase 9
- Plan `09-01` closes `DET-06` by re-running the deferred replay determinism suites from Phase 03 closeouts.
- Plan `09-02` closes `API-07` by re-running full OpenAI compatibility suites and OpenAPI drift regeneration checks deferred in Phase 04 closeouts.
- Plan `09-03` closes `FFI-05` by making `ffi-asan` merge-gate governance explicit and verifiable (required-check policy + evidence).

### Wave and Dependency Strategy
- Wave 1: `09-01` and `09-02` run in parallel; they touch different evidence streams and can produce independent pass/fail signals.
- Wave 2: `09-03` depends on `09-01` and `09-02` so governance evidence can reference fresh determinism and compatibility revalidation outcomes in one closure artifact.

### Determinism Revalidation Constraints (DET-06)
- Treat the skipped heavy suites in `03-01-SUMMARY.md` and `03-02-SUMMARY.md` as mandatory reruns for this phase.
- Preserve deterministic execution settings (`--test-threads=1` where required) and existing fast-math guard path.
- Do not redefine determinism success criteria; re-prove them on current code.

### OpenAI Revalidation Constraints (API-07)
- Move beyond targeted Phase 04 closeout subsets; execute the full OpenAI compatibility suite set for current handlers/tests.
- Regenerate OpenAPI using the existing exporter/drift check flow and verify contract parity.
- Keep all compatibility behavior under existing `openai_compat` route/handler ownership.

### ASAN Governance Closure Constraints (FFI-05)
- `ffi-asan` remains in existing `.github/workflows/ci.yml`; governance closure is branch-protection enforcement, not new workflow creation.
- Required-check policy must be documented with concrete evidence (check context name(s), branch target, and active enforcement state).
- Capture both local repository evidence (workflow/job definitions) and remote policy evidence (required status checks) in summary artifacts.

### Agent/Execution Policy for This Phase
- Planning docs are tightly coupled; use one agent stream for this artifact set to avoid plan-structure drift.
- Execution can use agent teams only for separable workstreams (determinism vs OpenAI) with strict file boundaries and minimal diffs.

### Claude's Discretion
- Exact command ordering inside each plan as long as deferred suites and governance proofs are complete.
- Minimal fallback checks when environment-specific constraints block a command, with explicit residual-risk notes.
- Evidence packaging format inside `09-0x-SUMMARY.md` files.

</decisions>

<specifics>
## Specific Ideas

- Requirements for this phase: `DET-06`, `API-07`, `FFI-05`.
- Deferred signals to close from prior summaries:
  - Phase 03 deferred replay suites:
    - `cargo test --test determinism_core_suite canonical_hashing -- --test-threads=1`
    - `cargo test --test record_replay_receipt_harness -- --test-threads=1`
    - `cargo test -p adapter-os --no-default-features --test determinism_replay_harness -- --test-threads=1`
    - `cargo test -p adapteros-server-api --test replay_determinism_tests -- --test-threads=1`
  - Phase 04 deferred compatibility/export checks:
    - `cargo test -p adapteros-server-api openai_ -- --nocapture`
    - `cargo run -p adapteros-server-api --bin export-openapi -- target/codegen/openapi.json`
  - Phase 02 unresolved governance note:
    - `ffi-asan` exists, but merge-gate required-check policy still needed manual closure.
- Existing anchors:
  - `.github/workflows/ci.yml` defines `ffi-asan` (push-triggered ASAN lane).
  - `scripts/ci/check_openapi_drift.sh` is canonical OpenAPI drift gate.
  - Existing OpenAI tests live in `crates/adapteros-server-api/tests/` and should be extended/re-run in place.
- Planning artifacts for this phase: `09-01-PLAN.md`, `09-02-PLAN.md`, `09-03-PLAN.md`.

</specifics>

<deferred>
## Deferred Ideas

- New determinism features or receipt schema redesign beyond revalidation.
- New OpenAI endpoint families or compatibility layers outside current shim routes.
- CI workflow architecture refactors unrelated to `ffi-asan` required-check governance.
- Operations release sign-off tasks that belong to Phase 10.

</deferred>

---

*Phase: 09-determinism-and-compatibility-revalidation*
*Context gathered: 2026-02-24*
