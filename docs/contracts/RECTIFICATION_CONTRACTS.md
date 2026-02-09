# Rectification Contracts

This document defines the automated contract gates introduced by the whole-repo rectification program.

Last verified: 2026-02-09

## Contract Artifacts

Generated from source:

- `docs/generated/api-route-inventory.json`
- `docs/generated/ui-route-inventory.json`
- `docs/generated/middleware-chain.json`

Generator:

- `scripts/contracts/generate_contract_artifacts.py`

## Contract Checks

- `scripts/contracts/check_contract_artifacts.sh`
  - Validates generated artifacts are present and up to date.

- `scripts/contracts/check_api_route_tiers.py`
  - Validates route tier invariants for worker/internal/public/protected surfaces.

- `scripts/contracts/check_ui_routes.py`
  - Validates public UI route set and key protected routes.

- `scripts/contracts/check_middleware_chain.py`
  - Validates protected and global middleware order contracts.

- `scripts/contracts/check_startup_contract.sh`
  - Validates `./start` and `service-manager` startup/help surface contract.

- `scripts/contracts/check_determinism_contract.sh`
  - Validates deterministic constants and tmp path policy invariants.
  - On pull requests, requires label `determinism-contract-change` when determinism-critical files change.

- `scripts/contracts/check_docs_claims.sh`
  - Validates canonical docs and contract references are present.

Aggregate runner:

- `scripts/contracts/check_all.sh`

## CI Integration

- `.github/workflows/ci.yml` job `rectification-contracts` runs `scripts/contracts/check_all.sh`.

## Contributor Workflow

When editing route definitions, middleware order, UI routes, or startup contract:

1. Make source change.
2. Run:

   ```bash
   scripts/contracts/generate_contract_artifacts.py
   scripts/contracts/check_all.sh
   ```

3. Commit source + regenerated artifacts together.

