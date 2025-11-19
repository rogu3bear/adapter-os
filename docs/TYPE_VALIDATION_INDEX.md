# Type Validation Workflow - Complete Index

This index provides quick navigation to all type validation documentation and files.

## Quick Links

**For Developers (5 minutes):**
- [TYPE_VALIDATION_QUICK_START.md](./TYPE_VALIDATION_QUICK_START.md) - Common tasks and troubleshooting

**For Complete Reference (20 minutes):**
- [GITHUB_ACTIONS_TYPE_VALIDATION.md](./GITHUB_ACTIONS_TYPE_VALIDATION.md) - Full specification and architecture

**For Technical Details (15 minutes):**
- [TYPE_VALIDATION_IMPLEMENTATION.md](./TYPE_VALIDATION_IMPLEMENTATION.md) - Design and implementation

**For Operations (30 minutes):**
- [WORKFLOW_DEPLOYMENT_CHECKLIST.md](./WORKFLOW_DEPLOYMENT_CHECKLIST.md) - Pre/post deployment verification

## The Workflow File

**Location:** `.github/workflows/type-validation.yml`
- **Size:** 496 lines (16KB)
- **Jobs:** 9 coordinated validations
- **Execution Time:** 1-3 minutes (depending on cache)
- **Triggers:** Push to main, PRs, manual dispatch

## What Does It Validate?

### 1. Rust API Types
- Compilation without errors
- Clippy lints (treated as errors)
- Documentation generation
- Serde serialization support

### 2. OpenAPI Specification
- Generated from Rust types via `cargo xtask openapi-docs`
- Valid JSON structure
- Required fields (openapi, info, paths, schemas)
- No dangling schema references

### 3. TypeScript Types
- TypeScript compilation (`tsc --noEmit`)
- Frozen lockfile enforcement
- API types and client files exist
- Full type checking passes

### 4. API Contracts
- Contract test suite execution
- Type compatibility verification
- Deterministic test execution

### 5. Type Drift Detection
- Counts Rust types vs OpenAPI schemas
- Warns if significant drift detected
- Posts comparison on PR

## The 9 Jobs Explained

### Foundation Layer
**check-rust-types** (60-90 seconds)
- Validates all Rust API types compile
- Required before all other jobs
- Hard stop on failure

### OpenAPI Layer
**generate-openapi-spec** (90-120 seconds)
- Generates `docs/api/openapi.json`
- Uploads as artifact for inspection
- Validates structure

### TypeScript Layer
**check-typescript-types** (30-45 seconds)
- Installs UI dependencies
- Runs TypeScript compiler
- Verifies API files exist

### Parallel Validation Layer (5 jobs)
**count-schema-types** - Type drift detection
**validate-type-consistency** - Schema reference validation
**run-type-validation-tests** - Type-specific test suites
**validate-api-contracts** - API contract tests
**check-serde-serialization** - Serde validation (warnings)

### Summary Layer
**final-validation**
- Reports all job statuses
- Fails workflow if critical job failed
- Posts PR comment if all passed

## When Does It Run?

**Automatically:**
- Push to `main` with type changes
- PR to `main` with type changes

**Manually:**
- GitHub Actions tab → Type Validation → Run workflow

**Change Paths That Trigger:**
- `crates/adapteros-api-types/**`
- `crates/adapteros-server-api/**`
- `ui/src/api/**`
- `ui/src/**`
- `Cargo.toml`, `Cargo.lock`
- `ui/package.json`, `ui/pnpm-lock.yaml`
- `.github/workflows/type-validation.yml`

## How to Use

### Before Pushing Code
```bash
# Run quick validation locally
cargo check -p adapteros-api-types --all-features
cargo clippy -p adapteros-api-types -- -D warnings
cd ui && pnpm install && pnpm exec tsc --noEmit
cargo xtask openapi-docs
```

### After Creating PR
- Workflow runs automatically
- Check Actions tab for results
- Read PR comments for feedback
- Download artifacts if needed

### On Workflow Failure
1. Read error message in job logs
2. Check [TYPE_VALIDATION_QUICK_START.md](./TYPE_VALIDATION_QUICK_START.md) for fix
3. Reproduce locally using commands above
4. Push fixed code
5. Workflow runs again automatically

## Common Scenarios

### I Changed Rust Types
- Workflow validates compilation
- Generates new OpenAPI spec
- Compares with TypeScript types
- Posts drift report on PR

### I Changed TypeScript Types
- Workflow validates with tsc
- Ensures no type errors
- Verifies files exist
- No Rust validation needed

### I Changed API Endpoints
- Update Rust types first
- OpenAPI spec auto-generates
- Verify TypeScript types match
- API contracts must pass

### Workflow Failed - What Now?
1. Click job name to see error
2. Look up error type in Quick Start guide
3. Run fix command locally
4. Push again

## Documentation Organization

```
docs/
├─ TYPE_VALIDATION_INDEX.md (this file)
│  └─ Navigation and overview
├─ TYPE_VALIDATION_QUICK_START.md
│  └─ 5-minute developer guide
├─ GITHUB_ACTIONS_TYPE_VALIDATION.md
│  └─ 20-minute complete reference
├─ TYPE_VALIDATION_IMPLEMENTATION.md
│  └─ 15-minute technical details
└─ WORKFLOW_DEPLOYMENT_CHECKLIST.md
   └─ 30-minute deployment guide

.github/workflows/
└─ type-validation.yml
   └─ Production workflow (496 lines)
```

## Key Features

### Fail-Fast Strategy
- Foundation job must pass first
- Subsequent jobs only run if dependencies pass
- Minimizes total execution time
- Clear error reports

### Type Synchronization
- Rust types → OpenAPI spec (automated)
- TypeScript types must match
- Drift detection with warnings
- Contract validation

### Smart Configuration
- Triggers only on type changes
- Caching for 50-70% speedup
- Concurrency control (one per branch)
- Artifact uploads for inspection

### Developer Feedback
- PR comments with results
- Type count comparison
- Success/failure summary
- Clear actionable errors

## Performance

**Execution Times:**
- Cold run (no cache): 2-3 minutes
- Warm run (with cache): 1-2 minutes
- Parallel jobs save ~30-40% time

**Caching:**
- Rust: Based on `Cargo.lock`
- pnpm: Based on `ui/pnpm-lock.yaml`
- Auto-cleanup after 5 days

## Troubleshooting Flowchart

```
Workflow Failed?
│
├─ Job: check-rust-types
│  └─ Run: cargo clippy -p adapteros-api-types -- -D warnings
│
├─ Job: check-typescript-types
│  └─ Run: cd ui && pnpm install && pnpm exec tsc --noEmit
│
├─ Job: generate-openapi-spec
│  └─ Run: cargo xtask openapi-docs
│  └─ Check: docs/api/openapi.json exists and is valid JSON
│
├─ Job: validate-api-contracts
│  └─ Run: cargo test -p adapteros-server-api --test api_contracts
│
└─ Other jobs
   └─ See GITHUB_ACTIONS_TYPE_VALIDATION.md for detailed troubleshooting
```

## Integration with Other Workflows

**Separate from (non-blocking):**
- `ci.yml` - Format, clippy, tests
- `duplication.yml` - Code quality
- `schema-drift-detection.yml` - Schema compatibility

**Complements:**
- Faster than full test suite
- Type-specific validation
- Detailed PR feedback

**Recommended order:**
1. type-validation.yml (types)
2. ci.yml (general)
3. duplication.yml (quality)
4. schema-drift-detection.yml (compat)

## Getting Help

### Quick Issues
- Check [TYPE_VALIDATION_QUICK_START.md](./TYPE_VALIDATION_QUICK_START.md) troubleshooting section

### Detailed Questions
- See [GITHUB_ACTIONS_TYPE_VALIDATION.md](./GITHUB_ACTIONS_TYPE_VALIDATION.md) FAQ section

### Implementation Details
- Read [TYPE_VALIDATION_IMPLEMENTATION.md](./TYPE_VALIDATION_IMPLEMENTATION.md) job specs

### Deployment Problems
- Follow [WORKFLOW_DEPLOYMENT_CHECKLIST.md](./WORKFLOW_DEPLOYMENT_CHECKLIST.md)

### Reproduce Locally
All jobs have local equivalent commands:
```bash
# Foundation
cargo check -p adapteros-api-types --all-features

# OpenAPI
cargo xtask openapi-docs

# TypeScript
cd ui && pnpm install && pnpm exec tsc --noEmit

# Contracts
cargo test -p adapteros-server-api --test api_contracts
```

## Maintenance

### Regular Tasks
- Weekly: Monitor for failures
- Monthly: Check execution times
- Quarterly: Update tool versions

### Updating the Workflow
1. Edit `.github/workflows/type-validation.yml`
2. Test locally with `act` tool
3. Create PR with changes
4. Workflow validates itself
5. Merge when tests pass

### Adding New Validations
1. Create new job in workflow
2. Set dependencies correctly
3. Add to `final-validation` dependencies
4. Document in reference guide
5. Test locally

## Status and Support

**Current Status:** Production Ready ✓
**Last Updated:** 2025-11-19
**Version:** 1.0

**Maintained by:** Team
**Issues/Questions:** See troubleshooting sections above

## Related Documentation

- **Rust API Types:** `crates/adapteros-api-types/`
- **Server API:** `crates/adapteros-server-api/`
- **UI Types:** `ui/src/api/`
- **OpenAPI Spec:** `docs/api/openapi.json` (generated)

## Quick Reference Card

```
┌─────────────────────────────────────────────────────┐
│         TYPE VALIDATION WORKFLOW QUICK REF          │
├─────────────────────────────────────────────────────┤
│ Triggers: Push/PR to main, manual dispatch          │
│ Duration: 1-3 minutes (depends on cache)            │
│ Jobs: 9 coordinated validations                     │
│ Critical: 4 jobs (others are warnings)              │
│                                                     │
│ Validates:                                          │
│  ✓ Rust types compile (clippy)                      │
│  ✓ OpenAPI spec generation                          │
│  ✓ TypeScript compilation (tsc)                     │
│  ✓ API contracts                                    │
│  ✓ Type drift detection                             │
│                                                     │
│ PR Feedback: Auto-comments with results             │
│ Artifacts: OpenAPI spec + diagnostics               │
│ Cache: Rust (50% speedup) + pnpm (70% speedup)      │
│                                                     │
│ On Failure:                                         │
│ 1. Check error message                              │
│ 2. Read Quick Start troubleshooting                 │
│ 3. Run local validation commands                    │
│ 4. Fix code and push                                │
└─────────────────────────────────────────────────────┘
```

---

**Navigation:**
- [← Back to docs](.)
- [→ Quick Start Guide](./TYPE_VALIDATION_QUICK_START.md)
- [→ Complete Reference](./GITHUB_ACTIONS_TYPE_VALIDATION.md)
- [→ Implementation Details](./TYPE_VALIDATION_IMPLEMENTATION.md)
- [→ Deployment Checklist](./WORKFLOW_DEPLOYMENT_CHECKLIST.md)
