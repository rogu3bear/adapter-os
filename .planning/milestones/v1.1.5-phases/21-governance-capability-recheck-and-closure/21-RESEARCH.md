---
phase: 21-governance-capability-recheck-and-closure
created: 2026-02-26
status: ready_for_planning
---

# Phase 21: Governance Capability Recheck and Closure - Research

**Researched:** 2026-02-26
**Domain:** Canonical capability recheck, required-check enforcement proof retry, and milestone closure reconciliation
**Confidence:** HIGH

## User Constraints

### Locked Decisions (from 21-CONTEXT.md)
- Phase scope is limited to closing `GOV-16` with canonical evidence; no new governance scope is allowed.
- Canonical target is fixed (`rogu3bear/adapter-os`, `main`, `FFI AddressSanitizer (push)`).
- Latest baseline evidence shows `blocked_external` and must remain the starting truth.
- No policy write path is allowed until preflight classifies `status=capable`.
- Closure requires `status=enforced_verified` capable-path artifacts from the canonical executor.
- If still blocked, debt language remains explicit and `GOV-16` stays open.

### Claude's Discretion (from 21-CONTEXT.md)
- Polling window tuning for capability rechecks.
- Evidence packaging format.
- Reconciliation sequencing once branch outcome is known.

### Deferred Ideas (from 21-CONTEXT.md)
- Scheduled reruns for capability unlock detection.
- Additional governance domain coverage.
- Cross-organization rollout beyond approved manifest.

## Summary

Existing canonical scripts already provide all required control points for closure:
- capability classification: `scripts/ci/check_governance_preflight.sh`
- deterministic polling receipts: `scripts/ci/run_governance_capability_loop.sh`
- canonical write/readback/rollback execution contract: `scripts/ci/execute_governance_required_checks.sh`
- outcome regrading + routing receipts: `scripts/ci/audit_governance_drift.sh` and `scripts/ci/render_governance_graduation_receipts.sh`

Phase 21 should therefore be a minimal rerun/reconcile cycle:
1. capture fresh gate truth and branch decision,
2. execute canonical enforcement proof path,
3. reconcile milestone artifacts based on observed branch status.

## Standard Stack

| Library/Tool | Version | Purpose | Why Standard |
|--------------|---------|---------|--------------|
| `gh` CLI | repo standard | GitHub branch-protection read/write/readback operations | Canonical interface for governance scripts |
| `jq` | repo standard | Deterministic report transforms | Already used by governance scripts |
| `rg` | repo standard | Deterministic token checks | Existing guard checks use it |
| Bash (`set -euo pipefail`) | repo standard | Script orchestration | Existing automation surface |

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Capability check | Ad-hoc `gh api` in plans | `check_governance_preflight.sh` + `run_governance_capability_loop.sh` | Preserves gate-state and deterministic receipts |
| Canonical enforcement proof | Manual PATCH/readback sequences | `execute_governance_required_checks.sh` | Already encodes gate, verification, and rollback contracts |
| Outcome reconciliation | One-off text transforms | `audit_governance_drift.sh` + `render_governance_graduation_receipts.sh` | Keeps matrix and routing semantics consistent |

## Validation Architecture

| Property | Value |
|----------|-------|
| Framework | Deterministic shell verification (`bash`, `jq`, `rg`, `gh`) |
| Quick run command | `bash scripts/ci/check_governance_preflight.sh --repo rogu3bear/adapter-os --branch main --required-context 'FFI AddressSanitizer (push)'` |
| Full suite command | `bash scripts/ci/execute_governance_required_checks.sh --repo rogu3bear/adapter-os --branch main --required-context 'FFI AddressSanitizer (push)' --manifest docs/governance/target-manifest.json --output-dir var/evidence/governance-enforcement-rerun-<UTCSTAMP>` |
| Estimated runtime | ~30-120 seconds depending on API latency and gate status |

### Phase Requirements -> Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| GOV-16 | Canonical capable branch reaches immutable `enforced_verified` proof or emits guarded blocked/failure receipts | integration | `bash scripts/ci/execute_governance_required_checks.sh --repo rogu3bear/adapter-os --branch main --required-context 'FFI AddressSanitizer (push)' --manifest docs/governance/target-manifest.json --output-dir var/evidence/governance-enforcement-rerun-<UTCSTAMP>` | yes |
| OPS-11 | Target matrix/routing receipts regenerate from current execution window | integration | `bash scripts/ci/audit_governance_drift.sh --manifest docs/governance/target-manifest.json --output-dir var/evidence/governance-graduation-rerun-<UTCSTAMP> --fail-on drifted && bash scripts/ci/render_governance_graduation_receipts.sh --report var/evidence/governance-graduation-rerun-<UTCSTAMP>/report.json --output-dir var/evidence/governance-graduation-rerun-<UTCSTAMP>` | yes |
| AUD-01 | Planning + governance artifacts stay coherent with canonical branch truth | integration | `node /Users/star/.codex/skills/gsd-codex-runtime/runtime/get-shit-done/bin/gsd-tools.cjs validate consistency --raw` | yes |

## Planning Implications

- `21-01` should capture fresh deterministic capability receipts and emit an explicit branch decision contract.
- `21-02` should run canonical enforcement exactly once per execution window and classify branch outcome.
- `21-03` should reconcile all closure artifacts and either close `GOV-16` (`enforced_verified`) or preserve explicit debt with fresh evidence.
