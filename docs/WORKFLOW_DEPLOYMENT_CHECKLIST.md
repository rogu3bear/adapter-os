# Type Validation Workflow Deployment Checklist

Use this checklist before committing the workflow to ensure all components are in place and functioning correctly.

## Pre-Deployment Verification

### Step 1: File Integrity Check
- [ ] `.github/workflows/type-validation.yml` exists (496 lines)
- [ ] `docs/GITHUB_ACTIONS_TYPE_VALIDATION.md` exists (reference docs)
- [ ] `docs/TYPE_VALIDATION_QUICK_START.md` exists (developer guide)
- [ ] `docs/TYPE_VALIDATION_IMPLEMENTATION.md` exists (implementation details)
- [ ] All files have correct permissions (readable)
- [ ] No syntax errors in YAML file

**Verification command:**
```bash
ls -lh .github/workflows/type-validation.yml docs/TYPE_VALIDATION* docs/GITHUB_ACTIONS_TYPE_VALIDATION.md
wc -l .github/workflows/type-validation.yml  # Should be 496
```

### Step 2: YAML Syntax Validation
- [ ] Workflow YAML is valid (no syntax errors)
- [ ] All job names are quoted strings
- [ ] All indentation is correct (2 spaces)
- [ ] All required fields present

**Verification command:**
```bash
# Using GitHub's schema validation
node -e "
const yaml = require('js-yaml');
const fs = require('fs');
try {
  yaml.load(fs.readFileSync('.github/workflows/type-validation.yml', 'utf8'));
  console.log('✓ YAML syntax is valid');
} catch(e) {
  console.error('✗ YAML error:', e.message);
  process.exit(1);
}
"
```

Or check in GitHub Actions workflow syntax validator at:
https://github.com/rhysd/actionlint

### Step 3: Configuration Review
- [ ] Trigger events configured correctly (push, pull_request, workflow_dispatch)
- [ ] Path filters include all necessary paths
- [ ] Concurrency settings configured (cancel-in-progress: true)
- [ ] Environment variables set (CARGO_TERM_COLOR, DATABASE_URL, etc.)
- [ ] All job dependencies are correct

**Checklist:**
```yaml
on:
  push:
    branches: [main]
    paths: [all API type paths covered]
  pull_request:
    branches: [main]
    paths: [same paths as push]
  workflow_dispatch: [present]

concurrency:
  group: type-validation-${{ github.ref }}
  cancel-in-progress: true [set]

env:
  CARGO_TERM_COLOR: always [set]
  RUST_BACKTRACE: 1 [set]
  DATABASE_URL: sqlite://var/aos-cp.sqlite3 [set]
```

### Step 4: Job Dependencies Validation
- [ ] Foundation job `check-rust-types` has no dependencies
- [ ] `generate-openapi-spec` depends on `check-rust-types`
- [ ] `check-typescript-types` depends on `check-rust-types`
- [ ] All downstream jobs have correct dependencies
- [ ] `final-validation` depends on all critical jobs
- [ ] Dependency graph forms valid DAG (no cycles)

**Dependency graph:**
```
check-rust-types
  ├─→ generate-openapi-spec
  │    └─→ count-schema-types
  ├─→ check-typescript-types
  │    ├─→ validate-type-consistency
  │    └─→ run-type-validation-tests
  ├─→ validate-api-contracts
  └─→ check-serde-serialization
       └─ final-validation (depends on all)
```

### Step 5: Job Step Validation
For each job, verify:

**check-rust-types:**
- [ ] Checkout step present
- [ ] Rust toolchain installation uses dtolnay/rust-toolchain@stable
- [ ] Cache configuration present
- [ ] `cargo check -p adapteros-api-types` present
- [ ] `cargo clippy` present with `-D warnings`
- [ ] `cargo doc` present

**generate-openapi-spec:**
- [ ] Checkout, Rust, cache steps present
- [ ] `cargo build -p adapteros-server --release` present
- [ ] `cargo xtask openapi-docs` present
- [ ] File existence verification present
- [ ] OpenAPI structure validation with Node.js present
- [ ] Artifact upload present (docs/api/openapi.json)
- [ ] 30-day retention configured

**check-typescript-types:**
- [ ] Node.js 20 setup
- [ ] pnpm 9 setup
- [ ] pnpm cache configuration
- [ ] `pnpm install --frozen-lockfile` present
- [ ] `tsc --noEmit` present
- [ ] API files verification present
- [ ] Artifact upload on failure present

**Additional jobs:**
- [ ] All have appropriate dependencies
- [ ] All have appropriate `continue-on-error` settings
- [ ] Critical jobs fail workflow (no continue-on-error)
- [ ] Non-critical jobs continue on error

### Step 6: Error Handling Review
- [ ] All critical paths have `continue-on-error: false`
- [ ] Non-critical validation has appropriate error handling
- [ ] `final-validation` job has proper exit logic
- [ ] PR comment jobs have `continue-on-error: true`

### Step 7: Artifact Configuration
- [ ] OpenAPI spec artifact configured (30 days)
- [ ] TypeScript diagnostics artifact on failure (7 days)
- [ ] Artifact paths correct
- [ ] No sensitive files in artifacts

**Verify:**
```yaml
# openapi-spec artifact
- name: Upload OpenAPI spec as artifact
  uses: actions/upload-artifact@v4
  with:
    name: openapi-spec
    path: docs/api/openapi.json
    retention-days: 30

# typescript-diagnostics artifact
- name: Upload TypeScript diagnostics
  if: failure()
  uses: actions/upload-artifact@v4
  with:
    name: typescript-diagnostics
    path: ui/
    retention-days: 7
```

### Step 8: PR Comment Configuration
- [ ] Type comparison comment configured (count-schema-types)
- [ ] Success summary comment configured (final-validation)
- [ ] Comment uses `actions/github-script@v7`
- [ ] Proper error handling with `continue-on-error: true`

### Step 9: Cache Configuration
**Rust Cache:**
- [ ] Uses `Swatinem/rust-cache@v2`
- [ ] `cache-on-failure: true` set
- [ ] Keys are correct (Cargo.lock)

**pnpm Cache:**
- [ ] Uses `actions/cache@v4`
- [ ] Cache path correct (~/.pnpm-store)
- [ ] Key includes ui/pnpm-lock.yaml
- [ ] Restore keys configured

### Step 10: Action Versions
- [ ] `actions/checkout@v4` (latest)
- [ ] `dtolnay/rust-toolchain@stable` (pinned to stable)
- [ ] `Swatinem/rust-cache@v2` (latest)
- [ ] `actions/setup-node@v4` (latest)
- [ ] `pnpm/action-setup@v2` (latest)
- [ ] `actions/cache@v4` (latest)
- [ ] `actions/upload-artifact@v4` (latest)
- [ ] `actions/download-artifact@v4` (latest)
- [ ] `actions/github-script@v7` (latest)

## Local Testing

### Before Pushing

Run the following commands locally to verify workflow steps work:

#### 1. Check Rust Types
```bash
# [ ] Should pass without errors
cargo check -p adapteros-api-types --all-features
cargo clippy -p adapteros-api-types -- -D warnings
cargo doc -p adapteros-api-types --no-deps
```

Expected: All pass without warnings

#### 2. Generate OpenAPI Spec
```bash
# [ ] Should generate docs/api/openapi.json
cargo build -p adapteros-server --release
cargo xtask openapi-docs
test -f docs/api/openapi.json && echo "✓ OpenAPI spec generated"
```

Expected: File generated, valid JSON

#### 3. Validate OpenAPI Structure
```bash
# [ ] Should validate structure
node -e "
const fs = require('fs');
const spec = JSON.parse(fs.readFileSync('docs/api/openapi.json', 'utf8'));
console.log('✓ Valid JSON');
console.log('  OpenAPI version:', spec.openapi);
console.log('  Endpoints:', Object.keys(spec.paths).length);
console.log('  Schemas:', Object.keys(spec.components.schemas).length);
"
```

Expected: Valid structure with counts

#### 4. Check TypeScript Types
```bash
# [ ] Should pass without errors
cd ui
pnpm install --frozen-lockfile
pnpm exec tsc --noEmit
test -f src/api/types.ts && echo "✓ Types file exists"
test -f src/api/client.ts && echo "✓ Client file exists"
```

Expected: No TypeScript errors

#### 5. Run API Contract Tests
```bash
# [ ] Should pass all tests
cargo test -p adapteros-server-api --test api_contracts -- --nocapture --test-threads=1
```

Expected: All tests pass

### GitHub Actions Act Tool (Optional)

If you have `act` installed (brew install act):

```bash
# Run single job
act -j check-rust-types

# Run all jobs
act
```

## Documentation Verification

### Step 11: Documentation Completeness
- [ ] Quick Start guide covers common scenarios
- [ ] Reference docs include all job descriptions
- [ ] Implementation guide explains architecture
- [ ] All docs are in `docs/` directory
- [ ] Docs are referenced in workflow comments

**Verify:**
```bash
grep -l "check-rust-types\|generate-openapi" docs/TYPE_VALIDATION*.md docs/GITHUB_ACTIONS*.md
# Should find at least 2 files
```

### Step 12: Documentation Cross-References
- [ ] Quick Start references full docs
- [ ] Full docs reference implementation guide
- [ ] All docs reference workflow file
- [ ] Code snippets match actual commands

## Git Integration

### Step 13: Pre-Commit Checks
```bash
# Verify no unstaged changes
git status

# Stage only the workflow files
git add .github/workflows/type-validation.yml
git add docs/GITHUB_ACTIONS_TYPE_VALIDATION.md
git add docs/TYPE_VALIDATION_QUICK_START.md
git add docs/TYPE_VALIDATION_IMPLEMENTATION.md
git add docs/WORKFLOW_DEPLOYMENT_CHECKLIST.md

# Verify staged changes
git diff --cached --stat

# [ ] Only expected files in staging area
# [ ] No unexpected files added
```

### Step 14: Commit Message
```bash
# Craft appropriate commit message
git commit -m "feat: add comprehensive type validation GitHub Actions workflow

- Add type-validation.yml with 9 coordinated jobs
- Validates Rust API types compilation
- Generates and validates OpenAPI spec
- Validates TypeScript types (tsc --noEmit)
- Validates API contracts
- Detects type drift between languages
- Fail-fast strategy with smart dependencies
- Provides PR feedback and artifact uploads
- Includes comprehensive documentation"
```

- [ ] Commit message explains purpose
- [ ] Commit message lists key features
- [ ] Uses conventional commit format

### Step 15: Push and Test
```bash
# Push to feature branch
git push origin feature/type-validation-workflow

# [ ] Create PR to main
# [ ] Workflow runs automatically
# [ ] All jobs pass
# [ ] PR gets comments
```

## Post-Deployment Verification

### Step 16: First Run Verification
- [ ] PR created with workflow file
- [ ] Workflow appears in Actions tab
- [ ] All 9 jobs are visible
- [ ] Jobs execute in correct order
- [ ] Job colors: green (success), red (failure)
- [ ] Total execution time reasonable (1-3 min)

### Step 17: Artifact Verification
- [ ] OpenAPI spec artifact created
- [ ] Artifact is downloadable
- [ ] Artifact contains valid JSON
- [ ] Retention policy respected

### Step 18: PR Feedback Verification
- [ ] Type comparison comment posted
- [ ] Comment shows type counts
- [ ] Success summary comment (if all pass)
- [ ] Comments are helpful and clear

### Step 19: Integration Verification
- [ ] Existing workflows not blocked
- [ ] No conflicts with ci.yml
- [ ] No conflicts with duplication.yml
- [ ] Workflow runs independently

## Rollout Plan

### Phase 1: Feature Branch Testing (Current)
- [ ] All verification steps above completed
- [ ] Local tests passing
- [ ] Documentation reviewed

### Phase 2: PR Review
- [ ] Code review of workflow
- [ ] Documentation review
- [ ] Team feedback collected
- [ ] Any adjustments made

### Phase 3: Merge to Main
- [ ] PR approved
- [ ] All checks passing
- [ ] Workflow merged to main
- [ ] Verify workflow file is accessible

### Phase 4: Team Communication
- [ ] Share quick start guide with team
- [ ] Explain when workflow runs
- [ ] Provide troubleshooting contacts
- [ ] Monitor for issues

## Troubleshooting Guide Reference

If any step fails:

1. **Check logs:** `gh run view <run-id> --log`
2. **Reproduce locally:** Use commands in "Local Testing" section
3. **Review documentation:** Check docs/GITHUB_ACTIONS_TYPE_VALIDATION.md
4. **Check configuration:** Verify job definitions match expected structure
5. **Ask for help:** Reference this checklist and provide error details

## Success Criteria

Workflow is ready for production when:

- [ ] All 14 pre-deployment verification steps complete
- [ ] All 10 local testing commands pass
- [ ] All 6 documentation sections verified
- [ ] Git integration steps complete
- [ ] First run on actual PR successful
- [ ] Team has access to documentation
- [ ] No known issues or blocking concerns

## Maintenance Schedule

After deployment:

- [ ] Weekly: Monitor workflow runs (no unexplained failures)
- [ ] Monthly: Review performance metrics (execution time)
- [ ] Quarterly: Check for tool version updates
- [ ] As needed: Add new validations or adjust configuration

## Sign-Off

When all checks are complete:

```
Workflow: Type Validation
Status: Ready for Production
Date: [YYYY-MM-DD]
Verified by: [Name]
Sign-off: [Signature/Confirmation]
```

---

**This checklist ensures a reliable, maintainable type validation workflow that serves the team effectively.**
