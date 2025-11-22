# Type Validation Quick Start Guide

## What Does It Do?

The type validation workflow ensures all types across Rust, TypeScript, and OpenAPI specs stay synchronized and compile correctly. It runs automatically on PR and push to `main`.

## When It Runs

Automatically when you:
- Push to `main` branch
- Open/update PR to `main` branch
- Change files in:
  - `crates/adapteros-api-types/**`
  - `crates/adapteros-server-api/**`
  - `ui/src/api/**`
  - `ui/src/**`
  - `Cargo.toml`, `Cargo.lock`
  - `ui/package.json`, `ui/pnpm-lock.yaml`

## Key Jobs (In Order)

1. **check-rust-types** ← Foundation (must pass)
   - Compiles `adapteros-api-types`
   - Runs clippy
   - Generates docs

2. **generate-openapi-spec** (depends on #1)
   - Generates OpenAPI spec from Rust types
   - Uploads `docs/api/openapi.json`

3. **check-typescript-types** (depends on #1)
   - Compiles TypeScript with `tsc --noEmit`
   - Validates `ui/src/api/` files

4. **Parallel Validation** (depends on above)
   - Counts types in each language
   - Validates schema references
   - Runs type-specific tests
   - Validates API contracts

5. **final-validation** (summary)
   - Reports success/failure
   - Comments on PR

## What Can Go Wrong

### Rust Types Don't Compile
```
✗ check-rust-types FAILED
```
**Fix:**
```bash
cargo clippy -p adapteros-api-types -- -D warnings
cargo fix --allow-dirty -p adapteros-api-types
```

### TypeScript Types Don't Compile
```
✗ check-typescript-types FAILED
```
**Fix:**
```bash
cd ui
pnpm install --frozen-lockfile
pnpm exec tsc --noEmit
```

### OpenAPI Spec Generation Fails
```
✗ generate-openapi-spec FAILED
```
**Fix:**
```bash
cargo xtask openapi-docs
# Check docs/api/openapi.json exists and is valid
jq . docs/api/openapi.json | head
```

### API Contracts Fail
```
✗ validate-api-contracts FAILED
```
**Fix:**
```bash
cargo test -p adapteros-server-api --test api_contracts -- --nocapture
# Read error message and update types/handlers
```

## Before Pushing

Run this to catch issues locally:

```bash
# Check Rust types
cargo check -p adapteros-api-types --all-features
cargo clippy -p adapteros-api-types -- -D warnings

# Check TypeScript
cd ui && pnpm install && pnpm exec tsc --noEmit

# Check OpenAPI spec
cargo xtask openapi-docs

# Check API contracts
cargo test -p adapteros-server-api --test api_contracts
```

Or use the Makefile:
```bash
make check  # Runs format, clippy, tests (partial)
```

## Common Changes

### Adding a New Rust Type

1. Add to `crates/adapteros-api-types/src/your_module.rs`:
```rust
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct MyType {
    pub field: String,
}
```

2. Export in `crates/adapteros-api-types/src/lib.rs`:
```rust
pub mod your_module;
pub use your_module::*;
```

3. OpenAPI spec auto-generates (run `cargo xtask openapi-docs`)

4. Generate TypeScript types (workflow handles this)

5. Push and let workflow validate:
   - Rust compiles ✓
   - OpenAPI spec generated ✓
   - TypeScript types updated ✓
   - API contracts validated ✓

### Adding a New TypeScript Type

1. Add to `ui/src/api/types.ts`:
```typescript
export interface MyType {
  field: string;
}
```

2. Use in `ui/src/api/client.ts`

3. TypeScript compiler validates (workflow handles)

4. If you add new API endpoint, update Rust handlers first (they generate OpenAPI)

### Changing an Existing Type

1. Update Rust type in `crates/adapteros-api-types/src/`
2. Update TypeScript type in `ui/src/api/types.ts`
3. Update handlers/API code to use new type
4. Run locally:
   ```bash
   cargo xtask openapi-docs  # Regenerate spec
   cd ui && pnpm exec tsc --noEmit  # Check TS
   ```
5. Push - workflow validates synchronization

## Checking Workflow Status

View on GitHub:
1. Go to repo Actions tab
2. Find "Type Validation" workflow
3. Click latest run
4. See which jobs passed/failed

Or via CLI:
```bash
gh run list --workflow=type-validation.yml
gh run view <run-id> --log  # Full logs
gh run view <run-id>        # Summary
```

Download artifacts:
```bash
gh run download <run-id> -n openapi-spec
cat docs/api/openapi.json | jq .
```

## PR Feedback

The workflow automatically comments on your PR:

1. **Type comparison:** Shows Rust type count vs OpenAPI schema count
2. **Success message:** All checks passed ✓

No manual steps needed - just review the comments.

## Manual Trigger

You can manually trigger the workflow from GitHub Actions tab:
1. Go to Actions → Type Validation
2. Click "Run workflow"
3. Select branch
4. Click "Run workflow"

Useful for:
- Testing after manual fixes
- Checking without pushing code

## Performance

**Expected times:**
- Cold run (no cache): 2-3 minutes
- Warm run (with cache): 1-2 minutes

**Speed up:**
- Don't change `Cargo.lock` unless needed
- Don't change `ui/pnpm-lock.yaml` unless needed
- Keep `crates/adapteros-api-types` compiling (foundation job)

## Next Steps

1. Review full docs: [`docs/GITHUB_ACTIONS_TYPE_VALIDATION.md`](./GITHUB_ACTIONS_TYPE_VALIDATION.md)
2. See workflow file: [`.github/workflows/type-validation.yml`](../.github/workflows/type-validation.yml)
3. Check job dependencies diagram in full docs

## Troubleshooting Checklist

- [ ] Run `cargo clippy -p adapteros-api-types -- -D warnings` locally
- [ ] Run `cd ui && pnpm install && pnpm exec tsc --noEmit` locally
- [ ] Run `cargo xtask openapi-docs` and verify `docs/api/openapi.json`
- [ ] Check latest workflow run logs: `gh run list --workflow=type-validation.yml`
- [ ] Download spec artifact and validate: `jq . docs/api/openapi.json`
- [ ] Ensure all imports exist in `crates/adapteros-api-types/src/lib.rs`
- [ ] Verify TypeScript types in `ui/src/api/types.ts` match Rust types

## Support

If workflow consistently fails:
1. Check error message in job logs
2. Reproduce locally using commands above
3. Fix locally and push again
4. Workflow automatically retries

The workflow is designed to fail **fast and clearly** so you can fix issues quickly.
