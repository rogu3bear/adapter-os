# Code Generation Command Design Document

## Architecture Overview

The `cargo xtask codegen` command implements a 4-step code generation pipeline that maintains type consistency between Rust backend APIs and TypeScript frontend.

```
┌──────────────────────────────────────────────────────────────┐
│  cargo xtask codegen                                         │
│  (Orchestrator: xtask/src/codegen.rs)                       │
└──────────────────────────────────────────────────────────────┘
                           │
        ┌──────────────────┼──────────────────┐
        │                  │                  │
        ▼                  ▼                  ▼
   Step 1:           Step 2:            Step 3:
   Dependency      Build &            TypeScript
   Check          OpenAPI             Generation
   ─────────      Export              ──────────
   · Rust         ──────────          · Invoke pnpm
   · Node.js      · cargo build       · openapi-
   · pnpm         · utoipa extract      typescript
   · openapi-ts   · Output JSON       · Format with
                                        prettier

        │                  │                  │
        └──────────────────┼──────────────────┘
                           │
                           ▼
                      Step 4:
                      Validation
                      ──────────
                      · Check spec
                      · Count types
                      · Verify exports

                           │
                           ▼
                    Print Report
                    & Exit Code
```

## Module Structure

### `xtask/src/codegen.rs` (380 lines)

Primary module containing:

#### Data Structures

```rust
pub struct CodegenConfig {
    pub workspace_root: PathBuf,      // Detected via Cargo.toml
    pub output_dir: PathBuf,          // target/codegen/
    pub skip_ts_gen: bool,            // Optional skip flag
    pub validate: bool,               // Enable validation
    pub verbose: bool,                // Verbose output
}

pub struct CodegenStep {
    pub name: String,                 // "Build & OpenAPI Export"
    pub success: bool,                // true/false
    pub duration_ms: u128,            // Execution time
    pub message: String,              // Result message
}

pub struct CodegenReport {
    pub steps: Vec<CodegenStep>,      // All steps
    pub total_duration_ms: u128,      // Total time
}
```

#### Public API

```rust
pub async fn run() -> Result<()>
// Entry point called from main.rs
// Uses default config, no arguments

pub async fn run_with_config(config: CodegenConfig) -> Result<()>
// Full pipeline with custom config
// Returns error on any step failure
```

#### Internal Stages

```rust
fn check_dependencies(config: &CodegenConfig) -> Result<()>
// Validate Rust, Node.js, pnpm, openapi-typescript
// Early exit if critical tools missing

async fn build_server_and_export_openapi(
    config: &CodegenConfig
) -> Result<PathBuf>
// cargo build --release -p adapteros-server-api
// Invoke utoipa extraction (via script)
// Return path to openapi.json

async fn generate_typescript_types(
    config: &CodegenConfig,
    spec_path: &Path
) -> Result<PathBuf>
// pnpm exec openapi-typescript <spec> --output types.generated.ts
// Format with prettier
// Return path to generated types

async fn validate_type_consistency(
    config: &CodegenConfig,
    spec_path: &Path
) -> Result<()>
// Parse OpenAPI JSON
// Check required fields
// Count endpoints & schemas
// Validate TS exports

fn print_report(report: &CodegenReport)
// Pretty-print step results with timings
```

## Dependency Management Strategy

### Required Dependencies

#### Compile-Time (Rust)

Already in `xtask/Cargo.toml`:
- `anyhow` - Error handling
- `tokio` - Async runtime
- `serde_json` - JSON parsing
- `std::process::Command` - Shell invocation

#### Runtime Requirements

**Conditional (for full pipeline):**

| Tool | Minimum | Check Command | How to Install |
|------|---------|---------------|----------------|
| Rust 1.75+ | 1.75 | `cargo --version` | `rustup update` |
| Node.js | 18.x | `node --version` | brew/apt/nvm |
| pnpm | 8.0+ | `pnpm --version` | `npm install -g pnpm` |
| openapi-typescript | 6.0+ | grep `ui/package.json` | `cd ui && pnpm add -D openapi-typescript` |

**Detection strategy:**

1. Try to execute each tool
2. Parse version output (for Node.js)
3. Check file existence (for openapi-typescript)
4. Early exit with clear installation instructions if missing

### Dependency Ordering

```
Step 1: Dependency Check
│
├─ Rust (always required)
└─ Node.js, pnpm, openapi-ts (required for TS generation)
     └ Can skip if --skip-ts flag added

Step 2: Build & OpenAPI Export
│
└─ Requires: Rust, server crate to compile
  (Must happen before TS generation to get spec)

Step 3: TypeScript Generation
│
├─ Requires: Node.js, pnpm, openapi-typescript
├─ Requires: OpenAPI spec from Step 2
└─ Skippable: Configuration flag

Step 4: Validation
│
├─ Requires: OpenAPI spec from Step 2
├─ Requires: TS types from Step 3 (if not skipped)
└─ Non-fatal: Warnings don't fail pipeline
```

## Build Process Details

### OpenAPI Extraction via utoipa

The server crate uses **utoipa** for compile-time spec generation:

```rust
// In adapteros-server-api/src/lib.rs
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        handlers::users::get_user,
        handlers::users::create_user,
        // ... all endpoints
    ),
    components(
        schemas(
            User,
            CreateUserRequest,
            // ... all types
        )
    )
)]
pub struct ApiDoc;
```

**Extraction mechanism:**

Option 1: Helper binary (planned):
```bash
cargo run --release --bin openapi-export \
  --manifest-path crates/adapteros-server-api/Cargo.toml \
  -- --output target/codegen/openapi.json
```

Option 2: Script wrapper (current):
```bash
./scripts/generate_openapi_simple.sh
# Internally invokes build + exports spec
```

**Output validation:**

```json
{
  "openapi": "3.0.0",
  "info": {
    "title": "AdapterOS API",
    "version": "1.0.0"
  },
  "servers": [{"url": "http://localhost:8080"}],
  "paths": {
    "/v1/users": {...},
    "/v1/adapters": {...}
  },
  "components": {
    "schemas": {
      "User": {...},
      "Adapter": {...}
    }
  }
}
```

## TypeScript Generation Flow

### openapi-typescript Processing

```
openapi.json (Input)
│
├─ Parse JSON
├─ Extract schemas from components.schemas
├─ Map OpenAPI types to TS:
│  ├─ string → string
│  ├─ integer → number
│  ├─ boolean → boolean
│  ├─ array → T[]
│  ├─ object → interface { ... }
│  └─ enum → "Value1" | "Value2"
├─ Handle nullable/optional
├─ Generate interfaces with exports
│
└─ types.generated.ts (Output)

Example input schema:
{
  "User": {
    "type": "object",
    "properties": {
      "id": {"type": "string"},
      "email": {"type": "string"},
      "role": {"enum": ["admin", "user"]}
    },
    "required": ["id", "email"]
  }
}

Example output:
export interface User {
  id: string;
  email: string;
  role: "admin" | "user";
}
```

### Formatting Post-Processing

After generation, types are formatted with Prettier:

```bash
pnpm exec prettier --write ui/src/api/types.generated.ts
```

Ensures:
- Consistent indentation (2 spaces)
- Proper line breaks
- Sorted imports
- Aligned documentation

## Validation Strategy

### OpenAPI Spec Validation

1. **Structure check**: Required fields exist
   - `openapi` field matches "3.0.0"
   - `info` contains title, version
   - `paths` is non-empty object

2. **Content validation**: Meaningful content
   - Path count > 0 (at least 1 endpoint)
   - Schema count > 0 (at least 1 type)
   - All path items have operations (get, post, etc.)

3. **Reference validation**: No broken links
   - All $ref targets exist in components.schemas
   - All parameter types are valid

### TypeScript Export Validation

1. **File existence**: Generated file created
2. **Content presence**: Has `export type` or `export interface`
3. **Export count**: At least N definitions (configurable threshold)

### Non-Fatal Warnings

Validation errors are non-fatal by default:
- Pipeline continues if types exist
- Warnings printed but exit code 0
- Use `--strict` flag for strict validation (future)

## Error Handling

### Dependency Failures

```
Check dependencies
├─ Rust not found
│  └─ FATAL: Exit with "Install Rust..."
├─ Node.js not found
│  └─ FATAL: Exit with "Install Node.js 18+..."
└─ Version mismatch (Node.js < 18)
   └─ FATAL: Exit with "Upgrade Node.js to 18+..."
```

### Build Failures

```
Build server
├─ Build command fails
│  └─ FATAL: Print stderr, exit with error
└─ Build succeeds but spec not generated
   └─ FATAL: "No OpenAPI spec found"
```

### Generation Failures

```
Generate TypeScript
├─ openapi-typescript not found
│  └─ FATAL: Prompt to install via pnpm
├─ Spec file missing
│  └─ FATAL: "OpenAPI spec not found at path"
└─ Generation fails
   └─ Check if file exists anyway
     ├─ Yes: Warn and continue
     └─ No: FATAL: Exit with error
```

## Performance Characteristics

### Build Time Breakdown

| Component | Time | Notes |
|-----------|------|-------|
| Dependency check | 0.1s | Quick subprocess checks |
| Rust rebuild | 30-60s | First build, 5-15s incremental |
| OpenAPI export | <1s | JSON serialization |
| TS generation | 2-5s | Parsing & code gen |
| Prettier format | 1-2s | File I/O + formatting |
| Validation | <1s | JSON parsing |
| **Total** | 40-80s | First run; 10-20s incremental |

### Optimization Strategies

1. **Incremental builds**: Leverage cargo's caching
   - Skip rebuild if server crate unchanged
   - Only regenerate if OpenAPI spec different

2. **Parallel steps**: Some steps could run in parallel
   - Dependency check + partial spec generation
   - (Current design is sequential for simplicity)

3. **Caching**: Cache OpenAPI spec between runs
   - Skip TS generation if spec unchanged
   - Validate cache via hash comparison

## Integration Points

### Makefile Targets

```makefile
codegen:         # Full pipeline
codegen-verbose: # With VERBOSE=1
```

### xtask main.rs

```rust
Some("codegen") => codegen::run().await?,
```

### CI/CD Integration

Can be added to GitHub Actions:

```yaml
- name: Generate and validate API types
  run: make codegen

- name: Verify no uncommitted changes
  run: git diff --exit-code ui/src/api/types.generated.ts
```

### Pre-commit Hook

```bash
#!/bin/sh
cargo xtask codegen
if [ $? -ne 0 ]; then
  echo "Code generation failed"
  exit 1
fi
```

## Extension Points

### Future Enhancements

1. **Configuration file** (`.codegenrc.json`):
   ```json
   {
     "include_paths": ["/v1/**"],
     "exclude_paths": ["/internal/**"],
     "output_dir": "ui/src/api",
     "type_prefix": "API"
   }
   ```

2. **Custom transformers**:
   ```rust
   struct TransformerPlugin {
       name: String,
       transform: fn(OpenAPI) -> OpenAPI
   }
   ```

3. **Multi-output formats**:
   - GraphQL schema generation
   - Postman collection export
   - SDK generation (Python, Go, etc.)

4. **Incremental generation**:
   - Detect changed endpoints
   - Regenerate only affected types

## Testing Strategy

### Unit Tests

Located in `xtask/src/codegen.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_find_workspace_root() { ... }

    #[test]
    fn test_codegen_report_all_success() { ... }
}
```

### Integration Tests

Could be added:

```bash
# Run full pipeline
cargo xtask codegen

# Verify outputs exist
test -f target/codegen/openapi.json
test -f ui/src/api/types.generated.ts

# Type-check UI
cd ui && pnpm exec tsc --noEmit
```

### Validation Tests

Current approach:
1. JSON schema validation (OpenAPI spec)
2. File existence checks
3. Content pattern matching (export count)

## Security Considerations

1. **No network access**: All tools run locally
2. **Input validation**: JSON parsing with error handling
3. **File permissions**: Uses standard fs operations
4. **No secrets**: No credential handling required
5. **Shell injection**: Uses Command::new (safe) not shell strings

## Documentation

### User-Facing

- `docs/CODEGEN_PIPELINE.md` - Usage guide
- `Makefile` - Target help text
- `cargo xtask --help` - CLI help

### Developer-Facing

- `xtask/src/codegen.rs` - Implementation with comments
- `docs/CODEGEN_DESIGN.md` - This document
- Code structure follows standard patterns

## References

### Related Code

- `crates/adapteros-server-api/src/` - API handlers with utoipa
- `ui/src/api/types.ts` - Manual type definitions (being replaced)
- `scripts/generate_openapi_simple.sh` - Current spec generation

### External References

- utoipa: https://docs.rs/utoipa/
- OpenAPI 3.0: https://spec.openapis.org/oas/v3.0.3
- openapi-typescript: https://openapi-ts.dev/
