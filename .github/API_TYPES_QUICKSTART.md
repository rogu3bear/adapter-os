# API Types Quick Reference

> **TL;DR**: Run `make gen-types` after changing API handlers/models in Rust.

## Quick Commands

```bash
# After making API changes in Rust
make gen-types

# Check if types are in sync (like CI does)
make check-types-drift

# Generate all SDKs (TS + Python)
make gen-sdks
```

## When to Regenerate Types

Run `make gen-types` after:

- ✅ Adding new API endpoints
- ✅ Modifying request/response types
- ✅ Changing field names or types
- ✅ Adding/removing fields from schemas
- ✅ Updating utoipa annotations

## CI Drift Check

The CI will fail with this error if types are out of sync:

```
::error::Generated TypeScript types are out of sync with OpenAPI spec!
```

**Fix it:**

```bash
make gen-types
git add ui/src/api/generated.ts
git commit -m "chore: update generated API types"
```

## File Locations

- **Generated Types**: `ui/src/api/generated.ts` (auto-generated, don't edit)
- **OpenAPI Spec**: `target/codegen/openapi.json` (auto-generated)
- **Script**: `scripts/generate-sdks.sh`
- **Config**: `codegen/`

## Example Workflow

1. **Edit Rust API handler:**
   ```rust
   #[derive(ToSchema, Serialize)]
   pub struct NewType {
       pub id: String,
       pub name: String,
   }
   ```

2. **Regenerate types:**
   ```bash
   make gen-types
   ```

3. **Use in TypeScript:**
   ```typescript
   import type { components } from '@/api/generated';

   type NewType = components['schemas']['NewType'];
   ```

4. **Commit both:**
   ```bash
   git add crates/adapteros-server-api/
   git add ui/src/api/generated.ts
   git commit -m "feat: add NewType endpoint"
   ```

## Full Documentation

- [API Type Generation Guide](../docs/API_TYPE_GENERATION.md)
- [Codegen README](../codegen/README.md)
- [Summary](../API_TYPE_GENERATION_SUMMARY.md)

## Help

```bash
./scripts/generate-sdks.sh --help
```
