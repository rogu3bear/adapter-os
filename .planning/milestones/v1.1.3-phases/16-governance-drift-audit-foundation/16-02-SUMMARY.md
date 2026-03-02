# Phase 16-02 Summary: Read-Only Governance Drift Audit Runner and Reports

**Completed:** 2026-02-25
**Requirements:** GOV-13
**Outcome:** Deterministic read-only drift audit runner implemented with machine-readable and human-readable report artifacts.

## Scope

Execute Phase 16-02 by implementing a manifest-driven, read-only governance drift auditor that classifies targets and emits reproducible evidence under a UTC-stamped directory.

## Files Updated

- `scripts/ci/audit_governance_drift.sh`
- `var/evidence/governance-drift-20260225T212328Z/audit.log`
- `var/evidence/governance-drift-20260225T212328Z/report.json`
- `var/evidence/governance-drift-20260225T212328Z/report.txt`
- `var/evidence/governance-drift-20260225T212328Z/preflight-canonical-main.log`
- `var/evidence/governance-drift-20260225T212328Z/protection-canonical-main.err`

## Commands Executed (Exact)

1. Drift audit execution:
```bash
bash scripts/ci/audit_governance_drift.sh \
  --manifest docs/governance/target-manifest.json \
  --output-dir var/evidence/governance-drift-20260225T212328Z \
  --fail-on drifted
```

2. Report verification:
```bash
cat var/evidence/governance-drift-20260225T212328Z/report.txt
cat var/evidence/governance-drift-20260225T212328Z/report.json
```

## Results

### Audit behavior

- Runner executes only read operations:
  - `check_governance_preflight.sh`
  - `gh api .../required_status_checks` (GET only)
- No write/PATCH surfaces are used in this command path.
- Deterministic outcome classes emitted per target:
  - `compliant`
  - `drifted`
  - `blocked_external`
  - `approved_exception`

### Canonical run outcome

- Canonical target raw outcome: `blocked_external` (`HTTP 403`, preflight exit `20`).
- Final outcome: `approved_exception` (manifest-approved blocker class).
- Summary matrix in `var/evidence/governance-drift-20260225T212328Z/report.txt` confirms: 1 target, 0 drifted, 1 approved_exception.

### Evidence package

- Complete evidence bundle exists under `var/evidence/governance-drift-20260225T212328Z/` including logs, report JSON, and report TXT.

## Behavior Changed

- Added reusable, read-only governance drift audit surface for CI and operator workflows.

## Residual Risk

- Canonical branch-protection API capability remains externally blocked (`HTTP 403`); this phase intentionally records and classifies, not remediates.
