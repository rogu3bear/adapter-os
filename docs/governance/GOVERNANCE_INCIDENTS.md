# Governance Incident Process

## Severity Model

- **Sev2**: Blocking governance gate is broken on `main`.
- **Sev3**: Drift detected pre-merge and blocked by governance checks.

## Required Incident Record

Include:

- Detection time (UTC)
- Triggering check/job
- Scope (repo, branch, paths)
- Root cause
- Remediation PR
- Prevention follow-up

## Response Expectations

### Sev2

- Immediate owner assignment
- Hotfix path prioritized over feature work
- Post-incident prevention item required

### Sev3

- Fix in the same PR or follow-up before merge
- Owner confirms policy alignment before close

## Close Criteria

Incident can close only when:

- Failing governance check is green
- Root cause documented
- Preventative action accepted or intentionally waived with owner sign-off
