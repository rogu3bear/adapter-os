---
phase: "16"
name: "Governance Drift Audit Foundation"
created: 2026-02-25
---

# Phase 16: Governance Drift Audit Foundation — Context

## Decisions

- Phase 16 remains strictly read-only for governance APIs: no branch-protection PATCH/write operations are allowed.
- Target scope is controlled by an explicit manifest; ad hoc repo/branch probing is out of scope.
- Drift evidence must be deterministic and timestamped under `var/evidence/governance-drift-<UTCSTAMP>/`.
- Outcome classes are fixed for this phase: `compliant`, `drifted`, `blocked_external`, `approved_exception`.
- External blocker truth (`HTTP 403` on canonical write path) must remain explicit in all phase outputs.

## Discretion Areas

- Exact manifest file format (JSON vs YAML) as long as validation is deterministic and machine-checkable.
- Implementation shape for report generation (single script vs helper module) if output contract remains stable.
- CI invocation strategy (dedicated workflow vs existing governance check extension) if execution stays read-only.

## Deferred Ideas

- Automated remediation/patch mode for drifted targets (defer until capability, policy approvals, and rollback model are defined).
- Expansion beyond required status checks (for example, review dismissal rules, admin restrictions).
- Cross-organization governance rollout automation.
