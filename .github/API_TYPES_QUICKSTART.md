# API/OpenAPI Quick Reference

> **TL;DR**: After API route/schema changes, run `./scripts/ci/check_openapi_drift.sh --fix`.

## Quick Commands

```bash
# CI-equivalent drift check
./scripts/ci/check_openapi_drift.sh

# Regenerate committed OpenAPI spec when drift exists
./scripts/ci/check_openapi_drift.sh --fix

# Validate runtime route inventory coverage against OpenAPI
./scripts/ci/check_route_inventory_openapi_coverage.sh
```

## When to Regenerate OpenAPI

Run `./scripts/ci/check_openapi_drift.sh --fix` after:

- Adding or removing API routes
- Changing request/response schemas
- Updating `utoipa` annotations
- Renaming fields used in API DTOs

## CI Drift Failure

If CI reports OpenAPI drift, fix with:

```bash
./scripts/ci/check_openapi_drift.sh --fix
git add docs/api/openapi.json
git commit -m "chore: sync OpenAPI spec"
```

## File Locations

- **Committed canonical spec**: `docs/api/openapi.json`
- **Generated check artifact**: `target/codegen/openapi.check.json`
- **Exporter default output path**: `target/codegen/openapi.json`
- **Exporter binary source**: `crates/adapteros-server-api/src/bin/export-openapi.rs`

## Full Documentation

- [API Reference](../docs/API_REFERENCE.md)
- [APIS](../docs/APIS.md)
- [Route Inventory Coverage Exclusions](../docs/api/openapi_route_coverage_exclusions.txt)
