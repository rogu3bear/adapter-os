---
phase: "20"
name: "Capability Activation and Strict Proof Closure"
created: 2026-02-26
status: discussed
mode: delegated
---

# Phase 20: Capability Activation and Strict Proof Closure — Context

## Decisions

- Phase 20 stays within the fixed boundary: prove canonical capable-branch enforcement and reconcile governance debt artifacts; no new governance domains are introduced.
- Canonical target remains fixed to `rogu3bear/adapter-os` on `main` with required context `FFI AddressSanitizer (push)`.
- Capability gate remains mandatory before any policy mutation: `status=blocked_external` means strict no-write receipts (`write_attempts=0`, `policy_mutations=0`, `rollback_attempts=0`) and immediate blocker routing.
- Discuss-area decision (Capability gating): run deterministic preflight polling first on every execution attempt; gate-state receipt must exist before branching.
- Discuss-area decision (Canonical proof criteria): success requires `status=enforced_verified` with preserve/add/readback verification and rollback guard path captured in immutable artifacts.
- Discuss-area decision (Outcome regrading): after canonical capable proof, regenerate approved-target matrix and route each target to `retain`, `remediate`, `escalate_blocker`, or `review_exception` based on observed outcome class.
- Discuss-area decision (Debt retirement policy): milestone closeout language can only retire `HTTP 403` debt claims when canonical capable proof artifacts are present and cross-file narratives are reconciled.

## Discretion Areas

- Polling window tuning (`attempts`, `sleep_seconds`) when operators need extended wait windows, provided deterministic logs are preserved.
- Evidence presentation format (single consolidated report vs split receipts) as long as machine-readable artifacts remain canonical.
- Exact sequencing for matrix regeneration and document updates, provided all required artifacts and traceability updates are completed.

## Deferred Ideas

- Event-driven or scheduled automation to rerun capable proof without operator invocation.
- Expanding enforcement automation beyond required status checks into additional governance policy domains.
- Cross-organization rollout semantics for repositories outside the approved target manifest.
