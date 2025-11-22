# Code Generation Pipeline: OpenAPI → TypeScript

## Overview

The `cargo xtask codegen` command automates the complete code generation workflow for AdapterOS:

1. **Build & OpenAPI Export**: Compiles the server crate and extracts OpenAPI specification via utoipa
2. **TypeScript Generation**: Converts OpenAPI spec to TypeScript types using `openapi-typescript`
3. **Type Validation**: Verifies consistency between Rust API and generated TypeScript

## Quick Start

```bash
# Standard code generation
make codegen

# With verbose output
make codegen-verbose

# Or directly via cargo xtask
cargo xtask codegen
```

## Pipeline Steps

### Step 1: Dependency Check

Verifies required tools are installed:

- **Rust toolchain** (required): Checked via `cargo --version`
- **Node.js 18+** (required for TS generation): Checked via `node --version`
- **pnpm** (required for TS generation): Checked via `pnpm --version`
- **openapi-typescript** (required for TS generation): Verified in `ui/package.json`

Install dependencies if missing:

```bash
# Install Node.js
# macOS with Homebrew
brew install node

# Install pnpm globally
npm install -g pnpm

# Install openapi-typescript in UI directory
cd ui && pnpm add -D openapi-typescript
```

### Step 2: Build & OpenAPI Export

Builds the server API crate and extracts OpenAPI specification:

```bash
cargo build --release --locked --offline -p adapteros-server-api
```

The OpenAPI spec is generated using **utoipa**, a compile-time code generation framework that automatically derives OpenAPI 3.0 specs from Rust types and handler signatures.

**Key files:**
- Input: Rust handler definitions in `crates/adapteros-server-api/src/handlers/*.rs`
- Output: `target/codegen/openapi.json`

**Expected spec structure:**
- OpenAPI version 3.0.0
- Server metadata (title, version, description)
- API paths and endpoints
- Schema definitions (request/response types)
- Authentication schemes

### Step 3: TypeScript Generation

Converts OpenAPI JSON schema to TypeScript types:

```bash
cd ui && pnpm exec openapi-typescript \
  ../target/codegen/openapi.json \
  --output src/api/types.generated.ts
```

The `openapi-typescript` CLI converts OpenAPI schemas directly to TypeScript interface definitions with:

- **Strict typing**: All fields properly typed from schema definitions
- **Union types**: Proper handling of `oneOf`, `anyOf`, `allOf` constructs
- **Optional fields**: Correct `?` placement for nullable/optional properties
- **Enum types**: Converted to TypeScript string literal unions
- **Nested types**: Full type hierarchy from API schemas

**Output file:**
- Location: `ui/src/api/types.generated.ts`
- Format: TypeScript ES6 module with `export type` declarations
- Size: Typically 5-15 KB depending on API size

**Optional formatting:**
If prettier is available, generated types are automatically formatted:

```bash
cd ui && pnpm exec prettier --write src/api/types.generated.ts
```

### Step 4: Type Validation

Validates generated artifacts for consistency:

- **OpenAPI spec integrity**: Required fields (openapi, info, paths, components)
- **Schema completeness**: Endpoint and schema definitions count
- **TypeScript export count**: Verifies types were generated
- **Field validation**: Checks for proper type definitions

## Configuration

### Environment Variables

- `VERBOSE=1`: Enable verbose output during generation

```bash
VERBOSE=1 cargo xtask codegen
```

### Output Directories

- **OpenAPI spec**: `target/codegen/openapi.json`
- **TypeScript types**: `ui/src/api/types.generated.ts`

## Workflow Integration

### Development Flow

1. Modify Rust API types or handlers in `crates/adapteros-server-api/src/`
2. Run code generation:
   ```bash
   make codegen
   ```
3. Updated TypeScript types are ready for UI development

### Pre-Commit

Include code generation in your pre-commit workflow:

```bash
cargo xtask codegen
git add target/codegen/openapi.json ui/src/api/types.generated.ts
git commit -m "chore: regenerate API types"
```

### CI/CD Integration

Add to GitHub Actions workflow:

```yaml
- name: Generate API types
  run: cargo xtask codegen

- name: Verify types committed
  run: |
    git diff --exit-code ui/src/api/types.generated.ts || \
    (echo "Generated types out of sync" && exit 1)
```

## Type Consistency

### Rust ↔ TypeScript Mapping

| Rust Type | OpenAPI | TypeScript |
|-----------|---------|-----------|
| `String` | `type: string` | `string` |
| `i64` | `type: integer, format: int64` | `number` |
| `bool` | `type: boolean` | `boolean` |
| `Option<T>` | `nullable: true` or missing | `T \| null` or `T?` |
| `Vec<T>` | `type: array, items: {...}` | `T[]` |
| `enum Color { Red, Blue }` | `enum: ["Red", "Blue"]` | `"Red" \| "Blue"` |
| `struct User { ... }` | Named schema | `interface User { ... }` |

### Validation Checks

The pipeline validates:

1. **Spec validity**: Valid JSON with correct OpenAPI 3.0 structure
2. **Endpoint coverage**: All routes from `crates/adapteros-server-api/src/routes.rs`
3. **Schema references**: All referenced types defined in components
4. **Type exports**: Generated TypeScript file contains type definitions

## Troubleshooting

### "Rust/Cargo not found"

Install Rust:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### "Node.js not found" or "Version < 18"

Install Node.js 18+ using your package manager:
```bash
# macOS
brew install node@18
brew link node@18

# Verify
node --version  # Should be v18.x or higher
```

### "pnpm not found"

Install pnpm:
```bash
npm install -g pnpm
```

Verify:
```bash
pnpm --version
```

### "openapi-typescript not in ui/package.json"

Add the dependency:
```bash
cd ui
pnpm add -D openapi-typescript
```

### "No API endpoints found in spec"

This indicates the server build succeeded but no OpenAPI spec was generated. Check:

1. Server API crate compiles: `cargo build -p adapteros-server-api`
2. Handlers use utoipa attributes: `#[utoipa::path(...)]`
3. Routes are properly registered in `crates/adapteros-server-api/src/routes.rs`

Example utoipa handler:

```rust
/// Get user by ID
#[utoipa::path(
    get,
    path = "/v1/users/{id}",
    responses(
        (status = 200, description = "User found", body = User),
        (status = 404, description = "User not found")
    )
)]
pub async fn get_user(Path(id): Path<String>) -> Json<User> {
    // ...
}
```

### "Build failed"

Check server crate dependencies:
```bash
cargo build --release -p adapteros-server-api
```

Review error messages for missing dependencies or compilation errors.

### Generated types incomplete

Verify OpenAPI spec was generated:
```bash
cat target/codegen/openapi.json | jq '.paths | keys | length'
```

Count should be > 0. If zero, check handler annotations.

## Performance

Typical execution times:

- Dependency check: <1 second
- Build & OpenAPI export: 30-60 seconds (first run), 5-15 seconds (incremental)
- TypeScript generation: 2-5 seconds
- Type validation: <1 second
- **Total**: 40-80 seconds (first run), 10-20 seconds (incremental)

## Best Practices

### Keep OpenAPI Spec in Sync

1. Run `make codegen` after any API changes
2. Review generated TypeScript for correctness
3. Commit generated files alongside source changes

### Document Breaking Changes

When modifying API contracts:

1. Update API docs in handler comments
2. Run code generation
3. Document breaking changes in commit message

Example:

```rust
/// Create user with new email validation
#[utoipa::path(
    post,
    path = "/v1/users",
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "User created", body = User),
        (status = 400, description = "Invalid email format")
    )
)]
```

### Type Export Naming

Generated TypeScript follows OpenAPI schema naming:

- Request types: `CreateUserRequest`, `UpdateUserRequest`
- Response types: `User`, `UserList`
- Error types: `ErrorResponse`
- Enums: `UserRole`, `UserStatus`

### UI Integration

Import generated types:

```typescript
import type { User, CreateUserRequest } from './api/types.generated';

const createUser = async (req: CreateUserRequest): Promise<User> => {
  const response = await fetch('/v1/users', {
    method: 'POST',
    body: JSON.stringify(req),
  });
  return response.json();
};
```

## Advanced Configuration

### Custom OpenAPI Transform

Create a custom transform script in `ui/scripts/openapi-transform.js`:

```javascript
module.exports = {
  plugins: [
    {
      name: 'custom-transform',
      plugin: (schema) => {
        // Custom transformations
        return schema;
      }
    }
  ]
};
```

Reference in codegen:

```bash
openapi-typescript \
  openapi.json \
  --transform ./scripts/openapi-transform.js
```

### Partial Generation

Generate only OpenAPI spec without TypeScript:

```bash
SKIP_TS=1 cargo xtask codegen
```

(Requires modification to `xtask/src/codegen.rs`)

## References

- **utoipa docs**: https://docs.rs/utoipa/latest/utoipa/
- **OpenAPI 3.0 spec**: https://spec.openapis.org/oas/v3.0.3
- **openapi-typescript**: https://openapi-ts.dev/
- **API Types**: `ui/src/api/types.ts` (manual definitions)
- **Server API**: `crates/adapteros-server-api/src/`

## Related Commands

```bash
# Just generate OpenAPI docs (legacy)
cargo xtask openapi-docs

# Validate OpenAPI spec
make validate-openapi

# Run codegen with verbose output
make codegen-verbose

# Type-check UI without building
cd ui && pnpm exec tsc --noEmit
```
