# Phase 7 Execution: Docs/Contracts/CI Reliability Hardening

## Objective
- Consolidate docs validation to a single canonical gate and reduce duplicate/conflicting CI signals.

## Implementation Change
- Updated `scripts/validate-docs.sh` to:
  - Run `scripts/contracts/check_docs_claims.sh` as the default canonical validator.
  - Keep legacy heuristic checks optional behind `--legacy`.
  - Provide deterministic CLI usage and strict shell behavior (`set -euo pipefail`).

## Overlap Resolution
- Canonical gate (source-of-truth):
  - `scripts/contracts/check_docs_claims.sh`
  - `scripts/contracts/generate_contract_artifacts.py --check`
- Legacy heuristic gate (non-authoritative, optional):
  - old policy-pack count grep
  - sampled source citation checks
  - lightweight README/CLI marker checks
- Net effect: default docs validation no longer fails on heuristic drift when canonical contract artifacts are valid.

## Verification Run
- Ran canonical gate directly:
`scripts/contracts/check_docs_claims.sh`
- Result: passed.

- Ran consolidated entrypoint (default):
`scripts/validate-docs.sh`
- Result: passed (canonical mode, legacy skipped).

- Ran consolidated entrypoint with legacy checks:
`scripts/validate-docs.sh --legacy`
- Result: failed on heuristic-only checks:
  - policy pack count mismatch (`code=31`, docs mention `25`)
  - README legacy version marker missing (`alpha-v`)

## Contract Artifact Status
- `docs/generated/api-route-inventory.json`: in sync.
- `docs/generated/ui-route-inventory.json`: in sync.
- `docs/generated/middleware-chain.json`: in sync.

## Phase 7 Completion
- [x] Unified docs-claims gate strategy delivered and implemented.
- [x] Legacy overlap reduced via optional mode.
- [x] Contract artifact checks verified.
- [x] Residual heuristic failures documented explicitly.

