---
phase: 20-capability-activation-and-strict-proof-closure
created: 2026-02-26
status: ready_for_planning
---

# Phase 20: Capability Activation and Strict Proof Closure - Research

**Researched:** 2026-02-26
**Domain:** GitHub branch-protection capability activation and deterministic governance evidence reconciliation
**Confidence:** HIGH

## User Constraints

### Locked Decisions (from 20-CONTEXT.md)
- Phase 20 stays within the fixed boundary: prove canonical capable-branch enforcement and reconcile governance debt artifacts; no new governance domains are introduced.
- Canonical target remains fixed to `rogu3bear/adapter-os` on `main` with required context `FFI AddressSanitizer (push)`.
- Capability gate remains mandatory before any policy mutation: `status=blocked_external` means strict no-write receipts (`write_attempts=0`, `policy_mutations=0`, `rollback_attempts=0`) and immediate blocker routing.
- Run deterministic preflight polling first on every execution attempt; gate-state receipt must exist before branching.
- Success requires `status=enforced_verified` with preserve/add/readback verification and rollback guard path captured in immutable artifacts.
- After canonical capable proof, regenerate approved-target matrix and route each target to `retain`, `remediate`, `escalate_blocker`, or `review_exception` based on observed outcome class.
- Milestone closeout language can only retire `HTTP 403` debt claims when canonical capable proof artifacts are present and cross-file narratives are reconciled.

### Claude's Discretion (from 20-CONTEXT.md)
- Polling window tuning (`attempts`, `sleep_seconds`) when operators need extended wait windows, provided deterministic logs are preserved.
- Evidence presentation format (single consolidated report vs split receipts) as long as machine-readable artifacts remain canonical.
- Exact sequencing for matrix regeneration and document updates, provided all required artifacts and traceability updates are completed.

### Deferred Ideas (from 20-CONTEXT.md)
- Event-driven or scheduled automation to rerun capable proof without operator invocation.
- Expanding enforcement automation beyond required status checks into additional governance policy domains.
- Cross-organization rollout semantics for repositories outside the approved target manifest.

## Summary

Phase 20 should use the existing governance script surface without introducing alternate enforcement paths. The repository already has deterministic scripts for capability polling (`run_governance_capability_loop.sh`), hard-gated canonical enforcement (`execute_governance_required_checks.sh`), and post-run multi-target reclassification (`audit_governance_drift.sh` + `render_governance_graduation_receipts.sh`). Planning should chain these surfaces directly and capture explicit branch outcomes.

The critical branch point remains external capability status. If capability is still blocked, the phase should produce deterministic no-write evidence and preserve debt posture. If capability becomes available, the phase should record `enforced_verified` canonical artifacts first, then regenerate target matrix/routing outputs and reconcile planning/governance language to observed outcomes.

**Primary recommendation:** Build Phase 20 as a three-plan sequence: gate-readiness capture -> canonical capable-path proof execution -> artifact and debt-language reconciliation.

## Standard Stack

### Core

| Library/Tool | Version | Purpose | Why Standard |
|--------------|---------|---------|--------------|
| `gh` CLI | repo standard | GitHub branch-protection read/write/readback operations | Canonical interface already used by all governance scripts |
| `jq` | repo standard | Deterministic JSON extraction/sorting and report rendering | Existing scripts rely on stable `jq` transforms |
| `rg` | repo standard | Deterministic token checks in script outputs | Existing scripts already use `rg` for guard checks |
| Bash (`set -euo pipefail`) | repo standard | Deterministic shell orchestration | Current governance automation already implemented in Bash |

### Supporting

| Tool | Purpose | When to Use |
|------|---------|-------------|
| `scripts/ci/check_governance_preflight.sh` | Classify gate state (`capable`, `blocked_external`, `misconfigured`, `error`) | Before any policy write attempt |
| `scripts/ci/run_governance_capability_loop.sh` | Poll preflight deterministically with receipt logs | To produce pre-write readiness evidence |
| `scripts/ci/execute_governance_required_checks.sh` | Canonical preserve/add/readback/rollback flow | For GOV-16 proof execution |
| `scripts/ci/audit_governance_drift.sh` | Multi-target outcome classification | For OPS-11 regrade after canonical run |
| `scripts/ci/render_governance_graduation_receipts.sh` | Deterministic matrix/routing artifacts | For operator-facing reconciliation receipts |

## Architecture Patterns

### Pattern 1: Gate-First Branching
**What:** Read-only capability classification always executes before any write path.
**When to use:** Every canonical enforcement attempt.
**Implementation anchor:** `execute_governance_required_checks.sh` preflight-before hard gate and `blocked-write-attempts.txt` receipts.

### Pattern 2: Immutable UTC-Stamped Evidence Directories
**What:** Every run writes complete receipts under `var/evidence/<flow>-<UTCSTAMP>/`.
**When to use:** All governance plan execution and reconciliation operations.
**Implementation anchor:** Existing `governance-enforcement-*` and `governance-graduation-*` directories.

### Pattern 3: Outcome-to-Action Mapping
**What:** Raw/final outcome classes map to deterministic operator actions.
**When to use:** Any post-run reporting or closeout decision.
**Implementation anchor:** `render_governance_graduation_receipts.sh` routing action mapping.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Canonical enforcement proof | Ad-hoc `gh api` command sequences in plans | `scripts/ci/execute_governance_required_checks.sh` | Script already enforces hard gate, verification matrix, and rollback path |
| Capability polling | Custom retry loops in plan tasks | `scripts/ci/run_governance_capability_loop.sh` | Existing script emits deterministic attempt logs and gate-state receipts |
| Matrix/routing generation | One-off awk/sed transformations | `audit_governance_drift.sh` + `render_governance_graduation_receipts.sh` | Existing scripts preserve canonical outcome and action semantics |

## Common Pitfalls

### Pitfall 1: Retiring debt language without capable proof
**What goes wrong:** Docs claim closure while canonical run still shows `blocked_external`.
**How to avoid:** Require `status=enforced_verified` artifact proof before changing blocker-debt statements.

### Pitfall 2: Mixing blocked and capable evidence in one narrative
**What goes wrong:** Contradictory artifact claims across PROJECT/ROADMAP/audit/checklist.
**How to avoid:** Explicitly branch reconciliation logic by observed canonical status and link only matching receipts.

### Pitfall 3: Reclassifying targets before canonical branch truth is finalized
**What goes wrong:** Target matrix is regenerated from stale assumptions.
**How to avoid:** Make matrix/routing regeneration dependent on canonical run outputs from the same execution window.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Deterministic shell command verification (`bash`, `jq`, `rg`, `gh`) |
| Config file | none (script-level contracts and exit codes) |
| Quick run command | `bash scripts/ci/check_governance_preflight.sh --repo rogu3bear/adapter-os --branch main --required-context 'FFI AddressSanitizer (push)'` |
| Full suite command | `bash scripts/ci/execute_governance_required_checks.sh --repo rogu3bear/adapter-os --branch main --required-context 'FFI AddressSanitizer (push)' --manifest docs/governance/target-manifest.json --output-dir var/evidence/governance-enforcement-exec-<UTCSTAMP>` |
| Estimated runtime | ~30-120 seconds, depends on API latency and gate status |

### Phase Requirements -> Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| GOV-16 | Canonical capable branch proves read/write/readback or guarded failure branch with rollback/no-write receipts | integration | `bash scripts/ci/execute_governance_required_checks.sh --repo rogu3bear/adapter-os --branch main --required-context 'FFI AddressSanitizer (push)' --manifest docs/governance/target-manifest.json --output-dir var/evidence/governance-enforcement-exec-<UTCSTAMP>` | yes |
| OPS-11 | Approved target matrix/routing is regenerated from current observed outcomes | integration | `bash scripts/ci/audit_governance_drift.sh --manifest docs/governance/target-manifest.json --output-dir var/evidence/governance-graduation-<UTCSTAMP> --fail-on drifted && bash scripts/ci/render_governance_graduation_receipts.sh --report var/evidence/governance-graduation-<UTCSTAMP>/report.json --output-dir var/evidence/governance-graduation-<UTCSTAMP>` | yes |
| AUD-01 | Planning/audit/checklist files are coherent with observed run outcomes | integration | `node /Users/star/.codex/skills/gsd-codex-runtime/runtime/get-shit-done/bin/gsd-tools.cjs validate consistency --raw` | yes |

### Nyquist Sampling Rate

- **Minimum sample interval:** after each committed task, run the preflight quick command and capture output to evidence.
- **Full suite trigger:** before closing Plan 02 and before Plan 03 reconciliation updates.
- **Phase-complete gate:** governance run artifacts + planning reference checks must pass before `/gsd:verify-work 20`.

## Planning Implications

- `20-01` should capture deterministic capability readiness receipts and explicit branch decision evidence.
- `20-02` should execute canonical enforcement proof path and classify result (`enforced_verified` vs guarded blocker branches).
- `20-03` should regenerate multi-target matrix/routing and reconcile planning/governance/audit wording to observed truth.
