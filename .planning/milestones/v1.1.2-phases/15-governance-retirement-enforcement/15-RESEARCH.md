---
phase: 15-governance-retirement-enforcement
created: 2026-02-25
status: ready_for_planning
---

# Phase 15 Research: Governance Retirement Enforcement

## Problem Statement

Retire remaining governance debt `FFI-05` by proving branch-protection enforcement is operational on the canonical private repository target. Closure requires verifiable read/write/read evidence plus planning artifact reconciliation.

## Inputs Reviewed

- `.planning/PROJECT.md`
- `.planning/ROADMAP.md`
- `.planning/STATE.md`
- `.planning/REQUIREMENTS.md`
- `scripts/ci/check_governance_preflight.sh`

## Locked Constraints

1. Keep repository private; unblock path is GitHub plan capability upgrade.
2. Enforce full required check set, not only `FFI AddressSanitizer (push)`.
3. Use additive preserve+add semantics for required contexts.
4. Hard gate before write:
   - Exit `20` (`blocked_external`) means evidence capture only.
   - Exit `0` (`capable`) allows policy write.

## Required API/Command Surface

- Preflight command:
  - `bash scripts/ci/check_governance_preflight.sh --repo rogu3bear/adapter-os --branch main --required-context 'FFI AddressSanitizer (push)'`
- Branch protection endpoint:
  - `repos/rogu3bear/adapter-os/branches/main/protection/required_status_checks`
- Read baseline and post-write states with `gh api`.
- Use `jq` union/sort for context-set preservation checks.

## Risk Analysis

1. External capability still blocked (`HTTP 403`): no write path; planning closure must not proceed.
2. Write regression removes existing required context: must detect pre/post diff and rollback immediately.
3. Wrong branch/repo target (`404/422`): fail explicit, preserve baseline.
4. False closure risk in docs: only update debt posture after successful `capable` + context verification.

## Verification Strategy

- Technical gates:
  - Preflight before and after write.
  - Explicit context inclusion checks for canonical set.
  - Explicit context preservation check against `pre-read.json`.
- Artifact gates:
  - `pre-read.json`, `write.json`, `post-read.json`, `preflight-before.log`, `preflight-after.log`, `verification.txt`.
- Documentation gates:
  - No residual unresolved debt language in milestone/project artifacts.

## Validation Architecture

1. Layer 1: Capability gate (`blocked_external` vs `capable`) with no side effects.
2. Layer 2: Enforcement gate (required-status-check PATCH + immediate readback).
3. Layer 3: Preservation gate (pre/post context diff and rollback path).
4. Layer 4: Narrative consistency gate (`.planning/*` and governance docs aligned to observed truth).

## Planning Implications

- Plan 01 should terminate safely on external blocker and persist evidence.
- Plan 02 should be the only policy write surface and must include rollback guard.
- Plan 03 should own cross-file wording reconciliation and final acceptance checks.
