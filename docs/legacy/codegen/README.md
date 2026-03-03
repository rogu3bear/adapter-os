# Code Generation (Legacy Root)

Status: legacy docs-only directory.

## Current Canonical Paths

- Generated OpenAPI/codegen scratch artifacts: `target/codegen/`
- Canonical committed OpenAPI spec: `docs/api/openapi.json`
- Export/check entrypoints:
  - `crates/adapteros-server-api/src/bin/export-openapi.rs`
  - `scripts/ci/check_openapi_drift.sh`

## Guidance

- Do not add new runtime/product paths under root `codegen/`.
- Keep this directory for historical reference only until fully retired.
