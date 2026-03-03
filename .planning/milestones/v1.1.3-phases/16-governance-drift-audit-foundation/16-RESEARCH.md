---
phase: 16-governance-drift-audit-foundation
created: 2026-02-25
status: ready_for_planning
---

# Phase 16 Research: Governance Drift Audit Foundation

## Problem Statement

Phase 15 closed with accepted external blocker truth for canonical required-check enforcement (`blocked_external`, `HTTP 403`). The next milestone step is to prevent silent governance drift by introducing deterministic read-only audits and operator response guidance without mutating branch protection.

## Inputs Reviewed

- `.planning/PROJECT.md`
- `.planning/ROADMAP.md`
- `.planning/STATE.md`
- `.planning/REQUIREMENTS.md`
- `.planning/research/v1.1.3-GOVERNANCE-DRIFT-PREPLAN.md`
- `scripts/ci/check_governance_preflight.sh`
- `docs/governance/README.md`

## Locked Constraints

1. No governance writes in Phase 16 (read-only audits only).
2. Target inventory must be explicit and versioned; do not infer targets dynamically.
3. Reports must be deterministic and reproducible for the same manifest snapshot.
4. External blockers and approved exceptions must be first-class outcome states.

## Required API/Command Surface

- Existing capability probe path:
  - `bash scripts/ci/check_governance_preflight.sh --repo <repo> --branch <branch> --required-context '<context>'`
- Read-only required-check endpoint:
  - `repos/<owner>/<repo>/branches/<branch>/protection/required_status_checks`
- Deterministic artifact/report outputs:
  - `var/evidence/governance-drift-<UTCSTAMP>/report.json`
  - `var/evidence/governance-drift-<UTCSTAMP>/report.txt`

## Risk Analysis

1. Manifest drift (target set changes without review) can produce false confidence; manifest validation and ownership fields must be explicit.
2. Ambiguous output classes can cause operator mis-triage; classification rules must be deterministic and documented.
3. Rate-limit/auth variability could make reports flaky; capture explicit API status + error class per target.
4. Narrative drift risk persists if docs claim parity completion before Phase 17 evidence exists.

## Verification Strategy

- Technical gates:
  - Deterministic manifest validation.
  - Deterministic JSON report schema validation.
  - Re-run consistency check on the same manifest snapshot.
- Artifact gates:
  - Manifest file + validator output.
  - Drift report (`json` + human summary).
  - CI/check-only run transcript.
- Documentation gates:
  - Governance runbook maps each outcome class to operator response.

## Validation Architecture

1. Layer 1: Manifest validity gate (schema + unique targets + canonical context-set reference).
2. Layer 2: Read-only capture gate (per-target API reads with explicit status capture).
3. Layer 3: Drift classification gate (compliant/drifted/blocked_external/approved_exception).
4. Layer 4: Operator handoff gate (CI surface + runbook response matrix).

## Planning Implications

- Plan 01 must establish the manifest contract and deterministic validation.
- Plan 02 must implement the read-only audit runner and stable report outputs.
- Plan 03 must wire CI/check-only execution and document operator response procedures.
