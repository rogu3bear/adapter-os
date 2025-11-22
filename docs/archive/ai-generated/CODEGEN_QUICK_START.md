# TypeScript Code Generation Quick Start

## Overview

AdapterOS uses `openapi-typescript` to automatically generate TypeScript type definitions from your OpenAPI 3.0 API specification. This ensures your frontend types are always in sync with your backend API.

**Key Benefits:**
- Zero runtime cost (types only)
- Automatic sync with backend API changes
- Type-safe API interactions
- <100ms generation time
- Works seamlessly with existing `ui/src/api/client.ts`

---

## Quick Start (5 Minutes)

### Step 1: Install Dependencies

```bash
cd ui
pnpm add -D openapi-typescript prettier
pnpm install
```

### Step 2: Generate Types

Run the full code generation pipeline from the workspace root:

```bash
# From workspace root
make codegen

# Or with verbose output for debugging
make codegen-verbose
```

### Step 3: Verify Generated Types

```bash
# Check the generated file was created
ls -lh ui/src/api/types.generated.ts

# Validate TypeScript
cd ui && pnpm codegen:validate

# View the file
head -100 ui/src/api/types.generated.ts
```

### Step 4: Use Generated Types in Your Code

```typescript
// ui/src/api/client.ts

import type * as Schema from './types.generated';

class ApiClient {
  async login(email: string, password: string) {
    return this.post<Schema.components['schemas']['LoginResponse']>(
      '/v1/auth/login',
      { email, password }
    );
  }
}
```

---

## Available Commands

### From Workspace Root

```bash
# Full pipeline: Build server → Generate OpenAPI → Generate TypeScript types
make codegen

# With debugging output
make codegen-verbose

# Generate OpenAPI documentation (without TypeScript)
make openapi-docs
```

### From `ui/` Directory

```bash
# Generate types directly (requires OpenAPI spec already exists)
pnpm codegen

# Generate types with custom configuration
pnpm codegen:config

# Watch mode for development (regenerates on spec changes)
pnpm codegen:watch

# Validate generated types compile
pnpm codegen:validate
```

---

## Development Workflow

### Using Watch Mode

For active development, use watch mode to regenerate types when the OpenAPI spec changes:

**Terminal 1: Start codegen in watch mode**
```bash
cd ui
pnpm codegen:watch
```

**Terminal 2: Start development server**
```bash
make ui-dev
# or: cd ui && pnpm dev
```

**Terminal 3: (Optional) Start backend server**
```bash
./scripts/start_server.sh
# or: cargo run --release -p adapteros-server
```

---

## Full Codegen Pipeline Explained

When you run `make codegen`, here's what happens:

```
Step 1: Dependency Check (500ms)
  ✓ Rust/Cargo
  ✓ Node.js 18+
  ✓ pnpm
  ✓ openapi-typescript

Step 2: Build & OpenAPI Export (30-60s)
  cargo build --release -p adapteros-server-api
  → Extracts OpenAPI spec via utoipa
  → Output: target/codegen/openapi.json

Step 3: Generate TypeScript Types (100-200ms)
  pnpm exec openapi-typescript
  → Input: target/codegen/openapi.json
  → Output: ui/src/api/types.generated.ts

Step 4: Format & Validate (500ms)
  pnpm exec prettier
  → Formats generated code
  → Basic type consistency checks

Total Time: ~40-80 seconds
```

---

## Generated Type Structure

The generated `types.generated.ts` file contains:

### Endpoint Paths

```typescript
export type paths = {
  "/api/v1/auth/login": {
    post: operations["login"];
  };
  "/api/v1/adapters": {
    get: operations["listAdapters"];
    post: operations["createAdapter"];
  };
  // ... all other endpoints
};
```

### Operations

```typescript
export namespace operations {
  export interface login {
    requestBody: {
      content: {
        "application/json": components["schemas"]["LoginRequest"];
      };
    };
    responses: {
      200: {
        content: {
          "application/json": components["schemas"]["LoginResponse"];
        };
      };
    };
  }
}
```

### Schemas (Request/Response Types)

```typescript
export namespace components {
  export namespace schemas {
    export interface LoginRequest {
      email: string;
      password: string;
    }

    export interface LoginResponse {
      token: string;
      user_id: string;
      role: "admin" | "operator" | "sre" | "viewer";
    }
  }
}
```

---

## Integration Examples

### Example 1: Using Response Types

```typescript
// ui/src/api/client.ts

public async getAdapters(): Promise<Schema.components['schemas']['AdapterList']> {
  return this.get('/v1/adapters');
}
```

### Example 2: Request Type Safety

```typescript
// ui/src/components/AdapterForm.tsx

import type { Schema } from '../api/types.generated';

interface Props {
  onSubmit: (data: Schema.components['schemas']['CreateAdapterRequest']) => void;
}

export function AdapterForm({ onSubmit }: Props) {
  const handleSubmit = (formData: unknown) => {
    // TypeScript ensures formData matches CreateAdapterRequest
    onSubmit(formData as Schema.components['schemas']['CreateAdapterRequest']);
  };
  // ...
}
```

### Example 3: Error Responses

```typescript
// ui/src/api/client.ts

async request<T>(path: string): Promise<T> {
  try {
    const response = await fetch(path);
    return await response.json() as T;
  } catch (error) {
    // Error is typed as ErrorResponse
    const errorResponse: Schema.components['schemas']['ErrorResponse'] = {
      error: error.message,
      code: 'UNKNOWN_ERROR',
    };
    throw errorResponse;
  }
}
```

---

## Troubleshooting

### Issue: "openapi-typescript: command not found"

```bash
# Install locally if not present
cd ui && pnpm add -D openapi-typescript

# Or reinstall all dependencies
pnpm install --force
```

### Issue: "No OpenAPI spec found"

```bash
# Make sure server is built
cargo build --release -p adapteros-server-api

# Or run full pipeline
make codegen
```

### Issue: Generated File is Empty

```bash
# Check OpenAPI spec was generated
ls -la target/codegen/openapi.json

# Try with verbose output
VERBOSE=1 make codegen

# Check for errors
cd ui && pnpm codegen 2>&1
```

### Issue: TypeScript Compilation Errors

```bash
# Validate generated types
cd ui && pnpm codegen:validate

# Check TypeScript configuration
cd ui && pnpm exec tsc --version

# Force regeneration
rm -f ui/src/api/types.generated.ts
make codegen
```

### Issue: Types Don't Match Backend API

```bash
# Ensure server API has utoipa annotations
grep -r "#\[utoipa" crates/adapteros-server-api/src/

# Rebuild and regenerate
make clean
make codegen-verbose
```

---

## Configuration Options

### Custom Transform (Optional)

If you need to customize how types are generated, edit `ui/openapi-typescript.config.ts`:

```typescript
export default defineConfig({
  input: '../target/codegen/openapi.json',
  output: './src/api/types.generated.ts',

  // Custom transformations
  transform: {
    paths(path: string) {
      // Example: Remove /api/v1 prefix
      if (path.startsWith('/api/v1')) {
        return path.replace('/api/v1', '');
      }
      return path;
    },
  },
});
```

### Prettier Formatting (Optional)

To customize Prettier formatting of generated code, create `ui/.prettierrc.json`:

```json
{
  "semi": true,
  "singleQuote": true,
  "trailingComma": "es5",
  "printWidth": 100,
  "tabWidth": 2
}
```

---

## CI/CD Integration

### Pre-commit Hook

Ensure types are regenerated before commits:

```bash
#!/bin/bash
# .git/hooks/pre-commit

if git diff --cached --name-only | grep -q "crates/adapteros-server-api"; then
  echo "API types changed, regenerating TypeScript types..."
  make codegen
  git add ui/src/api/types.generated.ts
  if [ $? -ne 0 ]; then
    echo "Type generation failed, aborting commit"
    exit 1
  fi
fi
```

### GitHub Actions

Add to your CI workflow:

```yaml
- name: Check OpenAPI/TypeScript Sync
  run: |
    make codegen
    git diff --exit-code ui/src/api/types.generated.ts
```

---

## Maintenance

### When to Regenerate

Regenerate types whenever:
- Backend API endpoints change
- Request/response schemas are modified
- HTTP status codes are updated
- New fields are added to request/response bodies
- Before committing API type changes

### Regular Updates

```bash
# Update dependencies monthly
cd ui && pnpm up openapi-typescript

# Check for newer versions
pnpm list openapi-typescript

# Review changelog
# https://github.com/openapi-ts/openapi-typescript/releases
```

### Monitoring Generated Size

```bash
# Check generated file size
wc -l ui/src/api/types.generated.ts
du -h ui/src/api/types.generated.ts

# Typical sizes:
# - Small API (10 endpoints): 5-10 KB
# - Medium API (30+ endpoints): 30-50 KB
# - Large API (100+ endpoints): 100-200 KB
```

---

## Git Workflow

### Committing Generated Types

```bash
# Generate types
make codegen

# Review changes
git diff ui/src/api/types.generated.ts

# Commit together with API changes
git add crates/adapteros-server-api/src/
git add ui/src/api/types.generated.ts
git commit -m "api: update endpoint schema for new adapter registration"
```

### Handling Merge Conflicts

If merge conflicts occur in `types.generated.ts`:

```bash
# Always regenerate instead of manually merging
make codegen

# Then accept generated version
git add ui/src/api/types.generated.ts
git commit -m "chore: regenerate types after merge"
```

---

## Advanced Topics

### Custom OpenAPI Transform

Create `ui/scripts/openapi-transform.js` for advanced customizations:

```javascript
module.exports = async (schema) => {
  // Remove deprecated endpoints
  Object.keys(schema.paths).forEach(path => {
    if (path.includes('deprecated')) {
      delete schema.paths[path];
    }
  });

  return schema;
};
```

### Type Augmentation

Add custom types alongside generated ones:

```typescript
// ui/src/api/types.ts

export type * from './types.generated';

// Add custom types not in OpenAPI spec
export interface ClientConfig {
  apiUrl: string;
  timeout: number;
}
```

### Build-time Validation

Add type checking to build process:

```bash
# ui/package.json
"build": "pnpm codegen:validate && tsc --noEmit && vite build"
```

---

## Performance Considerations

### Codegen Time Breakdown

- Dependency check: ~500ms
- Build server-api: ~30-60s (heaviest step)
- OpenAPI export: ~2-5s
- TypeScript generation: ~100-200ms
- Prettier formatting: ~500ms
- Validation: ~1-2s

**Optimization:** Use `--locked --offline` flags to speed up Rust builds (cache dependencies locally).

### Generated Code Size

- Types only: 5-50 KB (uncompressed)
- Gzipped in bundle: 1-10 KB
- Zero runtime cost

No HTTP client code is included, keeping bundle size minimal.

---

## Support and Resources

- **openapi-typescript Documentation:** https://openapi-ts.dev/
- **OpenAPI 3.0 Specification:** https://spec.openapis.org/oas/v3.0.3
- **GitHub Issues:** https://github.com/openapi-ts/openapi-typescript/issues
- **Workspace Makefile:** `make help` shows all available commands

---

## Next Steps

1. **Run the pipeline:** `make codegen`
2. **Review generated types:** `head -100 ui/src/api/types.generated.ts`
3. **Update imports:** Replace manual types with generated ones in client
4. **Test:** `cd ui && pnpm build`
5. **Commit:** `git add ui/src/api/types.generated.ts && git commit ...`

---

**Last Updated:** November 19, 2024
**Status:** Ready for Use
