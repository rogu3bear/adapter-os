# Generated Artifact Policy

This policy defines which generated artifacts are committed versus runtime-only.

## Committed (Contract Artifacts)

Only contract-critical generated artifacts are committed:

| Artifact | Source | Deterministic Command | Owner | Validation |
|---|---|---|---|---|
| `docs/api/openapi.json` | OpenAPI export | `bash scripts/ci/check_openapi_drift.sh --fix` | `@adapteros-contracts` | `bash scripts/ci/check_openapi_drift.sh` |
| `docs/generated/api-route-inventory.json` | Route inventory generator | `python3 scripts/contracts/generate_contract_artifacts.py` | `@adapteros-contracts` | `bash scripts/contracts/check_docs_claims.sh` |
| `docs/generated/ui-route-inventory.json` | Route inventory generator | `python3 scripts/contracts/generate_contract_artifacts.py --ui-only` | `@adapteros-contracts` | `bash scripts/contracts/check_docs_claims.sh` |
| `docs/generated/middleware-chain.json` | Route inventory generator | `python3 scripts/contracts/generate_contract_artifacts.py` | `@adapteros-contracts` | `bash scripts/contracts/check_docs_claims.sh` |
| `docs/generated/api-surface-matrix.json` | API surface contract check | `python3 scripts/contracts/check_api_surface.py --fix` | `@adapteros-contracts` | `python3 scripts/contracts/check_api_surface.py` |

## Runtime-Only (Never Committed)

Examples:

- Tool logs and captures (`.playwright-cli/`, `.playwright-mcp/`)
- Runtime reports (`var/reports/`)
- Playwright run outputs (`var/playwright/runs/`)
- Build outputs (`target/`, `target-*`)
- Temporary bundles/captures under `var/`

## Policy Controls

- Allowed committed generated artifacts are declared in `docs/governance/generated-artifact-allowlist.json`.
- Tooling config allowlist is declared in `docs/governance/tooling-config-allowlist.json`.
- CI blocks unauthorized tracked generated artifacts.

## Review Expectations

Any PR touching committed generated artifacts must include:

- Generator command used
- Associated source change rationale
- Confirmation of deterministic validation check pass
