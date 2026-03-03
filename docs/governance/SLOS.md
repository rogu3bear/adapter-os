# Governance SLOs

## SLO Targets

- Unauthorized tracked generated artifacts on `main`: **0**
- Governance gate bypasses without approval record: **0**
- Mean time to governance-failure fix: **< 1 business day**
- Tracked binary budget compliance: **100%**

## Measurement

Measured via monthly governance report output:

- `var/reports/governance/hygiene-report.json`
- `var/reports/governance/hygiene-report.md`

## Error Budget and Response

- Any SLO breach creates a governance incident.
- `main` branch governance gate breakage is Sev2.
- Pre-merge drift detections are Sev3.
