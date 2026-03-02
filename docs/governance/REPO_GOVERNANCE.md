# Repository Governance

This document defines repository governance rules that block drift and preserve deterministic, auditable artifacts.

## Canonical Runtime and Build Roots

- Runtime data root: `var/`
- Build cache root: `target/`
- Golden baseline root: `golden_runs/`

Rules:
- Runtime/test artifacts must not be written under crate-local `var/` or root `tmp/` trees.
- Product and CI scripts should use `target/` unless a script explicitly documents a temporary alternate target directory.
- Golden baselines must use `golden_runs/` (never `var/golden_runs`).

## Committed Contract-Generated Artifacts

The following generated artifacts are intentionally committed and contract-governed:

- `docs/api/openapi.json`
- `docs/generated/api-route-inventory.json`
- `docs/generated/ui-route-inventory.json`
- `docs/generated/middleware-chain.json`
- `docs/generated/api-surface-matrix.json`

Any additional generated artifact must be approved and added to the allowlist at:

- `docs/governance/generated-artifact-allowlist.json`

## Local Tooling State (Not Committed)

The following directories are local tooling state and must remain untracked:

- `.playwright-cli/`
- `.playwright-mcp/`
- `.codex/`
- `.claude/`
- `.agents/`
- `.harmony/`
- `.integrator/`
- `.worker_logs/`

Tracked tooling files are limited to static configuration in policy allowlists.

## Governance Enforcement

Blocking checks are executed by `.github/workflows/governance.yml`:

- Layout contract
- Generated artifact policy
- Tooling state policy
- Repository size/binary budget policy

See:

- `scripts/ci/check_tracked_generated_policy.sh`
- `scripts/ci/check_tooling_state_policy.sh`
- `scripts/ci/check_repo_size_budget.sh`
- `scripts/contracts/check_repo_layout_contract.sh`

## Rollout and Operations

- Governance checks are blocking for pull requests into `main`.
- Local cleanup path: `scripts/cleanup/local_tooling_artifacts.sh`
- Monthly report path: `scripts/governance/generate_hygiene_report.sh`
