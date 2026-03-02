# Phase 17-02 Summary: Run Parity Verification and Exception Handling

**Completed:** 2026-02-25
**Requirements:** OPS-09
**Outcome:** Multi-repo parity verification executed with explicit blocker/exception evidence and deterministic report artifacts.

## Scope

Execute Phase 17-02 by running parity verification on approved targets with fail-on drifted policy and explicit approved-exception handling.

## Files Updated

- `var/evidence/governance-parity-20260225T213006Z/audit.log`
- `var/evidence/governance-parity-20260225T213006Z/report.json`
- `var/evidence/governance-parity-20260225T213006Z/report.txt`
- `var/evidence/governance-parity-20260225T213006Z/parity-matrix.txt`
- `var/evidence/governance-parity-20260225T213006Z/approved-exceptions.txt`

## Commands Executed (Exact)

1. Parity audit run:
```bash
bash scripts/ci/audit_governance_drift.sh \
  --manifest docs/governance/target-manifest.json \
  --output-dir var/evidence/governance-parity-20260225T213006Z \
  --fail-on drifted
```

2. Matrix and exception receipts:
```bash
jq -r '.results | sort_by(.id)[] | [
  ("id=" + .id),
  ("outcome=" + .outcome),
  ("raw_outcome=" + .raw_outcome),
  ("endpoint_status=" + .endpoint_status),
  ("strict=" + .strict),
  ("missing=" + ((.missing_contexts | length) | tostring))
] | join(" ")' var/evidence/governance-parity-20260225T213006Z/report.json > var/evidence/governance-parity-20260225T213006Z/parity-matrix.txt

jq -r '.results[] | select(.outcome == "approved_exception") | "id=" + .id + " raw_outcome=" + .raw_outcome + " reason=" + (.approved_exception_reason // "")' \
  var/evidence/governance-parity-20260225T213006Z/report.json > var/evidence/governance-parity-20260225T213006Z/approved-exceptions.txt
```

## Results

- Report summary: `targets=4`, `compliant=0`, `drifted=0`, `approved_exception=4`.
- Each target resolved to approved_exception with raw `blocked_external` (`HTTP 403`) evidence.
- No unapproved drift outcomes remained under fail-on drifted policy.

## Behavior Changed

- Multi-repo parity proof path now has deterministic report + receipt outputs suitable for audit handoff.

## Residual Risk

- All current parity targets are blocked by external capability constraints; parity is evidenced through explicit approved exceptions, not strict-policy compliance proof.
