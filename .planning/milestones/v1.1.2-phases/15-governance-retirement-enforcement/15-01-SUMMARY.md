# Phase 15-01 Summary: Capability Gate and Immutable Baseline Evidence

**Completed:** 2026-02-25
**Requirements:** GOV-09, GOV-11, AUTO-01
**Outcome:** Immutable governance evidence captured; write path remained externally gated (`blocked_external`, exit `20`).

## Scope

Execute the Phase 15-01 gate-first path: create a new timestamped evidence workspace, run canonical governance preflight before any policy write, and branch deterministically into capable vs evidence-only handling.

## Files Updated

- `var/evidence/governance-retirement-20260225T201849Z/checklist-contexts.txt`
- `var/evidence/governance-retirement-20260225T201849Z/target.txt`
- `var/evidence/governance-retirement-20260225T201849Z/timestamp.txt`
- `var/evidence/governance-retirement-20260225T201849Z/preflight-before.log`
- `var/evidence/governance-retirement-20260225T201849Z/preflight-before.exit`
- `var/evidence/governance-retirement-20260225T201849Z/gate-state.txt`
- `var/evidence/governance-retirement-20260225T201849Z/evidence-only.txt`
- `var/evidence/governance-retirement-20260225T201849Z/execution-path.txt`

## Commands Executed (Exact)

1. Preflight hard gate:
```bash
bash scripts/ci/check_governance_preflight.sh \
  --repo rogu3bear/adapter-os \
  --branch main \
  --required-context 'FFI AddressSanitizer (push)'
```

2. Evidence-path assertions:
```bash
wc -l var/evidence/governance-retirement-20260225T201849Z/checklist-contexts.txt
cat var/evidence/governance-retirement-20260225T201849Z/gate-state.txt
cat var/evidence/governance-retirement-20260225T201849Z/evidence-only.txt
```

3. Blocked-path guard verification (no write attempts):
```bash
test -f var/evidence/governance-retirement-20260225T201849Z/write.json
test -f var/evidence/governance-retirement-20260225T201849Z/post-read.json
test -f var/evidence/governance-retirement-20260225T201849Z/rollback.json
```

## Results

### Deterministic evidence workspace

- Created fresh immutable evidence directory: `var/evidence/governance-retirement-20260225T201849Z`.
- Canonical checklist input contains exactly eight unique required contexts.
- Captured target metadata (`repo`, `branch`, `required_context`, `phase`, `plan`, UTC capture timestamp).

### Capability gate outcome

- `preflight-before.log` emitted `status=blocked_external` with `reason=http_403`.
- Preflight process exit code captured as `20` (`blocked_external`).
- `gate-state.txt` recorded as `blocked_external`.

### Baseline/readiness branch behavior

- As required by plan guardrails, no branch-protection write/readback artifacts were generated on blocked path.
- Explicit evidence-only marker recorded in `var/evidence/governance-retirement-20260225T201849Z/evidence-only.txt`.

## Behavior Changed

- None (evidence capture + planning artifact updates only).

## Residual Risk

- External capability gate remains unresolved (`HTTP 403`), so required-check enforcement (`15-02`) and reconciliation closure (`15-03`) remain blocked until governance API capability becomes `capable`.

## Next-Action Guidance

- Re-run preflight gate on canonical target when account/repo capability changes.
- Once `status=capable`, execute `/gsd:execute-phase 15` to proceed with `15-02` write/readback enforcement path.

## Requirement Status Impact

- `AUTO-01` remains active (execution followed taste profile + gate-first flow).
- `GOV-09` and `GOV-11` are **not yet complete** because successful capable-path enforcement evidence is still blocked externally.
