# Code Generation Examples and Patterns

## Usage Examples

### Basic Usage

```bash
# Run full code generation pipeline
make codegen

# Equivalent to:
cargo xtask codegen
```

### Verbose Output

```bash
# See detailed step-by-step output
make codegen-verbose

# Or set environment variable
VERBOSE=1 cargo xtask codegen
```

### Example Output

```
========================================
  AdapterOS Code Generation Pipeline
========================================

Step 1/4: Checking dependencies...
  Checking Rust toolchain...
  Checking Node.js...
    Found: v20.10.0
  Checking pnpm...
  Checking openapi-typescript in UI project...

Step 2/4: Building server and extracting OpenAPI spec...
  Build output:
    Compiling adapteros-server-api v0.1.0
    Finished release [optimized] target(s) in 45.32s
  Invoking OpenAPI generation script...

Step 3/4: Generating TypeScript types...
  Converting OpenAPI spec to TypeScript...
    Input: /Users/star/Dev/aos/target/codegen/openapi.json
    Output: /Users/star/Dev/aos/ui/src/api/types.generated.ts

Step 4/4: Validating type consistency...
  Checking OpenAPI spec integrity...
  Found 42 API endpoints
  Validating generated TypeScript types...
  Found 67 TypeScript type definitions
  Validating request/response schemas...
  Found 156 schema definitions

========================================
  Code Generation Report
========================================

✓ Dependency Check
  All dependencies satisfied
✓ Build & OpenAPI Export (45231 ms)
  OpenAPI spec written to /Users/star/Dev/aos/target/codegen/openapi.json
✓ TypeScript Generation (3451 ms)
  TypeScript types written to /Users/star/Dev/aos/ui/src/api/types.generated.ts
✓ Type Validation
  All types consistent

Total time: 49123 ms

✓ Code generation completed successfully
```

## Integration Workflow Examples

### Developing a New API Endpoint

1. **Create Rust endpoint** with utoipa documentation:

```rust
// In crates/adapteros-server-api/src/handlers/users.rs

use utoipa::path;
use axum::extract::Path;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub display_name: String,
}

#[derive(Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub created_at: String,
}

/// Create a new user
#[utoipa::path(
    post,
    path = "/v1/users",
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "User created", body = User),
        (status = 400, description = "Invalid request"),
        (status = 409, description = "Email already exists")
    ),
    tag = "Users"
)]
pub async fn create_user(
    Body(req): Body<CreateUserRequest>,
) -> (StatusCode, Json<User>) {
    // Implementation
}
```

2. **Regenerate types**:

```bash
make codegen
```

3. **Use in TypeScript**:

```typescript
import type { CreateUserRequest, User } from './api/types.generated';

const createUser = async (req: CreateUserRequest): Promise<User> => {
  const response = await fetch('/v1/users', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  });

  if (!response.ok) {
    throw new Error(`Failed to create user: ${response.statusText}`);
  }

  return response.json();
};

// Usage with full type safety
const newUser = await createUser({
  email: 'user@example.com',
  display_name: 'John Doe',
});

console.log(newUser.id, newUser.created_at);
```

### Updating API Response Types

Before:
```rust
#[derive(Serialize)]
pub struct User {
    pub id: String,
    pub email: String,
}
```

After (adding new field):
```rust
#[derive(Serialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub phone_number: Option<String>,  // New field
}

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

Steps:
1. Update Rust code
2. Run `make codegen`
3. TypeScript automatically includes `phone_number?: string`
4. UI code gets auto-completion for new field

### Using Enums with Type Safety

Rust:
```rust
#[derive(Serialize, Deserialize)]
pub enum UserRole {
    #[serde(rename = "admin")]
    Admin,
    #[serde(rename = "operator")]
    Operator,
    #[serde(rename = "viewer")]
    Viewer,
}

#[derive(Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub role: UserRole,
}
```

Generated TypeScript:
```typescript
export type UserRole = "admin" | "operator" | "viewer";

export interface User {
  id: string;
  role: UserRole;
}
```

TypeScript usage with full validation:
```typescript
const user: User = {
  id: "user-123",
  role: "admin", // Type-checked, only valid values allowed
};

// Catch typos at compile time
const invalidRole: UserRole = "moderator"; // TS Error!
```

### Complex Nested Types

Rust:
```rust
#[derive(Serialize, Deserialize)]
pub struct CreateAdapterRequest {
    pub name: String,
    pub metadata: AdapterMetadata,
    pub policies: Vec<PolicyRule>,
}

#[derive(Serialize, Deserialize)]
pub struct AdapterMetadata {
    pub description: String,
    pub tags: Vec<String>,
    pub version: String,
}

#[derive(Serialize, Deserialize)]
pub struct PolicyRule {
    pub name: String,
    pub conditions: Vec<Condition>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Condition {
    #[serde(rename = "role_check")]
    RoleCheck { required_role: String },
    #[serde(rename = "time_window")]
    TimeWindow { start: String, end: String },
}
```

Generated TypeScript:
```typescript
export interface CreateAdapterRequest {
  name: string;
  metadata: AdapterMetadata;
  policies: PolicyRule[];
}

export interface AdapterMetadata {
  description: string;
  tags: string[];
  version: string;
}

export interface PolicyRule {
  name: string;
  conditions: Condition[];
}

export type Condition =
  | { type: "role_check"; required_role: string }
  | { type: "time_window"; start: string; end: string };
```

TypeScript usage with discriminated unions:
```typescript
const createAdapter = async (
  req: CreateAdapterRequest
): Promise<Adapter> => {
  const validated = req.policies.map((p) => ({
    name: p.name,
    conditions: p.conditions.map((c) => {
      switch (c.type) {
        case "role_check":
          return { type: "role_check", required_role: c.required_role };
        case "time_window":
          return { type: "time_window", start: c.start, end: c.end };
        default:
          // TS ensures all cases covered
          return exhaustive(c);
      }
    }),
  }));

  // ... send to API
};
```

## CI/CD Integration Examples

### GitHub Actions Workflow

```yaml
name: Generate API Types

on:
  pull_request:
    paths:
      - 'crates/adapteros-server-api/src/**'
      - 'ui/package.json'
      - 'xtask/src/codegen.rs'

jobs:
  codegen:
    runs-on: macos-13
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo registry
        uses: actions/cache@v3
        with:
          path: ~/.cargo/registry
          key: ${{ runner.os }}-cargo-registry-${{ hashFiles('**/Cargo.lock') }}

      - name: Cache cargo index
        uses: actions/cache@v3
        with:
          path: ~/.cargo/git
          key: ${{ runner.os }}-cargo-index-${{ hashFiles('**/Cargo.lock') }}

      - name: Cache cargo build
        uses: actions/cache@v3
        with:
          path: target
          key: ${{ runner.os }}-cargo-build-target-${{ hashFiles('**/Cargo.lock') }}

      - name: Setup Node.js
        uses: actions/setup-node@v3
        with:
          node-version: '20'

      - name: Setup pnpm
        uses: pnpm/action-setup@v2
        with:
          version: 8

      - name: Generate API types
        run: cargo xtask codegen

      - name: Check for uncommitted changes
        run: |
          if ! git diff --quiet ui/src/api/types.generated.ts; then
            echo "Generated types are out of sync!"
            echo "Please run 'make codegen' and commit the changes"
            exit 1
          fi

      - name: Type-check UI
        run: cd ui && pnpm exec tsc --noEmit
```

### Pre-commit Hook

```bash
#!/bin/bash
# .git/hooks/pre-commit

set -e

echo "Checking if API types need regeneration..."

# Generate types
cargo xtask codegen > /dev/null 2>&1

# Check if anything changed
if git diff --quiet ui/src/api/types.generated.ts target/codegen/openapi.json; then
  exit 0
else
  echo "API types out of sync. Generated new types."
  echo "Please stage and re-commit:"
  echo "  git add ui/src/api/types.generated.ts target/codegen/openapi.json"
  exit 1
fi
```

Install:
```bash
chmod +x .git/hooks/pre-commit
```

## API Evolution Examples

### Versioning Pattern

Rust:
```rust
// Keep old endpoint for backwards compatibility
#[utoipa::path(
    get,
    path = "/v1/users/{id}",
    deprecated,  // Utoipa marks as deprecated in spec
    responses((status = 200, body = UserV1))
)]
pub async fn get_user_v1(Path(id): Path<String>) -> Json<UserV1> {}

// New endpoint with enhanced functionality
#[utoipa::path(
    get,
    path = "/v2/users/{id}",
    responses((status = 200, body = User))
)]
pub async fn get_user_v2(Path(id): Path<String>) -> Json<User> {}
```

Generated TypeScript:
```typescript
export interface UserV1 {
  id: string;
  email: string;
}

export interface User {
  id: string;
  email: string;
  phone_number?: string;
  created_at: string;
}

// Migration helper
const migrateUserV1ToV2 = (user: UserV1): User => ({
  ...user,
  created_at: new Date().toISOString(),
});
```

### Adding Optional Fields

Without breaking existing code:

```rust
#[derive(Serialize)]
pub struct Adapter {
    pub id: String,
    pub name: String,

    // New optional field
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}
```

TypeScript automatically handles:
```typescript
export interface Adapter {
  id: string;
  name: string;
  license?: string; // Optional
}

const adapter: Adapter = {
  id: "ad1",
  name: "code-review",
  // license is optional, no need to provide
};
```

## Testing Generated Types

### Runtime Type Guards

```typescript
import type { User } from './api/types.generated';

function isUser(obj: unknown): obj is User {
  return (
    typeof obj === 'object' &&
    obj !== null &&
    'id' in obj &&
    'email' in obj &&
    typeof obj.id === 'string' &&
    typeof obj.email === 'string'
  );
}

const checkUser = (data: unknown): User => {
  if (!isUser(data)) {
    throw new Error('Invalid user object');
  }
  return data;
};
```

### API Response Validation

```typescript
import type { User } from './api/types.generated';

async function fetchUser(id: string): Promise<User> {
  const response = await fetch(`/v1/users/${id}`);
  const data = await response.json();

  // Verify against schema
  if (!isUser(data)) {
    throw new TypeError(`Invalid user from API: ${JSON.stringify(data)}`);
  }

  return data;
}
```

## Troubleshooting Workflow

### Issue: Types out of sync with API

```bash
# 1. Regenerate
make codegen

# 2. Check what changed
git diff ui/src/api/types.generated.ts

# 3. Verify TS still type-checks
cd ui && pnpm exec tsc --noEmit

# 4. Commit if valid
git add ui/src/api/types.generated.ts
git commit -m "chore: regenerate API types"
```

### Issue: Build fails during codegen

```bash
# 1. Check server crate builds
cargo build -p adapteros-server-api

# 2. Review error messages for missing fields or types

# 3. Verify utoipa attributes on all public handlers
grep -r "#\[utoipa::path" crates/adapteros-server-api/src/

# 4. Try clean rebuild
cargo clean
cargo build -p adapteros-server-api
```

### Issue: Generated types missing

```bash
# 1. Verify spec was generated
ls -la target/codegen/openapi.json
cat target/codegen/openapi.json | jq '.' > /dev/null

# 2. Check endpoint count
cat target/codegen/openapi.json | jq '.paths | keys | length'

# 3. Check openapi-typescript installed
cd ui && pnpm ls openapi-typescript

# 4. Manual regeneration
cd ui && pnpm exec openapi-typescript \
  ../target/codegen/openapi.json \
  --output src/api/types.generated.ts
```

## Best Practices Checklist

Before running `make codegen`:

- [ ] All Rust API changes are complete
- [ ] Handler functions have `#[utoipa::path(...)]` annotations
- [ ] Request/response types are properly documented with doc comments
- [ ] No syntax errors in Rust code

After running `make codegen`:

- [ ] Generated `types.generated.ts` has no TypeScript errors
- [ ] Run `cd ui && pnpm exec tsc --noEmit` to verify
- [ ] Review changes in `git diff`
- [ ] Commit generated files alongside source changes

## Performance Tips

### Speed up incremental builds

```bash
# Enable cargo incremental compilation
export CARGO_INCREMENTAL=1

# Run with parallel jobs
cargo build --release -j 4
```

### Cache OpenAPI spec

If not changing Rust API frequently:
```bash
# Store spec in VCS
git add target/codegen/openapi.json

# Skip rebuild on CI if no API changes
if git diff --quiet origin/main crates/adapteros-server-api; then
  echo "No API changes, skipping codegen"
else
  make codegen
fi
```

## References

- Utoipa documentation: https://docs.rs/utoipa/latest/
- OpenAPI specification: https://spec.openapis.org/
- TypeScript handbook: https://www.typescriptlang.org/docs/
