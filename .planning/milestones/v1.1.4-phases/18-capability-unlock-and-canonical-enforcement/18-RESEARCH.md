---
phase: 18-capability-unlock-and-canonical-enforcement
created: 2026-02-25
status: ready_for_planning
---

# Phase 18 Research: Capability Unlock and Canonical Enforcement

## Problem Statement

Phase 17 proved parity with explicit approved exceptions, but strict enforcement closure remains unverified while required-check API capability is externally blocked. `GOV-14` and `GOV-15` require a deterministic unlock-aware path that preserves no-write behavior when blocked and executes canonical read/write/readback + rollback artifacts when capability becomes available.

## Inputs Reviewed

- `.planning/PROJECT.md`
- `.planning/ROADMAP.md`
- `.planning/STATE.md`
- `.planning/REQUIREMENTS.md`
- `.planning/phases/18-capability-unlock-and-canonical-enforcement/18-CONTEXT.md`
- `.planning/milestones/v1.1.3-phases/17-multi-repo-parity-proof/17-VERIFICATION.md`
- `docs/governance/README.md`
- `docs/governance/target-manifest.json`
- `scripts/ci/check_governance_preflight.sh`
- `scripts/ci/audit_governance_drift.sh`

## Locked Constraints

1. Preserve deterministic no-write behavior whenever preflight is not `capable`.
2. Do not claim strict enforcement completion without write/readback proof and rollback evidence.
3. Keep capability outcome taxonomy explicit: `capable`, `blocked_external`, `misconfigured`, `error`.
4. Keep evidence immutable, timestamped, and reproducible from repo-local commands.

## Required API/Command Surface

- `bash scripts/ci/check_governance_preflight.sh --repo rogu3bear/adapter-os --branch main --required-context 'FFI AddressSanitizer (push)'`
- `gh api repos/rogu3bear/adapter-os/branches/main/protection/required_status_checks`
- `gh api --method PATCH repos/rogu3bear/adapter-os/branches/main/protection/required_status_checks ...`
- `bash scripts/ci/audit_governance_drift.sh --manifest docs/governance/target-manifest.json --output-dir var/evidence/governance-enforcement-<UTCSTAMP> --fail-on drifted`

## Risk Analysis

1. Capability may remain externally blocked; blocked branch must still close with deterministic no-write evidence and explicit debt posture.
2. Write/readback path could diverge from expected context union/strict behavior; pre/post snapshots and rollback proof are mandatory.
3. Contradictory artifact language can produce false closure claims; reconciliation pass must align planning/governance docs to observed outcome branch.

## Verification Strategy

- Validate preflight status transitions and deterministic logging across retries.
- Verify blocked branch produces no write-attempt artifacts.
- Verify capable branch produces pre-read, write receipt, post-read, rollback receipt, and post-rollback readback.
- Verify roadmap/state/verification/UAT artifacts point to the same branch truth.

## Planning Implications

- 18-01 should harden capability gate and deterministic unlock detection artifacts.
- 18-02 should execute blocked-branch path with explicit no-write safety and operator-ready evidence.
- 18-03 should execute capable-branch canonical enforcement + rollback verification and reconcile milestone artifacts.
