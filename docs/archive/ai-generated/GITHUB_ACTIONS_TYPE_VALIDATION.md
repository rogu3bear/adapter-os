# GitHub Actions Type Validation Workflow

## Overview

The **Type Validation Workflow** (`.github/workflows/type-validation.yml`) is a comprehensive CI/CD pipeline that ensures type safety across the AdapterOS codebase. It validates Rust API types, TypeScript types, OpenAPI schemas, and API contracts in a coordinated, fail-fast pipeline.

**Trigger Events:**
- Push to `main` branch
- Pull requests to `main` branch
- Manual trigger via `workflow_dispatch`

**Trigger Paths:**
- Changes to `crates/adapteros-api-types/**`
- Changes to `crates/adapteros-server-api/**`
- Changes to `ui/src/api/**` or `ui/src/**`
- Changes to `Cargo.toml`, `Cargo.lock`, `ui/package.json`, `ui/pnpm-lock.yaml`
- Changes to workflow file itself

## Job Architecture

The workflow uses a **dependency-driven job pipeline** with strategic parallelization:

```
┌─────────────────────┐
│ check-rust-types    │  ← Foundation job (must pass first)
└──────────┬──────────┘
           │
      ┌────┴─────────────────────────────────────┐
      │                                           │
┌─────▼──────────────────┐            ┌──────────▼──────────────┐
│ generate-openapi-spec  │            │ check-typescript-types  │
└─────┬──────────────────┘            └──────────┬──────────────┘
      │                                           │
      │                ┌──────────────────────────┘
      │                │
      ├────────────────┼──────────────────────────┐
      │                │                          │
┌─────▼─────────────────┴──────┐   ┌─────────────▼──────────────┐
│ count-schema-types            │   │ validate-type-consistency  │
└──────────────────────────────┘   └────────────────────────────┘
      │
      ├─────────────────────────────────────────┐
      │                                         │
┌─────▼───────────────────────┐   ┌────────────▼─────────────────┐
│ run-type-validation-tests    │   │ validate-api-contracts      │
└──────────────────────────────┘   └────────────────────────────┘
      │
      └────────────┬──────────────────┬────────────┬──────────────┐
                   │                  │            │              │
             ┌─────▼──────────┐ ┌─────▼──┐ ┌────┴──────┐ ┌────────▼──┐
             │ check-serde    │ │ parallel││ validation│ │ final-    │
             │ serialization  │ └────────┘ │ cleanup   │ │ validation│
             └────────────────┘             └──────────┘ └───────────┘
```

## Job Descriptions

### 1. `check-rust-types` (Foundation)
**Purpose:** Validate Rust API type definitions compile correctly.

**Steps:**
1. Checkout repository
2. Install Rust toolchain (stable)
3. Cache build artifacts
4. Check `adapteros-api-types` compilation with all features
5. Run clippy on `adapteros-api-types` with `-D warnings`
6. Build documentation (validates doc comments)

**Fails on:**
- Compilation errors in `adapteros-api-types`
- Clippy warnings treated as errors
- Missing/invalid doc comments

**Artifacts:** None (caching only)

### 2. `generate-openapi-spec` (Depends: check-rust-types)
**Purpose:** Generate and validate OpenAPI specification from Rust types.

**Steps:**
1. Checkout repository
2. Install Rust toolchain
3. Cache build artifacts
4. Build entire server for OpenAPI generation
5. Run `cargo xtask openapi-docs` to generate spec
6. Verify spec exists at `docs/api/openapi.json`
7. Validate OpenAPI structure:
   - Must have `openapi` version field
   - Must have `info` section
   - Must have `paths` section
   - Must have `components.schemas` section
8. Output summary (version, title, endpoints count, schemas count)

**Fails on:**
- Server build failure
- OpenAPI generation failure
- Missing spec file
- Invalid OpenAPI structure

**Artifacts:** Uploaded `openapi-spec` (artifact: `docs/api/openapi.json`)

### 3. `check-typescript-types` (Depends: check-rust-types)
**Purpose:** Validate TypeScript types and run TypeScript compiler.

**Steps:**
1. Checkout repository
2. Setup Node.js 20
3. Setup pnpm 9
4. Cache pnpm dependencies
5. Install UI dependencies: `pnpm install --frozen-lockfile`
6. Run TypeScript compiler: `tsc --noEmit`
7. Verify `ui/src/api/types.ts` exists and count type definitions
8. Verify `ui/src/api/client.ts` exists
9. Upload diagnostics on failure

**Fails on:**
- pnpm install failure
- TypeScript compilation errors
- Missing API types or client files
- Type checking errors

**Artifacts:** Uploaded on failure (TypeScript diagnostics)

### 4. `count-schema-types` (Depends: generate-openapi-spec)
**Purpose:** Compare Rust type counts with OpenAPI schema counts.

**Steps:**
1. Download OpenAPI spec from previous job
2. Count Rust types: `grep "^pub struct|^pub enum"` in `crates/adapteros-api-types/src/**`
3. Count OpenAPI schemas from generated spec
4. Generate comparison report (Markdown)
5. Check for significant drift (Rust > 2× OpenAPI)
6. Comment report on PR if pull request event

**Warnings:**
- If OpenAPI schemas < Rust types ÷ 2
- Suggests adding missing type exports

**Outputs:**
- `rust_types.count` - Total Rust type definitions
- `openapi_schemas.count` - Total OpenAPI schemas
- PR comment with comparison report

### 5. `validate-type-consistency` (Depends: check-typescript-types, generate-openapi-spec)
**Purpose:** Validate naming consistency and schema references.

**Steps:**
1. Download OpenAPI spec
2. Setup Node.js 20
3. Check for critical type names:
   - `ApiResponse`
   - `ErrorResponse`
   - `PaginatedResponse`
   - `HealthResponse`
4. Validate all OpenAPI `$ref` pointers are valid
5. Check references use `#/components/schemas/` convention

**Fails on:**
- Broken schema references (e.g., dangling `$ref`)
- Invalid reference paths

**Report:** Lists any broken references with their locations

### 6. `run-type-validation-tests` (Depends: check-rust-types, check-typescript-types)
**Purpose:** Run type-specific test suites.

**Steps:**
1. Install Rust and Node.js
2. Setup pnpm
3. Install UI dependencies
4. Run Rust API type tests: `cargo test -p adapteros-api-types --lib`
5. Run serialization tests (warns if not found)
6. Run TypeScript API tests (optional: `pnpm test src/api/`)

**Continues on error:** TypeScript tests continue on error (optional)

### 7. `validate-api-contracts` (Depends: check-rust-types, check-typescript-types)
**Purpose:** Run API contract validation tests.

**Steps:**
1. Install Rust
2. Run API contract tests: `cargo test -p adapteros-server-api --test api_contracts`
3. Uses single-threaded execution for determinism
4. Captures output with `--nocapture`

**Fails on:** Contract test failures

### 8. `check-serde-serialization` (Depends: check-rust-types)
**Purpose:** Validate Serde serialization/deserialization support.

**Steps:**
1. Check for serde-related warnings: `cargo check -p adapteros-api-types`
2. Attempt roundtrip tests (optional)
3. Outputs serde issues if found

**Warnings Only:** Does not fail workflow

### 9. `final-validation` (Depends: all critical jobs, Conditional: always)
**Purpose:** Summarize validation results and determine final status.

**Steps:**
1. Check all job statuses
2. Print summary table
3. Exit with error if any critical job failed:
   - `check-rust-types`
   - `check-typescript-types`
   - `generate-openapi-spec`
   - `validate-api-contracts`
4. Comment success on PR if all passed

**Exit Strategy:** Fails workflow if any critical job failed

## Trigger Conditions

### File Path Matching
The workflow only runs on commits that touch:

```yaml
paths:
  - 'crates/adapteros-api-types/**'      # Rust API types
  - 'crates/adapteros-server-api/**'     # Server API handlers
  - 'ui/src/api/**'                      # TypeScript API client/types
  - 'ui/src/**'                          # UI components (type safety)
  - 'Cargo.toml'                         # Dependencies
  - 'Cargo.lock'                         # Lock file changes
  - 'ui/package.json'                    # NPM/pnpm deps
  - 'ui/pnpm-lock.yaml'                  # Lock file changes
  - '.github/workflows/type-validation.yml'  # Workflow itself
```

### Event Triggers

| Event | Branches | Behavior |
|-------|----------|----------|
| `push` | `main` | Runs on all matching file changes |
| `pull_request` | `main` | Runs on PR creation, updates, reopens |
| `workflow_dispatch` | Manual | Allows manual trigger from Actions tab |

### Concurrency Control

```yaml
concurrency:
  group: type-validation-${{ github.ref }}
  cancel-in-progress: true
```

- **Group by ref:** Same branch/PR can only run one workflow at a time
- **Cancel in progress:** New push cancels previous workflow on same ref

## Environment Variables

```yaml
env:
  CARGO_TERM_COLOR: always          # Colored cargo output
  RUST_BACKTRACE: 1                 # Basic backtrace on panic
  DATABASE_URL: sqlite://var/aos-cp.sqlite3  # For compilation only
```

## Dependencies and Caching

### Rust Toolchain
- Uses `dtolnay/rust-toolchain@stable` (recommended best practices)
- Caches via `Swatinem/rust-cache@v2`
- Cache invalidates on `Cargo.lock` changes

### Node.js and pnpm
- Uses Node.js 20 (current LTS)
- Uses pnpm 9 (per project requirements)
- Caches via `actions/cache@v4`
- Cache key based on `ui/pnpm-lock.yaml`

## Fast-Fail Strategy

The workflow implements **aggressive fail-fast to prevent wasting resources:**

1. **Foundation jobs first:** `check-rust-types` is required before downstream jobs
2. **Parallel validation:** Independent checks run in parallel after foundation
3. **Stop on critical failures:**
   - Rust type compilation → stops `generate-openapi-spec`, OpenAPI jobs
   - TypeScript compilation → stops all TypeScript-dependent jobs
   - OpenAPI generation → stops schema validation jobs
4. **Continue-on-error for optional jobs:**
   - Serialization tests (warnings only)
   - TypeScript API tests (if not found)
5. **Final validation:** Catches any failures and provides clear summary

## Artifacts

### Uploaded Artifacts

| Artifact Name | Path | Retention | When |
|---------------|------|-----------|------|
| `openapi-spec` | `docs/api/openapi.json` | 30 days | Always |
| `typescript-diagnostics` | `ui/` | 7 days | On TypeScript failure |

### Usage
- Download artifacts from workflow run details
- Use in local debugging: `gh run download <run-id> -n openapi-spec`

## PR Feedback

The workflow provides automated feedback on PRs:

1. **Type comparison comment:** `count-schema-types` job posts type counts
2. **Success summary comment:** `final-validation` job posts success summary
3. **All continue-on-error:** Comments don't block PR approval

## Running Locally

### Full Validation (before pushing)
```bash
# Rust types
cargo check -p adapteros-api-types --all-features
cargo clippy -p adapteros-api-types -- -D warnings
cargo test -p adapteros-api-types --lib

# OpenAPI spec
cargo build -p adapteros-server --release
cargo xtask openapi-docs

# TypeScript types
cd ui && pnpm install
pnpm exec tsc --noEmit

# API contracts
cargo test -p adapteros-server-api --test api_contracts -- --nocapture --test-threads=1
```

### Individual Components
```bash
# Just Rust types
make clippy
cargo test -p adapteros-api-types

# Just TypeScript
cd ui && pnpm exec tsc --noEmit

# Just OpenAPI
cargo xtask openapi-docs

# Just contracts
cargo test -p adapteros-server-api --test api_contracts
```

## Troubleshooting

### Workflow Fails: "check-rust-types"
**Symptom:** Clippy warnings or compilation errors in `adapteros-api-types`

**Resolution:**
```bash
cargo clippy -p adapteros-api-types --all-features -- -D warnings
cargo fix --allow-dirty -p adapteros-api-types
cargo fmt -p adapteros-api-types
```

### Workflow Fails: "generate-openapi-spec"
**Symptom:** OpenAPI spec generation fails

**Resolution:**
```bash
# Check if server builds
cargo build -p adapteros-server --release

# Manually generate spec
cargo xtask openapi-docs

# Check spec validity
jq . docs/api/openapi.json | head -50
```

### Workflow Fails: "check-typescript-types"
**Symptom:** TypeScript compilation errors

**Resolution:**
```bash
cd ui
pnpm install --frozen-lockfile
pnpm exec tsc --noEmit
pnpm exec tsc --noEmit --pretty=false  # For CI-friendly output
```

### Workflow Fails: "validate-api-contracts"
**Symptom:** API contract test failures

**Resolution:**
```bash
cargo test -p adapteros-server-api --test api_contracts -- --nocapture --test-threads=1
# Check test output for specific contract violations
```

### Artifacts Not Downloaded
**Symptom:** Can't download OpenAPI spec artifact

**Resolution:**
```bash
# List workflow artifacts
gh run view <run-id> --json artifacts

# Download specific artifact
gh run download <run-id> -n openapi-spec
```

## Performance Metrics

**Expected execution times** (successful run):
- `check-rust-types`: 60-90s
- `check-typescript-types`: 30-45s (includes pnpm install)
- `generate-openapi-spec`: 90-120s
- `count-schema-types`: 5-10s
- `validate-type-consistency`: 5-10s
- `run-type-validation-tests`: 30-60s
- `validate-api-contracts`: 20-40s
- `check-serde-serialization`: 10-20s
- **Total (parallel)**: 120-160s (2-3 minutes)

**With cache hits:**
- Rust cache: ~50% time savings
- pnpm cache: ~70% time savings
- Combined typical: 60-90s total

## Maintenance

### Updating the Workflow

**Add new validation:**
1. Create new job
2. Set `needs: [check-rust-types]` at minimum
3. Add to `final-validation` dependencies
4. Document in this file

**Update tool versions:**
1. Edit action versions (e.g., `actions/setup-node@v4`)
2. Edit tool versions (e.g., `node-version: '20'`)
3. Update documentation
4. Test locally before committing

**Debug individual jobs:**
```bash
# Download workflow logs
gh run view <run-id> --log

# Grep for specific job
gh run view <run-id> --log | grep -A 50 "check-rust-types"
```

## Integration with Other Workflows

This workflow runs **independently** of other CI workflows:
- Does not block `ci.yml` (format, clippy, tests)
- Does not block `duplication.yml` (code duplication checks)
- Does not block `schema-drift-detection.yml` (schema validation)

**Recommended execution order:**
1. `type-validation.yml` (fail-fast, types first)
2. `ci.yml` (broader testing)
3. `duplication.yml` (code quality)
4. `schema-drift-detection.yml` (compatibility checks)

## FAQ

**Q: Why separate from main CI workflow?**
A: Type validation has different triggers (API changes) and provides detailed feedback without running full test suite.

**Q: Can I disable specific jobs?**
A: Yes, but not recommended. Individual jobs can be toggled by removing from `final-validation` needs.

**Q: What if I only changed TypeScript?**
A: Rust jobs are skipped if only `ui/src/**` changed (except `ui/src/api/`).

**Q: Can I manually trigger for a specific branch?**
A: Yes, use `workflow_dispatch` from Actions tab. It runs on current branch.

**Q: How do I test workflow changes locally?**
A: Use `act` (GitHub Actions local runner):
```bash
brew install act
act -j check-rust-types  # Test single job
act -l                   # List all jobs
```

## References

- **OpenAPI Spec:** https://spec.openapis.org/
- **Utoipa (Rust OpenAPI):** https://github.com/juhaku/utoipa
- **TypeScript Handbook:** https://www.typescriptlang.org/docs/handbook/
- **GitHub Actions Docs:** https://docs.github.com/en/actions
