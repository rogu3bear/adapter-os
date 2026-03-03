# Governance Ownership

## Owners

- Contract owner (API/schema/contracts): `@adapteros-contracts`
- Tooling owner (developer workflow/tooling dirs): `@adapteros-devex`
- CI governance owner (blocking checks/workflows): `@adapteros-ci`

## Responsibilities

### Contract owner

- Owns committed generated contract artifacts.
- Approves allowlist changes for generated artifacts.
- Validates deterministic regeneration commands remain accurate.

### Tooling owner

- Owns tooling-state policy and cleanup scripts.
- Keeps local-only artifacts out of tracked files.
- Maintains docs for safe local cleanup and worktree hygiene.

### CI governance owner

- Owns governance workflow definitions and required checks.
- Owns check reliability and false-positive tuning.
- Maintains policy gate severity and fail/open posture.

## Escalation

- Governance gate failure on pull request: assign owning team and resolve before merge.
- Governance gate failure on `main`: treat as Sev2 and hotfix immediately.
- Repeated policy regressions: open governance incident and schedule policy review.

## On-Call Routing

- Primary: CI governance owner
- Secondary: Contract owner
- Tertiary: Tooling owner

When ownership is unclear, route to CI governance owner first.
