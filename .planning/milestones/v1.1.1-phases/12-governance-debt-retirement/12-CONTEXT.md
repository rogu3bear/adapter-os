# Phase 12: Governance Debt Retirement - Context

**Gathered:** 2026-02-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Retire governance debt that remains after Phase 11 reconciliation by making branch-protection retirement operationally executable, automating governance preflight checks, and hardening planning traceability so requirement/audit status cannot drift.

This phase is governance/process hardening only. It must not introduce unrelated runtime, API, or UI feature work.

</domain>

<decisions>
## Implementation Decisions

### Treat debt retirement as a capability-gated playbook
- Reuse the established governance truth from Phase 11: branch-protection required-check APIs are externally gated in this environment (`HTTP 403`).
- Define one explicit retirement playbook that handles both outcomes:
  - capability available: execute read/write/read proof and retire debt;
  - capability blocked: preserve debt state with explicit owner/prerequisite.
- Do not reopen `FFI-05` narrative ambiguity or invent alternate closure paths.

### Add one canonical governance preflight automation path
- Extend existing script conventions under `scripts/ci/` with a dedicated governance preflight check.
- Preflight must resolve repo/branch/context deterministically and emit machine-usable pass/blocker signals.
- Keep automation read-oriented by default; no policy writes during preflight.

### Harden requirement/audit traceability as a contract
- Keep requirement, roadmap, state, and milestone audit records synchronized for governance debt status.
- Explicitly map new Phase 12 governance requirements (`GOV-06`, `GOV-07`, `GOV-08`) to concrete plan outputs.
- Avoid parallel tracking files; use existing `.planning/` sources of truth.

### Plan decomposition and dependency model
- `12-01`: branch-protection capability retirement playbook (`GOV-06`)
- `12-02`: governance preflight automation (`GOV-07`)
- `12-03`: requirements/audit traceability hardening (`GOV-08`)
- Dependency chain is sequential to prevent automation/traceability drifting ahead of playbook semantics.

### Claude's Discretion
- Exact preflight script interface/exit-code model as long as outcomes are deterministic and actionable.
- Minimal wording shape for governance docs/checklist updates as long as capability-gate semantics are explicit.
- Verification command ordering for artifact integrity and traceability checks.

</decisions>

<specifics>
## Specific Ideas

- Requirements for this phase: `GOV-06`, `GOV-07`, `GOV-08`.
- Inputs to carry forward from completed Phase 11:
  - canonical repo/branch context: `rogu3bear/adapter-os`, `main`
  - canonical required-check context: `FFI AddressSanitizer (push)`
  - confirmed external capability blocker pattern: branch-protection API `HTTP 403` in this environment
- Core outputs expected from this phase:
  - debt-retirement playbook for branch-protection capability transition
  - preflight automation path suitable for CI/operator use
  - hardened requirement/audit traceability for governance debt accounting

</specifics>

<deferred>
## Deferred Ideas

- Repository billing/visibility changes themselves (external platform prerequisite).
- New CI lane families or non-governance workflow redesign.
- Cross-phase hardening work owned by Phases 13 and 14.

</deferred>

---

*Phase: 12-governance-debt-retirement*
*Context gathered: 2026-02-24*
