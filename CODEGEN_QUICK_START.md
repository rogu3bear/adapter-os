# Code Generation Quick Start

## One-Minute Setup

```bash
# 1. Ensure dependencies installed
brew install node@20    # If not already installed
npm install -g pnpm    # If not already installed

# 2. Run code generation
make codegen

# 3. Done! Types updated in ui/src/api/types.generated.ts
```

## Most Common Tasks

### Generate API types after API changes
```bash
make codegen
```

### See what changed
```bash
git diff ui/src/api/types.generated.ts
git diff target/codegen/openapi.json
```

### Debug generation issues
```bash
make codegen-verbose
```

### Verify types are correct
```bash
cd ui && pnpm exec tsc --noEmit
```

## The Four Steps

```
Step 1: Check Rust, Node.js, pnpm installed
        ↓
Step 2: Build server & export OpenAPI spec
        ↓
Step 3: Convert OpenAPI to TypeScript types
        ↓
Step 4: Validate consistency
```

## Expected Output Files

```
target/codegen/openapi.json          ← OpenAPI specification
ui/src/api/types.generated.ts        ← Generated TypeScript types
```

## Using Generated Types

```typescript
import type { User, CreateUserRequest } from './api/types.generated';

const user: User = {
  id: "123",
  email: "user@example.com",
};

const req: CreateUserRequest = {
  email: "new@example.com",
  display_name: "John",
};
```

## Development Workflow

```bash
# 1. Update Rust API
# Edit crates/adapteros-server-api/src/handlers/*.rs

# 2. Generate new types
make codegen

# 3. Use types in UI
# Import from ui/src/api/types.generated.ts

# 4. Commit changes
git add crates/adapteros-server-api/src/...
git add ui/src/api/types.generated.ts
git add target/codegen/openapi.json
git commit -m "feat: new API endpoint with types"
```

## Troubleshooting

| Problem | Solution |
|---------|----------|
| "Node.js not found" | `brew install node@20` |
| "pnpm not found" | `npm install -g pnpm` |
| "openapi-typescript not found" | `cd ui && pnpm add -D openapi-typescript` |
| "Build failed" | `cargo build -p adapteros-server-api` |
| "No endpoints found" | Add `#[utoipa::path(...)]` to handlers |

## Adding a New API Endpoint

```rust
// 1. Define types
#[derive(Serialize, Deserialize)]
pub struct CreateItemRequest {
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub name: String,
}

// 2. Add handler with utoipa
#[utoipa::path(
    post,
    path = "/v1/items",
    request_body = CreateItemRequest,
    responses(
        (status = 201, body = Item),
        (status = 400)
    )
)]
pub async fn create_item(
    Body(req): Body<CreateItemRequest>,
) -> (StatusCode, Json<Item>) {
    // ...
}

// 3. Register in routes
// ...

// 4. Generate types
make codegen

// 5. Use in TypeScript
import type { Item, CreateItemRequest } from './api/types.generated';
const item = await fetch('/v1/items', {
  method: 'POST',
  body: JSON.stringify({name: "My Item"})
}).then(r => r.json()) as Item;
```

## CI Integration

Add to GitHub Actions:
```yaml
- name: Generate API types
  run: make codegen

- name: Verify types up to date
  run: git diff --exit-code ui/src/api/types.generated.ts || exit 1
```

## Environment Variables

```bash
# Enable verbose output
VERBOSE=1 cargo xtask codegen

# Custom output directory (future)
CODEGEN_OUTPUT=./custom/output make codegen
```

## Key Commands

| Command | Purpose |
|---------|---------|
| `make codegen` | Full pipeline |
| `make codegen-verbose` | With debug output |
| `cargo xtask codegen` | Direct invocation |
| `make validate-openapi` | Validate spec only |
| `make openapi-docs` | Legacy spec generation |

## Type Consistency

Rust types automatically become TypeScript:

```rust
String             →  string
i64                →  number
bool               →  boolean
Option<T>          →  T | null
Vec<T>             →  T[]
enum Color{Red}    →  "Red" (in union type)
struct User{...}   →  interface User{...}
```

## Performance

- **First run**: 40-80 seconds
- **Incremental**: 10-20 seconds
- **Mostly**: Cargo rebuild time

## Files to Know

| File | Purpose |
|------|---------|
| `xtask/src/codegen.rs` | Implementation |
| `Makefile` | Targets |
| `docs/CODEGEN_PIPELINE.md` | Full guide |
| `docs/CODEGEN_DESIGN.md` | Architecture |
| `docs/CODEGEN_EXAMPLES.md` | Examples |

## Help & Docs

```bash
# View Makefile help
make help

# View xtask help
cargo xtask

# Full documentation
cat docs/CODEGEN_PIPELINE.md
```

## Pre-Commit Hook

Create `.git/hooks/pre-commit`:

```bash
#!/bin/bash
set -e
cargo xtask codegen
git add ui/src/api/types.generated.ts
```

Make executable:
```bash
chmod +x .git/hooks/pre-commit
```

## What Gets Generated

**OpenAPI Spec** (JSON):
- All API endpoints
- Request/response schemas
- Authentication info
- Server configurations

**TypeScript Types** (TypeScript):
- Type definitions for all schemas
- Request interfaces
- Response interfaces
- Enum types

## Common Patterns

### Query with types
```typescript
import type { User } from './api/types.generated';

const getUser = async (id: string): Promise<User> => {
  return fetch(`/v1/users/${id}`).then(r => r.json());
};
```

### Create with types
```typescript
import type { CreateUserRequest, User } from './api/types.generated';

const createUser = async (req: CreateUserRequest): Promise<User> => {
  return fetch('/v1/users', {
    method: 'POST',
    body: JSON.stringify(req),
  }).then(r => r.json());
};
```

### Enums with types
```typescript
import type { UserRole } from './api/types.generated';

const roleOptions: UserRole[] = ['admin', 'user', 'viewer'];
```

## Next Steps

1. Read full documentation: `docs/CODEGEN_PIPELINE.md`
2. See examples: `docs/CODEGEN_EXAMPLES.md`
3. Understand design: `docs/CODEGEN_DESIGN.md`
4. Start generating: `make codegen`

---

**Status**: ✓ Ready to use
**Documentation**: ✓ Complete
**Implementation**: ✓ Compiled
**Tests**: ✓ Included
