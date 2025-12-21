# API Type Generation Guide

This document describes the API type generation workflow for AdapterOS, which ensures type safety between the Rust backend and TypeScript frontend.

## Overview

AdapterOS uses a **code-first** approach to API documentation:

```
Rust Code (utoipa) → OpenAPI Spec → TypeScript Types
                                   ↘ Python SDK
```

The CI pipeline enforces that generated types stay in sync with the backend, preventing type drift.

## Quick Commands

```bash
# Generate TypeScript types
make gen-types

# Generate Python SDK
make gen-sdk-python

# Generate all SDKs
make gen-sdks

# Check for drift (CI use)
make check-types-drift
```

## Workflow for API Changes

### 1. Make API Changes in Rust

Edit handlers and models in `crates/adapteros-server-api/`:

```rust
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MyNewType {
    pub id: String,
    pub name: String,
}

#[utoipa::path(
    get,
    path = "/api/my-endpoint",
    responses(
        (status = 200, description = "Success", body = MyNewType)
    )
)]
pub async fn my_handler() -> Json<MyNewType> {
    // implementation
}
```

### 2. Regenerate Types

```bash
make gen-types
```

This runs:
1. `cargo run --bin export-openapi` - Generates `target/codegen/openapi.json`
2. `openapi-typescript` - Generates `ui/src/api/generated.ts`

### 3. Review Changes

```bash
git diff ui/src/api/generated.ts
```

Check that:
- New types are correctly generated
- Existing types are updated as expected
- No unintended changes occurred

### 4. Update Frontend Code

Update any TypeScript code that uses the changed types:

```typescript
import type { components } from '@/api/generated';

type MyNewType = components['schemas']['MyNewType'];

// Use the type
const data: MyNewType = {
  id: '123',
  name: 'Example'
};
```

### 5. Test

```bash
# Type check
cd ui && pnpm exec tsc --noEmit

# Run tests
pnpm test

# Run transformer tests specifically
pnpm test src/api/__tests__/transformers.test.ts
```

### 6. Commit Changes

```bash
git add crates/adapteros-server-api/
git add ui/src/api/generated.ts
git commit -m "feat(api): add new endpoint for X

- Add MyNewType schema
- Generate TypeScript types
- Update API client"
```

## CI Validation

The `api-types-drift` CI job:

1. **Generates OpenAPI spec** from Rust backend
2. **Generates TypeScript types** from spec
3. **Compares** with committed `generated.ts`
4. **Fails** if there's drift

### When CI Fails

If you see this error:

```
::error::Generated TypeScript types are out of sync with OpenAPI spec!
```

**Fix it:**

```bash
# Regenerate types
make gen-types

# Or from UI directory
cd ui && pnpm run gen:types

# Commit the changes
git add ui/src/api/generated.ts
git commit -m "chore: update generated API types"
git push
```

## Generated Files

### TypeScript Types (`ui/src/api/generated.ts`)

- **Auto-generated** from OpenAPI spec
- **Do not edit manually**
- Contains all API types as TypeScript interfaces/types
- Includes path operations, request/response schemas, and enums

Example usage:

```typescript
import type { paths, components } from '@/api/generated';

// Response type for a specific endpoint
type GetAdapterResponse = paths['/api/adapters/{id}']['get']['responses']['200']['content']['application/json'];

// Schema type
type Adapter = components['schemas']['Adapter'];
```

### OpenAPI Spec (`target/codegen/openapi.json`)

- **Auto-generated** from Rust code
- OpenAPI 3.0 specification
- Used to generate client SDKs
- **Not committed** to git (in `target/`)

### Python SDK (`sdk/python/`)

- Full Python client library
- Generated using `openapi-generator-cli`
- Includes typed models and API methods
- **Not currently in use** (future feature)

## Configuration

### Generation Options

TypeScript generation uses these `openapi-typescript` flags:

- `--export-type`: Export all as `export type`
- `--enum`: Generate native TS enums
- `--alphabetize`: Consistent ordering
- `--empty-objects-unknown`: Safe empty object handling
- `--path-params-as-types`: Type-safe path parameters
- `--default-non-nullable=false`: Nullable by default

### package.json Scripts

From `ui/package.json`:

```json
{
  "scripts": {
    "gen:openapi": "cd .. && cargo run --bin export-openapi -- target/codegen/openapi.json",
    "gen:api-types": "openapi-typescript ../target/codegen/openapi.json --output src/api/generated.ts ...",
    "gen:types": "pnpm run gen:openapi && pnpm run gen:api-types",
    "check:drift": "pnpm run gen:types && git diff --exit-code src/api/generated.ts"
  }
}
```

## Tools Required

### For TypeScript Generation

- **pnpm** - Package manager
- **openapi-typescript** - Type generator (installed via `pnpm install`)

### For Python Generation (Optional)

- **openapi-generator-cli** - SDK generator

```bash
npm install -g @openapitools/openapi-generator-cli
# or
brew install openapi-generator
```

## Advanced Usage

### Generate Spec Only

```bash
./scripts/generate-sdks.sh --spec-only
```

Generates OpenAPI spec without client SDKs.

### Validate Spec

```bash
./scripts/generate-sdks.sh --spec-only --validate
```

Generates and validates the OpenAPI spec structure.

### Check Drift in CI

```bash
./scripts/generate-sdks.sh --check-drift
```

Exits with code 1 if generated types differ from committed version.

### Generate Python SDK

```bash
./scripts/generate-sdks.sh --python
```

Requires `openapi-generator-cli` installed.

## Troubleshooting

### Types Don't Match Rust Changes

1. Check utoipa annotations on your types
2. Ensure `#[derive(ToSchema)]` is present
3. Verify `#[utoipa::path()]` is on handlers
4. Rebuild: `cargo build -p adapteros-server-api`
5. Regenerate: `make gen-types`

### CI Drift Check Failing

```bash
# Pull latest
git pull origin main

# Regenerate locally
make gen-types

# Check if there are changes
git status ui/src/api/generated.ts

# If changes exist, commit them
git add ui/src/api/generated.ts
git commit -m "chore: sync generated API types"
git push
```

### "pnpm not found"

```bash
npm install -g pnpm
```

### "openapi-typescript not found"

```bash
cd ui && pnpm install
```

## Best Practices

1. **Always regenerate after API changes** - Don't rely on CI to catch it
2. **Review generated diffs** - Ensure changes are intentional
3. **Commit together** - Include Rust changes + generated types in same PR
4. **Test locally** - Run `pnpm test` before pushing
5. **Keep annotations updated** - Maintain utoipa annotations on all API types
6. **Use type imports** - Import from `generated.ts` instead of duplicating types

## Examples

### Adding a New Endpoint

**1. Define in Rust:**

```rust
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateUserRequest {
    pub email: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[utoipa::path(
    post,
    path = "/api/users",
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "User created", body = User)
    )
)]
pub async fn create_user(
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<User>, ApiError> {
    // implementation
}
```

**2. Regenerate types:**

```bash
make gen-types
```

**3. Use in TypeScript:**

```typescript
import type { paths } from '@/api/generated';

type CreateUserRequest = paths['/api/users']['post']['requestBody']['content']['application/json'];
type CreateUserResponse = paths['/api/users']['post']['responses']['201']['content']['application/json'];

const createUser = async (data: CreateUserRequest): Promise<CreateUserResponse> => {
  const response = await fetch('/api/users', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  });
  return response.json();
};
```

### Updating an Existing Type

**1. Modify Rust struct:**

```rust
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    // New field
    pub avatar_url: Option<String>,
}
```

**2. Regenerate:**

```bash
make gen-types
```

**3. TypeScript types automatically updated:**

```typescript
// generated.ts now includes avatar_url as optional
interface User {
  id: string;
  email: string;
  name: string;
  created_at: string;
  avatar_url?: string; // ← Automatically added
}
```

## Related Documentation

- [Codegen README](../codegen/README.md) - Detailed SDK generation docs
- [AGENTS.md](../AGENTS.md) - Development guardrails
- [API Client](../ui/src/api/client.ts) - API client implementation
- [utoipa Documentation](https://docs.rs/utoipa/) - OpenAPI annotations

## Support

For issues with type generation:

- Check [Troubleshooting](#troubleshooting) section above
- Search GitHub Issues for `area:api` or `area:codegen`
- Consult AGENTS.md for coding agent guidance

---

**Note**: This workflow is critical for maintaining type safety across the AdapterOS stack. Never skip type regeneration when making API changes.
