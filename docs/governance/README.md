# Governance

Policy and compliance.

---

## Branch Protection Notes

- Merge-gate governance for sanitizer safety depends on required status checks at the protected default branch.
- AdapterOS tracks the context label `FFI AddressSanitizer (push)` as governance evidence, but required release gates are executed locally via `scripts/ci/local_required_checks.sh` and `scripts/ci/local_release_gate.sh` (no remote workflow requirement).
- Live GitHub verification on 2026-03-06 showed the canonical target `rogu3bear/adapter-os` is public and branch-protection APIs are capable. Historical `blocked_external` receipts for the canonical target are retained for audit history only and should not be treated as current governance truth.
- Local release policy (effective 2026-03-03): `scripts/ci/local_release_gate.sh` defaults governance preflight to `LOCAL_RELEASE_GOVERNANCE_MODE=enforce`, failing closed by default while allowing manifest-approved `blocked_external` exceptions.
- Optional governance overrides remain available:
  - `LOCAL_RELEASE_GOVERNANCE_MODE=warn` (non-blocking evidence lane)
  - `LOCAL_RELEASE_GOVERNANCE_MODE=off` (skip governance preflight for local-only packaging/debug scenarios)
- Canonical governance retirement target is pinned to repository `rogu3bear/adapter-os`, branch `main`, required context `FFI AddressSanitizer (push)`.

### Branch-Protection Retirement Playbook (GOV-06)

#### Outcome Classes

- `capable`: branch-protection `required_status_checks` read/write/read succeeds and post-read contains `FFI AddressSanitizer (push)`.
- `blocked_external`: GitHub required-check API returns `HTTP 403` (for example, upgrade/plan limitation).
- `misconfigured`: wrong repository/branch/context target or required context missing after successful reads.
- `error`: non-policy execution failure (auth/network/CLI/runtime).

#### Canonical Command Sequence (read/write/read)

```bash
REPO="rogu3bear/adapter-os"
BRANCH="main"
REQUIRED_CONTEXT="FFI AddressSanitizer (push)"

gh api "repos/${REPO}/branches/${BRANCH}/protection/required_status_checks" \
  > var/evidence/phase12/12-01-pre-read.json

gh api --method PATCH \
  "repos/${REPO}/branches/${BRANCH}/protection/required_status_checks" \
  -f strict=true \
  -f "contexts[]=${REQUIRED_CONTEXT}" \
  > var/evidence/phase12/12-01-write.json

gh api "repos/${REPO}/branches/${BRANCH}/protection/required_status_checks" \
  > var/evidence/phase12/12-01-post-read.json
```

#### Retirement Gate Rules

- `capable`: proceed with retirement evidence package, including pre-read/write/post-read logs and post-read context proof.
- `blocked_external`: do not claim retirement; preserve governance debt with owner, prerequisite, and the exact `403` evidence.
- `misconfigured`: correct target/required-context mapping, then rerun the full read/write/read sequence.
- `error`: resolve execution failure first; do not update governance debt status until command evidence is reproducible.

#### Latest Canonical Evidence Snapshot (2026-03-06)

- Live `gh api` reads against `repos/rogu3bear/adapter-os/branches/main/protection` and related subresources confirmed:
  - repository visibility is public
  - `required_status_checks` are readable and already match `docs/governance/target-manifest.json`
  - no rulesets are present
  - optional hardening delta remains: `enforce_admins=true`, `required_conversation_resolution=true`, `dismiss_stale_reviews=true`

Historical blocked receipts from 2026-02 remain useful as audit history, but they are superseded for the canonical target by the 2026-03-06 capable-state verification.

---

## Governance Drift Audit Guardrails (GOV-13)

Phase 16 adds a deterministic, read-only drift audit for required status checks.

### Canonical Target Manifest

- Manifest path: `docs/governance/target-manifest.json`
- Canonical policy source: `canonical_policy.required_contexts`
- Target scope: `targets[]` entries (repo, branch, probe context, approved exceptions)

### Commands

```bash
# Validate manifest contract
bash scripts/ci/validate_governance_target_manifest.sh \
  --manifest docs/governance/target-manifest.json

# Run read-only drift audit and emit reports/evidence
bash scripts/ci/audit_governance_drift.sh \
  --manifest docs/governance/target-manifest.json \
  --fail-on drifted
```

### Outcome Classes and Operator Response

| Outcome | Meaning | Operator response |
|---|---|---|
| `compliant` | Target required-check policy matches expected contexts and `strict=true` | No action; archive evidence. |
| `drifted` | Required contexts are missing or strict mode is not true | Open remediation issue, assign owner, and track until parity is restored. |
| `blocked_external` | Branch-protection API is externally blocked (for example `HTTP 403`) | Record blocker evidence and escalate plan/visibility prerequisite. |
| `approved_exception` | A non-compliant raw outcome is explicitly approved in manifest exception policy | Keep exception traceable with owner/reason and revisit on policy review cadence. |

Artifacts are written under `var/evidence/governance-drift-<UTCSTAMP>/`:
- `manifest-validation.txt`
- `audit.log`
- `report.json`
- `report.txt`

### Approved Parity Target Set (v1.1.3)

Current approved multi-repo parity targets in `docs/governance/target-manifest.json`:
- `rogu3bear/adapter-os` (`main`)
- `rogu3bear/jkca-agent` (`main`)
- `rogu3bear/jkca-web` (`main`)
- `rogu3bear/scopic-web` (`main`)

Latest parity evidence snapshot (2026-02-25):
- `var/evidence/governance-parity-20260225T213006Z/report.txt`
- `var/evidence/governance-parity-20260225T213006Z/parity-matrix.txt`
- `var/evidence/governance-parity-20260225T213006Z/approved-exceptions.txt`

Observed parity posture for the latest canonical verification:
- `rogu3bear/adapter-os` (`main`) is compliant on required-check parity.
- The remaining approved exceptions, if any, are non-canonical targets and should not be projected onto AdapterOS main.

---

## Capability Unlock and Canonical Enforcement (v1.1.5 / Phase 21 rerun)

Phase 18 adds deterministic capability polling and explicit blocked/capable branch receipts for the canonical target.

### Commands

```bash
# Poll capability deterministically and emit gate-state receipts
bash scripts/ci/run_governance_capability_loop.sh \
  --repo rogu3bear/adapter-os \
  --branch main \
  --required-context 'FFI AddressSanitizer (push)' \
  --output-dir var/evidence/governance-enforcement-<UTCSTAMP> \
  --attempts 4 \
  --sleep-seconds 2

# Execute capable-path enforcement flow with built-in rollback guard
bash scripts/ci/execute_governance_required_checks.sh \
  --repo rogu3bear/adapter-os \
  --branch main \
  --required-context 'FFI AddressSanitizer (push)' \
  --manifest docs/governance/target-manifest.json \
  --output-dir var/evidence/governance-enforcement-exec-<UTCSTAMP>
```

### Branch Semantics

| Gate state | Required behavior |
|---|---|
| `blocked_external` | No branch-protection PATCH/write attempts; emit `blocked-write-attempts.txt` with `write_attempts=0`. |
| `capable` | Proceed with canonical read/write/readback enforcement path and rollback-safe evidence capture. |

Latest canonical capability snapshot (2026-03-06):
- Live GitHub API verification confirms the canonical target is `capable`.
- Current parity status is compliant on required status checks.
- Optional hardening delta is limited to review/admin/conversation settings, not required-check parity.

Historical capability snapshots (2026-02-26, superseded for canonical target):
- `var/evidence/governance-capability-rerun-20260226T022425Z/gate-state.txt` -> `blocked_external`
- `var/evidence/governance-capability-rerun-20260226T022425Z/capability-loop.log` -> 4 deterministic probes, all `blocked_external` (`exit 20`)
- `var/evidence/governance-capability-rerun-20260226T022425Z/branch-decision.txt` -> `next_action=retain_blocker_branch`
- `var/evidence/governance-enforcement-rerun-20260226T022456Z/gate-state.txt` -> `blocked_external` from executable enforcement flow (`execute_governance_required_checks.sh`)
- `var/evidence/governance-enforcement-rerun-20260226T022456Z/blocked-write-attempts.txt` -> `write_attempts=0`, `policy_mutations=0`, `rollback_attempts=0`
- `var/evidence/governance-enforcement-rerun-20260226T022456Z/execution-branch.txt` -> `status=blocked_external`, `next_action=retain_blocker_debt`

Latest prod-cut rehearsal snapshot (2026-03-02, historical):
- `.planning/prod-cut/evidence/governance/20260302T080306Z/preflight.log` -> `status=blocked_external`, `exit_code=20`
- `.planning/prod-cut/evidence/governance/20260302T080306Z/capability-loop.log` -> deterministic retry loop remained blocked
- `.planning/prod-cut/evidence/governance/20260302T080306Z/enforcement.log` -> enforcement flow stayed on blocked branch (no capable transition)
- `.planning/prod-cut/evidence/release/local_release_gate_prod-20260302T081822Z.log` -> strict prod gate failed at governance preflight by policy

---

## Multi-Repo Enforcement Graduation (v1.1.5 / Phase 21 reconciliation)

Phase 19 graduates parity reporting into deterministic target-level outcome matrix and operator routing receipts.

### Commands

```bash
# Run graduation audit over approved targets
bash scripts/ci/audit_governance_drift.sh \
  --manifest docs/governance/target-manifest.json \
  --output-dir var/evidence/governance-graduation-<UTCSTAMP> \
  --fail-on drifted

# Render deterministic matrix + routing action receipts
bash scripts/ci/render_governance_graduation_receipts.sh \
  --report var/evidence/governance-graduation-<UTCSTAMP>/report.json \
  --output-dir var/evidence/governance-graduation-<UTCSTAMP>
```

### Outcome Routing

| Outcome | Action |
|---|---|
| `compliant` | `retain` |
| `drifted` | `remediate` |
| `blocked_external` | `escalate_blocker` |
| `approved_exception` | `review_exception` |

Latest graduation snapshot (2026-02-26):
- `var/evidence/governance-graduation-rerun-20260226T022522Z/report.txt`
- `var/evidence/governance-graduation-rerun-20260226T022522Z/graduation-matrix.txt`
- `var/evidence/governance-graduation-rerun-20260226T022522Z/routing-actions.txt`

Observed graduation posture for this run:
- 4/4 targets resolved to final `approved_exception` from raw `blocked_external` (`HTTP 403`) outcomes.
- Routing actions are deterministic and mapped to `review_exception` for all approved targets.

---

## Contents

- [POLICIES.md](../POLICIES.md) — Policy engine and packs
- [SECURITY.md](../SECURITY.md) — Security guide
- [target-manifest.json](target-manifest.json) — Canonical governance drift target inventory
- [REPO_GOVERNANCE.md](REPO_GOVERNANCE.md) — Repository governance baseline
- [OWNERSHIP.md](OWNERSHIP.md) — Governance ownership and escalation
- [GENERATED_ARTIFACT_POLICY.md](GENERATED_ARTIFACT_POLICY.md) — Generated artifact policy
- [SLOS.md](SLOS.md) — Governance SLOs
- [GOVERNANCE_INCIDENTS.md](GOVERNANCE_INCIDENTS.md) — Incident handling
