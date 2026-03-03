---
phase: 19-multi-repo-enforcement-graduation
created: 2026-02-25
status: ready_for_planning
---

# Phase 19 Research: Multi-Repo Enforcement Graduation

## Problem Statement

Phase 17 proved multi-repo parity in approved-exception posture and Phase 18 establishes canonical capability-aware enforcement flow. `OPS-10` now requires graduation to target-level enforcement posture across approved repositories with deterministic outcome classification and actionable CI/operator routing.

## Inputs Reviewed

- `.planning/PROJECT.md`
- `.planning/ROADMAP.md`
- `.planning/STATE.md`
- `.planning/REQUIREMENTS.md`
- `.planning/phases/19-multi-repo-enforcement-graduation/19-CONTEXT.md`
- `.planning/phases/18-capability-unlock-and-canonical-enforcement/18-VERIFICATION.md`
- `docs/governance/README.md`
- `docs/governance/target-manifest.json`
- `scripts/ci/validate_governance_target_manifest.sh`
- `scripts/ci/audit_governance_drift.sh`

## Locked Constraints

1. Preserve deterministic read-only behavior for targets that remain externally blocked.
2. Keep per-target outcome classes explicit and reproducible from artifact evidence.
3. Do not report global enforcement closure if any target is drifted or unapproved blocked state.
4. Keep CI/operator routing deterministic, actionable, and aligned to manifest-approved exceptions.

## Required API/Command Surface

- `bash scripts/ci/validate_governance_target_manifest.sh --manifest docs/governance/target-manifest.json`
- `bash scripts/ci/audit_governance_drift.sh --manifest docs/governance/target-manifest.json --output-dir var/evidence/governance-graduation-<UTCSTAMP> --fail-on drifted`
- `gh api repos/<repo>/branches/<branch>/protection/required_status_checks`

## Risk Analysis

1. Target capability can be heterogeneous; mixed outcomes need explicit routing rather than single-state assumptions.
2. Approved-exception metadata can drift from real operator intent; stale exceptions must be surfaced in reconciliation outputs.
3. CI signaling can become ambiguous if fail-on policy and exception handling diverge.

## Verification Strategy

- Validate manifest structure and target inventory before multi-target execution.
- Capture deterministic matrix receipts mapping each target to final and raw outcomes.
- Verify CI/operator docs and milestone artifacts match observed target-level outcomes.

## Planning Implications

- 19-01 should lock enforcement-ready target policy and matrix schema.
- 19-02 should execute multi-target capability-aware run with deterministic evidence outputs.
- 19-03 should reconcile closure artifacts and publish audit-ready graduation package.
