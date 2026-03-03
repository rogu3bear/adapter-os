---
phase: 17-multi-repo-parity-proof
created: 2026-02-25
status: ready_for_planning
---

# Phase 17 Research: Multi-Repo Parity Proof

## Problem Statement

Phase 16 delivered deterministic read-only drift auditing for a single canonical target. `OPS-09` requires parity proof across an approved multi-repo target set while preserving explicit handling for externally blocked targets and approved exceptions.

## Inputs Reviewed

- `.planning/PROJECT.md`
- `.planning/ROADMAP.md`
- `.planning/STATE.md`
- `.planning/REQUIREMENTS.md`
- `.planning/phases/17-multi-repo-parity-proof/17-CONTEXT.md`
- `.planning/phases/16-governance-drift-audit-foundation/16-VERIFICATION.md`
- `docs/governance/target-manifest.json`
- `scripts/ci/validate_governance_target_manifest.sh`
- `scripts/ci/audit_governance_drift.sh`

## Locked Constraints

1. Keep parity proof read-only (no branch-protection write/remediation actions).
2. Approved target set must be explicit in manifest and reproducible.
3. External blockers must remain explicit and evidence-backed.
4. No false closure: parity claims must include exception/blocker truth where present.

## Required API/Command Surface

- `gh repo list rogu3bear --limit <N>` for candidate target discovery.
- `bash scripts/ci/validate_governance_target_manifest.sh --manifest docs/governance/target-manifest.json`
- `bash scripts/ci/audit_governance_drift.sh --manifest docs/governance/target-manifest.json --output-dir var/evidence/governance-parity-<UTCSTAMP> --fail-on drifted`

## Risk Analysis

1. Private-repo API capability constraints may keep targets in `blocked_external`; parity closure must treat this as explicit exceptions, not silent pass.
2. Manifest expansion could introduce malformed targets; validation gate must run before parity capture.
3. Mixed target capabilities could produce true drifted outcomes; fail-on behavior should block closure unless explicitly approved and documented.

## Verification Strategy

- Manifest validity gate for expanded target set.
- Deterministic parity report artifacts (`report.json`, `report.txt`, parity matrix/exceptions files).
- Planning reconciliation gate: requirements/roadmap/state/milestones aligned to parity evidence.

## Planning Implications

- 17-01 should finalize and evidence an approved multi-repo target set.
- 17-02 should run parity verification and emit explicit exception evidence.
- 17-03 should reconcile closure artifacts with no false parity claims.
