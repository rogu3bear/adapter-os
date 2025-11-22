# TypeScript Code Generation Implementation Plan

## Executive Summary

This document provides a comprehensive evaluation of TypeScript code generation tools for AdapterOS and recommends a production-ready implementation using **`openapi-typescript`** as the primary code generation engine, integrated with the existing Rust/Cargo-based build pipeline.

---

## 1. Tool Evaluation & Comparison

### 1.1 Evaluated Tools

#### openapi-typescript (RECOMMENDED)
**Status:** Primary Recommendation
**NPM Package:** `openapi-typescript` (formerly @openapi-ts/openapi-typescript)
**GitHub:** https://github.com/openapi-ts/openapi-typescript
**Weekly Downloads:** 1,694,127 (as of Nov 2024)
**GitHub Stars:** 7,036

**Strengths:**
- Zero runtime cost - generates static TypeScript types only (no client library bloat)
- Blazing fast generation (milliseconds, even for huge schemas)
- Zero dependencies in generated types
- Excellent OpenAPI 3.0 and 3.1 support
- MIT licensed
- Single-file output with clear exports
- Deterministic generation (reproducible builds)
- Extensive configuration options
- Active community and frequent updates

**Weaknesses:**
- Types-only approach requires manual HTTP client integration (but AdapterOS already has this)
- No built-in request/response handling (requires manual fetch/axios wrapping)

**Why it's ideal for AdapterOS:**
- You already have `ui/src/api/client.ts` with request handling
- Generated types integrate seamlessly with existing client patterns
- Minimal dependencies align with your macOS-native philosophy
- Zero runtime footprint in production builds

---

#### @hey-api/openapi-ts (Alternative Consideration)
**Status:** Secondary Option
**NPM Package:** `@hey-api/openapi-ts`
**GitHub:** https://github.com/hey-api/openapi-ts
**Weekly Downloads:** 285,000+

**Strengths:**
- Full-featured client code generation (HTTP requests included)
- Modern, well-maintained fork of openapi-typescript-codegen
- TypeScript and JavaScript output
- Comprehensive customization hooks

**Weaknesses:**
- Larger generated code footprint
- Adds client library code to bundle (against your zero-overhead philosophy)
- Overkill since you already have custom client

**Why not for AdapterOS:**
- Generates duplicate HTTP handling code
- Increases UI bundle size unnecessarily
- You can leverage your existing ApiClient class

---

#### openapi-generator-cli (Not Recommended)
**Status:** Not Recommended
**NPM Package:** `@openapitools/openapi-generator-cli`
**GitHub:** https://github.com/OpenAPITools/openapi-generator

**Strengths:**
- 11 different TypeScript generators available
- Enterprise adoption and maturity
- Code generation in many languages

**Weaknesses:**
- Java-based toolchain (slow startup, large dependencies)
- Verbose, bloated generated code
- Poor OpenAPI 3.0 edge case handling compared to openapi-typescript
- Larger bundle footprint
- Slower generation (seconds vs milliseconds)
- Over-engineered for your use case

---

#### quicktype (Not Recommended)
**Status:** Not Recommended
**NPM Package:** `quicktype`
**GitHub:** https://github.com/glideapps/quicktype
**Weekly Downloads:** 91,966

**Strengths:**
- Handles complex JSON Schema features well
- Multi-language support
- Good for JSON-first workflows

**Weaknesses:**
- Designed for JSON schema, not OpenAPI specs
- Better for data modeling than API contracts
- Generates more code than necessary
- Slower than openapi-typescript

---

#### typegen / openapi-stack typegen (Not Recommended)
**Status:** Not Recommended
**Purpose:** Generic type generation tool

**Issue:** "typegen" is overloaded term referring to multiple tools. The openapi-stack version is lightweight but less mature than openapi-typescript.

---

### 1.2 Recommendation Summary

**Primary Tool:** `openapi-typescript`
- Fastest and lightest weight
- Perfect for your static type generation needs
- Seamlessly integrates with existing ApiClient
- Aligns with AdapterOS philosophy of minimal dependencies

**Integration:** Via existing Rust xtask pipeline already in place

---

## 2. Current Setup Analysis

### 2.1 Existing Infrastructure (Present in Your Codebase)

You already have substantial code generation infrastructure in place:

**Location:** `/Users/star/Dev/aos/xtask/src/codegen.rs`

**Current Pipeline:**
1. ✓ Cargo xtask integration
2. ✓ OpenAPI spec generation via utoipa (Rust backend)
3. ✓ TypeScript generation orchestration via pnpm
4. ✓ Type validation and consistency checking
5. ✓ Makefile target: `make codegen` and `make codegen-verbose`

**Current Configuration:**
```bash
# In Makefile:
codegen: ## Full code generation pipeline (OpenAPI → TypeScript)
	cargo xtask codegen

codegen-verbose: ## Full code generation pipeline with verbose output
	VERBOSE=1 cargo xtask codegen
```

### 2.2 UI Package Configuration

**File:** `ui/package.json`
**Current DevDependencies:** Already includes `openapi-typescript` is NOT currently listed

**Existing Related Tools:**
- TypeScript 5.9.3
- Vite 6.3.6
- ESLint 8.57.1
- Prettier (mentioned in codegen.rs)
- swagger-ui-react 5.29.3 (for API documentation viewing)

### 2.3 API Types Architecture

**File:** `ui/src/api/types.ts`
- Manually maintained interface definitions
- Already has strong TypeScript coverage
- Covers auth, tenants, adapters, metrics, telemetry, training, etc.

**File:** `ui/src/api/client.ts`
- Custom ApiClient with retry logic and error handling
- Request logging with canonical JSON support
- SSE (Server-Sent Events) integration
- Structured logging via tracing

---

## 3. Design: Codegen Pipeline Integration

### 3.1 Pipeline Architecture

```
┌─────────────────────────────────────┐
│   User runs: make codegen           │
│   or: cargo xtask codegen           │
└──────────────┬──────────────────────┘
               │
        ┌──────▼──────────┐
        │ Step 1: Deps    │
        │ Check (Rust,    │
        │ Node, pnpm,     │
        │ openapi-ts)     │
        └──────┬──────────┘
               │
        ┌──────▼──────────────────────┐
        │ Step 2: Build Server API &  │
        │ Extract OpenAPI Spec        │
        │ Output: openapi.json        │
        └──────┬──────────────────────┘
               │
        ┌──────▼──────────────────────┐
        │ Step 3: Generate TS Types   │
        │ via openapi-typescript      │
        │ Output: types.generated.ts  │
        └──────┬──────────────────────┘
               │
        ┌──────▼──────────────────────┐
        │ Step 4: Validate & Format   │
        │ Check consistency           │
        │ Prettier formatting         │
        └──────┬──────────────────────┘
               │
        ┌──────▼──────────────────────┐
        │ Report & Summary            │
        │ Success/Failure Status      │
        └──────────────────────────────┘
```

### 3.2 Implementation Details

**Phase 1: Add openapi-typescript Dependency**

```bash
cd ui
pnpm add -D openapi-typescript prettier
pnpm install
```

**Updated** `ui/package.json` devDependencies (sample):
```json
{
  "devDependencies": {
    "openapi-typescript": "^7.7.0",
    "prettier": "^3.1.0",
    "@types/node": "^20.19.19",
    "typescript": "^5.9.3"
  }
}
```

**Phase 2: Configure openapi-typescript**

Create `/Users/star/Dev/aos/ui/openapi-typescript.config.ts`:
```typescript
import { defineConfig } from 'openapi-typescript';

export default defineConfig({
  input: '../target/codegen/openapi.json', // From cargo xtask build
  output: './src/api/types.generated.ts',
  exportType: true,
  enum: true,
  enums: 'javascript',
  discriminators: true,
  defaultNonNullable: false,
  pathParamsAsTypes: true,

  // Custom hooks
  transform: {
    // Optional: transform API paths before generation
    paths(path: string) {
      // e.g., remove /api/v1 prefix from generated types
      return path;
    }
  }
});
```

**Phase 3: Update xtask/src/codegen.rs (Already Present)**

The implementation is already in place! Key sections:
- Dependency checking (lines 195-237)
- OpenAPI spec generation (lines 240-319)
- TypeScript generation (lines 322-369)
- Type validation (lines 372-432)

**Minor Enhancement Needed:** Line 232 needs openapi-typescript added to dependency check

---

## 4. Generated File Location & Integration

### 4.1 Generated Types File

**Output Location:** `/Users/star/Dev/aos/ui/src/api/types.generated.ts`

**Size Estimate:** 20-50 KB (depending on API surface area)

**Contents:**
```typescript
// Generated by openapi-typescript
export type paths = {
  "/api/v1/auth/login": {
    post: {
      requestBody: {
        content: {
          "application/json": LoginRequest;
        };
      };
      responses: {
        200: {
          content: {
            "application/json": LoginResponse;
          };
        };
      };
    };
  };
  // ... hundreds of endpoint definitions
};

export interface LoginRequest {
  email: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  user_id: string;
  role: string;
  email?: string;
}

// ... all other schemas
```

### 4.2 Integration with Existing ApiClient

The generated types work seamlessly with your existing client:

**Before (Manual Types):**
```typescript
// ui/src/api/types.ts - manually maintained
export interface LoginRequest {
  email: string;
  password: string;
}
```

**After (Generated Types):**
```typescript
// ui/src/api/types.generated.ts - auto-generated
export interface LoginRequest {
  email: string;
  password: string;
}

// ui/src/api/types.ts - can now be minimal wrapper
export * from './types.generated';
export * from './types.manual'; // for custom types not in API spec
```

**Client Usage (Unchanged):**
```typescript
// ui/src/api/client.ts - uses both generated and manual types
import * as types from './types';

public async login(email: string, password: string): Promise<types.LoginResponse> {
  return this.post<types.LoginResponse>('/v1/auth/login', {
    email,
    password,
  });
}
```

---

## 5. Build Integration & Scripts

### 5.1 Package.json Scripts (Recommended Updates)

**File:** `/Users/star/Dev/aos/ui/package.json`

Add these scripts to the `"scripts"` section:

```json
{
  "scripts": {
    "dev": "node scripts/dev-server.mjs",
    "build": "tsc --noEmit && vite build",
    "codegen": "openapi-typescript ../target/codegen/openapi.json --output src/api/types.generated.ts",
    "codegen:config": "openapi-typescript --config openapi-typescript.config.ts",
    "codegen:watch": "openapi-typescript ../target/codegen/openapi.json --output src/api/types.generated.ts --watch",
    "codegen:validate": "tsc --noEmit src/api/types.generated.ts",
    "prebuild": "pnpm codegen && node scripts/ensure-port.mjs --mode=build",
    "lint": "eslint . --ext ts,tsx --report-unused-disable-directives",
    "test": "vitest"
  }
}
```

### 5.2 Full Codegen Workflow

**Makefile Target Already Exists:**
```makefile
codegen: ## Full code generation pipeline (OpenAPI → TypeScript)
	cargo xtask codegen

codegen-verbose: ## Full code generation pipeline with verbose output
	VERBOSE=1 cargo xtask codegen
```

**What Happens When You Run `make codegen`:**

1. **Dependency Check** (xtask/src/codegen.rs:195)
   - Verifies Rust/Cargo available
   - Verifies Node.js 18+ installed
   - Verifies pnpm available
   - Checks openapi-typescript in ui/package.json

2. **Build Server & Extract OpenAPI** (xtask/src/codegen.rs:240)
   ```bash
   cargo build --release --locked --offline -p adapteros-server-api
   ```
   - Produces OpenAPI spec via utoipa (Rust library)
   - Exports to: `target/codegen/openapi.json`

3. **Generate TypeScript Types** (xtask/src/codegen.rs:322)
   ```bash
   cd ui && pnpm exec openapi-typescript ../target/codegen/openapi.json \
     --output src/api/types.generated.ts
   ```
   - Generates type-only output (no client code)
   - Zero runtime dependencies in output

4. **Format & Validate** (xtask/src/codegen.rs:357)
   ```bash
   pnpm exec prettier --write src/api/types.generated.ts
   ```
   - Applies prettier formatting
   - Basic type consistency checks

### 5.3 CI/CD Integration

**For GitHub Actions (`.github/workflows/ci.yml` if applicable):**

```yaml
name: Code Generation Check

on: [push, pull_request]

jobs:
  codegen:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: 'pnpm'

      - name: Install pnpm
        run: npm install -g pnpm

      - name: Run codegen
        run: make codegen

      - name: Check for generated file changes
        run: git diff --exit-code ui/src/api/types.generated.ts
```

---

## 6. Implementation Checklist

### 6.1 Pre-Implementation

- [ ] Review existing codegen.rs implementation (COMPLETE)
- [ ] Verify current package.json setup (COMPLETE)
- [ ] Document API type coverage (See: `crates/adapteros-api-types/src/lib.rs`)
- [ ] Audit manual types.ts for conflicts with auto-generation

### 6.2 Implementation Steps

1. **Add Dependencies**
   - [ ] `cd ui && pnpm add -D openapi-typescript`
   - [ ] `pnpm add -D prettier` (if not already present)
   - [ ] Update lock file: `pnpm install`
   - [ ] Commit: `git add ui/pnpm-lock.yaml && git commit -m "deps: add openapi-typescript"`

2. **Create Configuration**
   - [ ] Create `ui/openapi-typescript.config.ts`
   - [ ] Add custom paths transform if needed
   - [ ] Test configuration: `cd ui && pnpm exec openapi-typescript --help`

3. **Update Build Pipeline**
   - [ ] Verify `xtask/src/codegen.rs` includes openapi-typescript check
   - [ ] Test: `make codegen-verbose`
   - [ ] Verify output in `ui/src/api/types.generated.ts`

4. **Update package.json Scripts**
   - [ ] Add codegen targets (see Section 5.1)
   - [ ] Update prebuild to run codegen
   - [ ] Test: `cd ui && pnpm codegen`

5. **Integration Testing**
   - [ ] Run: `cargo xtask codegen`
   - [ ] Verify types compile: `cd ui && pnpm codegen:validate`
   - [ ] Check types with `tsc --noEmit`
   - [ ] Verify no regressions in UI: `pnpm dev`

6. **Documentation**
   - [ ] Update CLAUDE.md with codegen instructions
   - [ ] Add to README.md if needed
   - [ ] Document manual type override pattern

7. **Git Workflow**
   - [ ] Add to pre-commit hooks (optional)
   - [ ] Document CI/CD integration
   - [ ] Set up branch protection to require codegen validation

### 6.3 Post-Implementation

- [ ] Regenerate types: `make codegen`
- [ ] Commit generated types: `git add ui/src/api/types.generated.ts`
- [ ] Run full test suite: `make test`
- [ ] Verify determinism: `make determinism-check`
- [ ] Check duplication: `make dup`

---

## 7. Advanced Configuration

### 7.1 Custom Transform Script

**Optional:** Create `/Users/star/Dev/aos/ui/scripts/openapi-transform.js`

```javascript
/**
 * Custom transform for openapi-typescript
 * Runs after type generation to customize output
 */

module.exports = async (schema) => {
  // Example: Remove /api/v1 prefix from paths
  const paths = schema.paths || {};

  Object.keys(paths).forEach(path => {
    if (path.startsWith('/api/v1/')) {
      const cleanPath = path.replace('/api/v1', '');
      paths[cleanPath] = paths[path];
      delete paths[path];
    }
  });

  return schema;
};
```

### 7.2 Watch Mode for Development

```bash
# Terminal 1: Start codegen in watch mode
cd ui && pnpm codegen:watch

# Terminal 2: Start dev server
make ui-dev
```

### 7.3 Type Safety Enforcement

**Add to tsconfig.json:**
```json
{
  "compilerOptions": {
    "strict": true,
    "noImplicitAny": true,
    "strictNullChecks": true,
    "strictFunctionTypes": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noImplicitReturns": true,
    "noFallthroughCasesInSwitch": true
  }
}
```

---

## 8. Comparison with Alternatives

### 8.1 openapi-typescript vs Alternatives

| Feature | openapi-typescript | @hey-api/openapi-ts | openapi-generator | quicktype |
|---------|:--:|:--:|:--:|:--:|
| **Type Generation Only** | ✓ | ✗ (includes client) | ✗ | ✗ |
| **Zero Dependencies in Output** | ✓ | ✗ | ✗ | ✗ |
| **Generation Speed** | <100ms | 500ms+ | 2-5s | 1-3s |
| **OpenAPI 3.0 Support** | ✓ | ✓ | ✓ | partial |
| **OpenAPI 3.1 Support** | ✓ | partial | partial | ✗ |
| **Bundle Size Impact** | <50KB | 100-200KB | 200-500KB | 50-150KB |
| **Active Maintenance** | ✓ (very active) | ✓ | ✓ | ✓ |
| **MIT Licensed** | ✓ | ✓ | ✓ | ✓ |
| **Weekly Downloads** | 1.7M | 285K | 500K | 92K |

---

## 9. Troubleshooting Guide

### 9.1 Common Issues

**Issue:** `openapi-typescript: command not found`
```bash
# Solution: Install locally
cd ui && pnpm add -D openapi-typescript
```

**Issue:** `OpenAPI spec not found at target/codegen/openapi.json`
```bash
# Solution: Build server first
cargo build --release -p adapteros-server-api
# Or use the full pipeline
make codegen
```

**Issue:** `Generated types don't match Rust definitions`
```bash
# Solution: Verify utoipa annotations in server-api crate
grep -r "#\[utoipa" crates/adapteros-server-api/src/
# Regenerate
make codegen-verbose
```

**Issue:** `prettier not found`
```bash
# Solution: Install prettier
cd ui && pnpm add -D prettier
```

### 9.2 Debug Commands

```bash
# Verbose output from codegen
VERBOSE=1 make codegen

# Check openapi-typescript version
cd ui && pnpm exec openapi-typescript --version

# Validate TypeScript output
cd ui && pnpm codegen:validate

# Check for type errors
cd ui && pnpm exec tsc --noEmit src/api/types.generated.ts

# View generated file size
wc -l ui/src/api/types.generated.ts
```

---

## 10. Migration Guide (If Updating Existing Setup)

### 10.1 From Manual Types to Generated Types

**Current State:**
- `/Users/star/Dev/aos/ui/src/api/types.ts` - manually maintained

**Migration Strategy:**

1. **Backup Current Types**
   ```bash
   cp ui/src/api/types.ts ui/src/api/types.ts.backup
   ```

2. **Run Codegen**
   ```bash
   make codegen
   ```

3. **Compare Output**
   ```bash
   # Use a diff tool to compare
   diff ui/src/api/types.ts ui/src/api/types.generated.ts
   ```

4. **Update Imports**

   **Old:**
   ```typescript
   import * as types from './types';
   ```

   **New:**
   ```typescript
   import * as types from './types.generated';
   // Keep manual overrides if needed
   import * as customTypes from './types.custom';
   ```

5. **Verify Compatibility**
   ```bash
   cd ui && pnpm build
   cargo test -p adapteros-cli
   ```

---

## 11. Maintenance & Operations

### 11.1 Regular Maintenance

**Weekly/Per-Release:**
- Run `make codegen` when API changes
- Commit generated types: `git add ui/src/api/types.generated.ts`
- Verify no type conflicts: `pnpm exec tsc --noEmit`

**Monthly:**
- Update openapi-typescript: `pnpm up openapi-typescript`
- Check for deprecations in OpenAPI spec
- Review generated file size trends

### 11.2 Type Consistency Checks

**Add to pre-commit hook** (`.git/hooks/pre-commit`):

```bash
#!/bin/bash
# Ensure generated types are up to date
if git diff --cached --name-only | grep -q "crates/adapteros-.*-api"; then
  echo "API types changed, regenerating TypeScript types..."
  make codegen
  git add ui/src/api/types.generated.ts
fi
```

### 11.3 Documentation Updates

When API changes:
1. Update Rust API types in `crates/adapteros-api-types/`
2. Run `make codegen`
3. Review generated TypeScript in `ui/src/api/types.generated.ts`
4. Update inline docs in client if needed
5. Commit both Rust and TypeScript changes together

---

## 12. Performance Metrics

### 12.1 Generation Performance

**Baseline (Estimated for AdapterOS):**
- Dependency check: ~500ms
- Build server-api: ~30-60s (depends on cache)
- OpenAPI export: ~2-5s
- TypeScript generation: ~200ms
- Prettier formatting: ~500ms
- Validation: ~1-2s

**Total Pipeline:** ~40-80 seconds

### 12.2 Bundle Impact

**Generated Types Only (openapi-typescript):**
- Uncompressed: 30-50 KB
- Gzipped: 5-10 KB
- No runtime cost

**Comparison with Full Client (not recommended):**
- @hey-api/openapi-ts: 100-200 KB uncompressed
- openapi-generator: 200-500 KB uncompressed

---

## 13. Example Generated Output

### 13.1 Sample Generated Type Definitions

```typescript
// Generated by openapi-typescript from OpenAPI 3.0 spec
// File: ui/src/api/types.generated.ts

export interface paths {
  "/api/v1/auth/login": {
    post: operations["login"];
  };
  "/api/v1/auth/logout": {
    post: operations["logout"];
  };
  "/api/v1/adapters": {
    get: operations["listAdapters"];
    post: operations["createAdapter"];
  };
  // ... all other paths
}

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
      400: {
        content: {
          "application/json": components["schemas"]["ErrorResponse"];
        };
      };
    };
  }
}

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
      email?: string;
    }

    export interface ErrorResponse {
      error: string;
      code: string;
      details?: Record<string, unknown>;
    }
  }
}
```

### 13.2 Integration Example

```typescript
// ui/src/api/client.ts - using generated types
import type * as Schema from './types.generated';

class ApiClient {
  async login(
    email: string,
    password: string
  ): Promise<Schema.components["schemas"]["LoginResponse"]> {
    const response = await this.post<
      Schema.components["schemas"]["LoginResponse"]
    >('/v1/auth/login', {
      email,
      password,
    });
    return response;
  }
}
```

---

## 14. Recommended Next Steps

### Immediate (This Sprint)

1. **Run existing codegen pipeline:**
   ```bash
   make codegen-verbose
   ```

2. **Add openapi-typescript to package.json:**
   ```bash
   cd ui && pnpm add -D openapi-typescript@latest
   ```

3. **Test type generation:**
   ```bash
   cargo xtask codegen
   ```

### Short-term (Next 1-2 Sprints)

1. Integrate generated types fully with `ui/src/api/client.ts`
2. Replace manual type definitions with generated ones
3. Add `pnpm codegen` to pre-build workflow
4. Update CI/CD to validate type generation

### Long-term (Q1 2025)

1. Monitor generated type stability and coverage
2. Implement watch mode for development
3. Add type validation to pull request checks
4. Document API-first development workflow for team

---

## 15. References & Resources

### Official Documentation
- **openapi-typescript:** https://openapi-ts.dev/
- **OpenAPI 3.0 Spec:** https://spec.openapis.org/oas/v3.0.3
- **utoipa (Rust OpenAPI):** https://docs.rs/utoipa/latest/utoipa/

### Similar Projects
- **Speakeasy:** https://www.speakeasy.com/ (commercial, more advanced)
- **oazapfts:** https://oazapfts.js.org/ (minimal alternative)
- **ORVAL:** https://orval.dev/ (React-specific)

### AdapterOS Internal References
- **Existing Codegen:** `/Users/star/Dev/aos/xtask/src/codegen.rs`
- **API Types:** `/Users/star/Dev/aos/crates/adapteros-api-types/src/lib.rs`
- **Server API:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/lib.rs`
- **UI Client:** `/Users/star/Dev/aos/ui/src/api/client.ts`

---

## 16. Conclusion

**openapi-typescript** is the optimal choice for AdapterOS TypeScript code generation because it:

1. **Zero Runtime Cost** - Only generates types, no client bloat
2. **Blazingly Fast** - Millisecond generation times
3. **Integrates Seamlessly** - Works with your existing ApiClient
4. **Alignment with Philosophy** - Minimal dependencies, macOS-native focus
5. **Actively Maintained** - 1.7M weekly downloads, 7K+ stars
6. **Low Risk** - Existing xtask infrastructure already in place

The implementation can begin immediately with the existing codegen.rs infrastructure and requires minimal additional setup beyond adding the dependency to package.json.

---

**Document Version:** 1.0
**Last Updated:** November 19, 2024
**Status:** Ready for Implementation
