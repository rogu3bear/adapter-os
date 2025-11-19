# Code Generation Command - Complete Index

## Overview

Complete implementation of `cargo xtask codegen` for AdapterOS. Automatically generates TypeScript API types from Rust OpenAPI specifications with full validation.

**Status**: ✓ Production Ready | ✓ Fully Documented | ✓ Compiled & Tested

---

## Quick Navigation

### For Users
- **Quick Start**: `CODEGEN_QUICK_START.md` - 5 minutes to first use
- **Full Pipeline Guide**: `docs/CODEGEN_PIPELINE.md` - Complete feature documentation
- **Usage Examples**: `docs/CODEGEN_EXAMPLES.md` - Real-world patterns and workflows

### For Developers
- **Architecture Design**: `docs/CODEGEN_DESIGN.md` - Technical deep-dive
- **Implementation**: `xtask/src/codegen.rs` - 380 lines of Rust code
- **Integration Points**: `xtask/src/main.rs`, `Makefile`

### Deliverables Summary
- **Overview**: `CODEGEN_DELIVERABLES.md` - What was delivered

---

## File Structure

```
/Users/star/Dev/aos/
├── xtask/src/
│   ├── codegen.rs                    [380 lines] - Core implementation
│   └── main.rs                       [+3 lines]  - Integration point
├── Makefile                          [+4 lines]  - Make targets
├── docs/
│   ├── CODEGEN_PIPELINE.md          [450+ lines] - User guide
│   ├── CODEGEN_DESIGN.md            [600+ lines] - Architecture
│   ├── CODEGEN_EXAMPLES.md          [550+ lines] - Examples & patterns
├── CODEGEN_QUICK_START.md           [Quick ref] - 1-minute setup
├── CODEGEN_DELIVERABLES.md          [Summary]   - What's included
└── CODEGEN_INDEX.md                 [This file] - Navigation guide
```

---

## Features

### Pipeline (4 Steps)

1. **Dependency Check**
   - Validates Rust, Node.js 18+, pnpm, openapi-typescript
   - Clear error messages with installation instructions

2. **Build & OpenAPI Export**
   - Compiles server via `cargo build --release`
   - Extracts OpenAPI spec using utoipa
   - Output: `target/codegen/openapi.json`

3. **TypeScript Generation**
   - Converts OpenAPI to TypeScript via `openapi-typescript`
   - Formats with Prettier
   - Output: `ui/src/api/types.generated.ts`

4. **Type Validation**
   - Validates OpenAPI spec structure
   - Counts endpoints and schemas
   - Verifies TypeScript exports
   - Non-fatal warnings don't block pipeline

### Key Capabilities

- ✓ Full type consistency Rust ↔ TypeScript
- ✓ Async/await for long operations
- ✓ Structured error handling
- ✓ Execution timing per step
- ✓ Pretty-printed reports
- ✓ Verbose output mode
- ✓ Integration with Makefile
- ✓ CI/CD ready

---

## Usage

### One Command

```bash
make codegen
```

### With Verbose Output

```bash
make codegen-verbose
```

### Direct Invocation

```bash
cargo xtask codegen
```

### Result

```
✓ Dependency Check
✓ Build & OpenAPI Export (45s)
✓ TypeScript Generation (3s)
✓ Type Validation
✓ Code generation completed successfully
```

---

## Integration

### Makefile Targets

```makefile
codegen           # Full pipeline
codegen-verbose   # With debug output
```

### xtask Command

```rust
Some("codegen") => codegen::run().await?,
```

### CI/CD Example

```yaml
- name: Generate API types
  run: make codegen

- name: Verify types committed
  run: git diff --exit-code ui/src/api/types.generated.ts
```

### Pre-commit Hook

```bash
#!/bin/bash
cargo xtask codegen
git add ui/src/api/types.generated.ts
```

---

## API Type Flow

### Input: Rust Code

```rust
#[derive(Serialize)]
pub struct User {
    pub id: String,
    pub email: String,
}

#[utoipa::path(
    post,
    path = "/v1/users",
    request_body = CreateUserRequest,
    responses((status = 201, body = User))
)]
pub async fn create_user(...) { ... }
```

### Intermediate: OpenAPI JSON

```json
{
  "paths": {
    "/v1/users": {
      "post": {
        "requestBody": {...},
        "responses": {
          "201": {"content": {"application/json": {"schema": {"$ref": "#/components/schemas/User"}}}}
        }
      }
    }
  },
  "components": {
    "schemas": {
      "User": {
        "type": "object",
        "properties": {
          "id": {"type": "string"},
          "email": {"type": "string"}
        }
      }
    }
  }
}
```

### Output: TypeScript Types

```typescript
export interface User {
  id: string;
  email: string;
}

export interface CreateUserRequest {
  email: string;
  display_name: string;
}
```

### Usage in TypeScript

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

---

## Type Mapping

| Rust | OpenAPI | TypeScript |
|------|---------|-----------|
| `String` | `type: string` | `string` |
| `i64` | `type: integer, format: int64` | `number` |
| `bool` | `type: boolean` | `boolean` |
| `Option<T>` | nullable or missing | `T?` \| `T \| null` |
| `Vec<T>` | `type: array, items: {...}` | `T[]` |
| `enum Color { Red, Blue }` | `enum: ["Red", "Blue"]` | `"Red" \| "Blue"` |
| `struct User { ... }` | Named schema | `interface User { ... }` |

---

## Performance

| Operation | Time |
|-----------|------|
| Dependency check | <1s |
| Build (first) | 30-60s |
| Build (incremental) | 5-15s |
| TypeScript generation | 2-5s |
| Formatting | 1-2s |
| Validation | <1s |
| **Total (first)** | **40-80s** |
| **Total (incremental)** | **10-20s** |

---

## Dependencies

### Required

| Tool | Version | Why | Install |
|------|---------|-----|---------|
| Rust | 1.75+ | Compile server | `rustup update` |
| Node.js | 18+ | Run openapi-typescript | `brew install node@20` |
| pnpm | 8+ | Package manager | `npm install -g pnpm` |
| openapi-typescript | 6+ | Type generation | `cd ui && pnpm add -D openapi-typescript` |

### Dependency Checking

Pipeline automatically validates all requirements before running. Early exit with clear installation instructions if tools missing.

---

## Documentation Map

### Quick Reference (5 min)
→ `CODEGEN_QUICK_START.md`

### User Guide (30 min)
→ `docs/CODEGEN_PIPELINE.md`
- Setup & installation
- Step-by-step workflow
- Configuration
- Troubleshooting
- Best practices

### Code Examples (20 min)
→ `docs/CODEGEN_EXAMPLES.md`
- New endpoint development
- Type updates
- Enum patterns
- Nested types
- CI/CD integration
- Testing
- Pre-commit hooks

### Architecture (45 min)
→ `docs/CODEGEN_DESIGN.md`
- System design
- Module structure
- Dependency strategy
- Build process details
- Error handling
- Performance characteristics
- Extension points

### Implementation Details
→ `xtask/src/codegen.rs` (380 lines)
- Well-commented code
- Unit tests included
- Data structures documented

### Deliverables Summary
→ `CODEGEN_DELIVERABLES.md`
- What was delivered
- Architecture overview
- Files summary

---

## Common Workflows

### 1. Create New Endpoint

```bash
# 1. Update Rust code
# Edit crates/adapteros-server-api/src/handlers/

# 2. Generate types
make codegen

# 3. Use in TypeScript
# Import from ui/src/api/types.generated.ts

# 4. Commit
git add crates/adapteros-server-api/src/...
git add ui/src/api/types.generated.ts
git commit -m "feat: new endpoint"
```

### 2. Update Response Type

```bash
# 1. Add field to Rust struct
# In crates/adapteros-server-api/src/types.rs

# 2. Regenerate
make codegen

# 3. TypeScript automatically updated
# Field available with proper typing
```

### 3. Debug Generation Issues

```bash
# Run with verbose output
make codegen-verbose

# Check generated spec
cat target/codegen/openapi.json | jq '.'

# Verify TypeScript syntax
cd ui && pnpm exec tsc --noEmit
```

### 4. Pre-commit Validation

```bash
# Install hook
chmod +x .git/hooks/pre-commit

# Types auto-validated before commit
git commit -m "update API"
```

---

## Troubleshooting

| Issue | Solution |
|-------|----------|
| "Rust/Cargo not found" | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| "Node.js not found" | `brew install node@20` |
| "Node.js < 18" | `brew upgrade node` |
| "pnpm not found" | `npm install -g pnpm` |
| "openapi-typescript not found" | `cd ui && pnpm add -D openapi-typescript` |
| "Build fails" | `cargo build -p adapteros-server-api` (check server crate) |
| "No endpoints found" | Add `#[utoipa::path(...)]` to handlers |
| "Types incomplete" | Verify OpenAPI spec: `cat target/codegen/openapi.json \| jq '.paths \| keys \| length'` |

---

## Testing

### Unit Tests

Included in `xtask/src/codegen.rs`:
```rust
#[test]
fn test_find_workspace_root() { ... }

#[test]
fn test_codegen_report_all_success() { ... }
```

### Integration Testing

```bash
# Generate types
make codegen

# Verify files exist
test -f target/codegen/openapi.json
test -f ui/src/api/types.generated.ts

# Type-check UI
cd ui && pnpm exec tsc --noEmit

# Check endpoint count
cat target/codegen/openapi.json | jq '.paths | keys | length'
```

---

## Design Principles

1. **Reliability**: Multiple validation stages catch issues early
2. **Clarity**: Clear error messages and comprehensive documentation
3. **Performance**: Incremental compilation with caching support
4. **Extensibility**: Plugin architecture for future enhancements
5. **Integration**: Seamlessly fits into existing workflows

---

## Future Enhancements

1. **Configuration file** (`.codegenrc.json`)
2. **Custom transformers** (plugin architecture)
3. **Additional formats** (GraphQL, Postman, SDKs)
4. **Incremental generation** (changed endpoints only)
5. **Caching** (skip TS if spec unchanged)
6. **Watch mode** (auto-regenerate on file changes)

---

## Compilation Status

✓ `xtask/src/codegen.rs` compiles without errors
✓ Integrates cleanly with existing `xtask` commands
✓ Dependencies properly declared in `Cargo.toml`
✓ Follows project conventions and patterns
✓ Includes unit tests
✓ Production-ready

---

## Related Commands

```bash
make codegen              # Full code generation
make codegen-verbose      # With verbose output
make openapi-docs         # Legacy spec generation
make validate-openapi     # Validate spec only
make build                # Build all crates
make test                 # Run test suite
cargo xtask codegen       # Direct invocation
cargo xtask --help        # View all xtask commands
```

---

## Getting Started

### Immediate Use (1 minute)

```bash
make codegen
```

### Learn Full Features (30 minutes)

Read: `docs/CODEGEN_PIPELINE.md`

### Understand Design (45 minutes)

Read: `docs/CODEGEN_DESIGN.md`

### See Examples (20 minutes)

Read: `docs/CODEGEN_EXAMPLES.md`

### Explore Code (varies)

Review: `xtask/src/codegen.rs`

---

## Support & References

- **utoipa docs**: https://docs.rs/utoipa/
- **OpenAPI 3.0 spec**: https://spec.openapis.org/oas/v3.0.3
- **openapi-typescript**: https://openapi-ts.dev/
- **Rust book**: https://doc.rust-lang.org/book/
- **TypeScript handbook**: https://www.typescriptlang.org/docs/

---

## Summary

The `cargo xtask codegen` command provides a complete, production-ready solution for:

- Maintaining type safety between Rust APIs and TypeScript UI
- Automating OpenAPI specification generation
- Validating API contract consistency
- Streamlining API development workflows
- Enabling robust CI/CD integration

**Ready to use**: Run `make codegen` and start generating types!

---

**Last Updated**: 2025-11-19
**Status**: Production Ready
**Maintainer**: James KC Auchterlonig
**License**: Dual MIT OR Apache-2.0
