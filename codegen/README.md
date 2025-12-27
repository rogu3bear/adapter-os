# AdapterOS SDK Code Generation

This directory contains configuration files for generating client SDKs from the OpenAPI specification.

## Overview

AdapterOS uses a code-first approach to API documentation and client generation:

1. **Rust Backend** → OpenAPI spec is generated from utoipa annotations in the Rust code
2. **OpenAPI Spec** → TypeScript types and Python SDK are generated from the spec
3. **CI Validation** → Ensures generated types stay in sync with the backend

## Quick Start

### Generate TypeScript Types

```bash
# From project root
make gen-types

# Or using the script directly
./scripts/generate-sdks.sh --typescript

# Or from the UI directory
cd ui && pnpm run gen:types
```

### Generate Python SDK

```bash
make gen-sdk-python

# Or using the script
./scripts/generate-sdks.sh --python
```

### Generate All SDKs

```bash
make gen-sdks

# Or
./scripts/generate-sdks.sh --all
```

### Check for Drift

```bash
make check-types-drift

# Or
./scripts/generate-sdks.sh --check-drift
```

## How It Works

### 1. OpenAPI Spec Generation

The OpenAPI specification is generated from Rust code using utoipa annotations:

```bash
cargo run -p adapteros-server-api --bin export-openapi -- target/codegen/openapi.json
```

This binary reads the API routes and schemas defined in `adapteros-server-api` and generates a complete OpenAPI 3.0 specification.

### 2. TypeScript Type Generation

TypeScript types are generated using `openapi-typescript`:

- **Input**: `target/codegen/openapi.json`
- **Output**: `ui/src/api/generated.ts`
- **Tool**: [openapi-typescript](https://github.com/drwpow/openapi-typescript)

The generated types are type-safe, tree-shakeable, and optimized for TypeScript projects.

**Configuration** (via CLI flags in `scripts/generate-sdks.sh`):
- `--export-type`: Export all types as `export type` (not just interfaces)
- `--enum`: Generate native TypeScript enums
- `--alphabetize`: Sort types alphabetically for consistent diffs
- `--empty-objects-unknown`: Treat empty objects as `Record<string, unknown>`
- `--default-non-nullable=false`: Make all fields nullable by default (safe default)
- `--path-params-as-types` is currently disabled to avoid template path index collisions with literal routes (e.g. `/healthz/all`)

### 3. Python SDK Generation

The Python SDK is generated using `openapi-generator-cli`:

- **Input**: `target/codegen/openapi.json`
- **Output**: `sdk/python/`
- **Tool**: [OpenAPI Generator](https://github.com/OpenAPITools/openapi-generator)
- **Configuration**: `codegen/python.json`

The generated SDK includes:
- Fully typed Python client
- Request/response models using Pydantic
- Async support
- Comprehensive documentation

## CI Integration

The CI pipeline includes an `api-types-drift` job that:

1. Generates the OpenAPI spec from Rust backend
2. Generates TypeScript types from the spec
3. Compares generated types with the committed version
4. **Fails if there's drift** (types are out of sync)

This ensures that:
- API changes in Rust are reflected in client types
- Developers update generated types before merging
- Type safety is maintained across the stack

## Workflow for Developers

### When Making API Changes

1. Make changes to Rust API handlers/models in `adapteros-server-api`
2. Update utoipa annotations if needed
3. Generate updated types:
   ```bash
   make gen-types
   ```
4. Review the changes in `ui/src/api/generated.ts`
5. Update any client code that uses the changed types
6. Run tests:
   ```bash
   cd ui && pnpm test
   ```
7. Commit both Rust changes and updated types

### When CI Fails on Drift

If CI fails with "Generated TypeScript types are out of sync":

```bash
# Regenerate types locally
cd ui && pnpm run gen:types

# Or from project root
make gen-types

# Review changes
git diff ui/src/api/generated.ts

# Commit the updated types
git add ui/src/api/generated.ts
git commit -m "chore: update generated API types"
```

## Configuration Files

### `typescript-fetch.json`

Configuration for TypeScript type generation (currently not used - we use CLI flags directly):

```json
{
  "generatorName": "typescript-fetch",
  "outputDir": "../ui/src/api/generated",
  "inputSpec": "../docs/api/openapi.json",
  "additionalProperties": {
    "supportsES6": true,
    "withInterfaces": true,
    "typescriptThreePlus": true,
    "modelPropertyNaming": "camelCase",
    "enumPropertyNaming": "PascalCase"
  }
}
```

Note: We currently use `openapi-typescript` with CLI flags instead of `openapi-generator-cli` for TypeScript because it's faster and produces more idiomatic types.

### `python.json`

Configuration for Python SDK generation:

```json
{
  "generatorName": "python",
  "outputDir": "../sdk/python",
  "inputSpec": "../docs/api/openapi.json",
  "additionalProperties": {
    "packageName": "adapteros_client",
    "packageVersion": "0.1.0",
    "projectName": "adapteros-client",
    "pythonAttrNoneIfUnset": true
  }
}
```

## Tools Required

### For TypeScript Generation

- **pnpm** (installed globally or in ui/node_modules)
- **openapi-typescript** (installed in ui/node_modules)

```bash
cd ui && pnpm install
```

### For Python Generation

- **openapi-generator-cli** (installed globally)

```bash
npm install -g @openapitools/openapi-generator-cli
```

Or using Homebrew:

```bash
brew install openapi-generator
```

## Troubleshooting

### "pnpm not found"

Install pnpm globally:

```bash
npm install -g pnpm
```

### "openapi-typescript not found"

Install UI dependencies:

```bash
cd ui && pnpm install
```

### "openapi-generator-cli not found"

For Python SDK generation, install the generator:

```bash
npm install -g @openapitools/openapi-generator-cli
# or
brew install openapi-generator
```

### Types are out of sync

Regenerate types:

```bash
make gen-types
```

Then commit the changes:

```bash
git add ui/src/api/generated.ts
git commit -m "chore: update generated API types"
```

### CI failing on drift check

The CI job compares generated types with committed versions. To fix:

1. Pull latest changes: `git pull`
2. Regenerate types: `make gen-types`
3. Commit if there are changes: `git add ui/src/api/generated.ts && git commit -m "chore: sync API types"`
4. Push: `git push`

## Best Practices

1. **Always regenerate types after API changes** - Use `make gen-types` after modifying API handlers
2. **Review generated changes** - Check `git diff ui/src/api/generated.ts` before committing
3. **Keep types in sync** - Don't manually edit `generated.ts`
4. **Test after regeneration** - Run `cd ui && pnpm test` to catch breaking changes
5. **Commit types with API changes** - Include both in the same PR

## Scripts

### `generate-sdks.sh`

Main script for SDK generation. Supports:

- `--spec-only`: Only generate OpenAPI spec
- `--typescript`: Generate TypeScript types only
- `--python`: Generate Python SDK only
- `--all`: Generate everything (default)
- `--validate`: Validate OpenAPI spec
- `--check-drift`: Check for drift (exits 1 if drift detected)
- `-h, --help`: Show help

Examples:

```bash
# Generate everything
./scripts/generate-sdks.sh

# Only TypeScript
./scripts/generate-sdks.sh --typescript

# Check for drift (CI-friendly)
./scripts/generate-sdks.sh --check-drift
```

## Future Enhancements

- [ ] Generate Go SDK
- [ ] Generate Rust SDK (for external consumers)
- [ ] Generate API documentation website
- [ ] Auto-commit types in CI (optional)
- [ ] Version SDK releases automatically
- [ ] Generate mock servers for testing

## Related Documentation

- [OpenAPI Specification](../docs/api/README.md) (if exists)
- [UI API Client](../ui/src/api/README.md) (if exists)
- [Contributing Guide](../CONTRIBUTING.md)

## Maintainers

For questions or issues with SDK generation, see:
- GitHub Issues: Tag with `area:api` or `area:codegen`
- AGENTS.md: API type generation section

---

**Note**: This is part of the AdapterOS deterministic inference platform. Generated types are critical for maintaining type safety across the Rust backend and TypeScript frontend.
