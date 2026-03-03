# ERRORS

Canonical error codes are defined in `/Users/star/Dev/adapter-os/crates/adapteros-core/src/error_codes.rs`.

## Response Contract

Public API errors must include:

```json
{
  "message": "Human-readable summary",
  "code": "CANONICAL_ERROR_CODE",
  "details": {
    "legacy_code": "OPTIONAL_OLD_CODE"
  }
}
```

- `code` is canonical.
- `details.legacy_code` is optional compatibility metadata during migration.
- `ErrorResponse` type: `/Users/star/Dev/adapter-os/crates/adapteros-api-types/src/lib.rs`.

## Canonicalization Rules

- Server handlers should emit via `ApiError` constructors in `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/api_error.rs`.
- Dynamic or legacy codes are normalized by `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/error_code_normalization.rs`.
- Middleware enforcement in `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/middleware/error_code_enforcement.rs` guarantees canonical `code` for JSON error responses.

## Drift Governance

Single command:

```bash
bash /Users/star/Dev/adapter-os/scripts/check_error_code_drift.sh
```

This script:

1. Regenerates inventory artifacts under `/Users/star/Dev/adapter-os/docs/error-inventory/`.
2. Validates that literal `with_code("...")` emissions are canonical or allowlisted.
3. Detects new `ErrorResponse::new(...)` emission sites without nearby `.with_code(...)`.
4. Fails CI on uncatalogued drift.

## Inventory Artifacts

- `/Users/star/Dev/adapter-os/docs/error-inventory/ERROR_CODE_INVENTORY.md`
- `/Users/star/Dev/adapter-os/docs/error-inventory/error_codes_inventory.json`
- `/Users/star/Dev/adapter-os/docs/error-inventory/error-code-disposition.csv`

## Adding a New Error Code

1. Add constant to `/Users/star/Dev/adapter-os/crates/adapteros-core/src/error_codes.rs`.
2. Use that constant from handlers (`ApiError` or canonical mapping).
3. If replacing an existing external code, add alias mapping in normalization and carry `legacy_code`.
4. Run drift check script and commit updated inventory artifacts.

## Alias/Deprecation Policy

- Existing externally visible non-canonical codes may remain as aliases temporarily.
- During migration window, return canonical `code` and preserve prior value in `details.legacy_code`.
- Deprecation/disposition decisions are tracked in `/Users/star/Dev/adapter-os/docs/error-inventory/error-code-disposition.csv`.
