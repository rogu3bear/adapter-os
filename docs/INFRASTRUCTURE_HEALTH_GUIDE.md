# Infrastructure Health Guide

## Overview

This guide documents the prevention measures implemented to avoid the infrastructure issues that required extensive rectification in the AdapterOS codebase.

## Background

The recent rectification effort restored:
- 601 tokio tests from complete failure
- Core compilation stability
- Proper dependency management
- Async code determinism

## Prevention Framework

### 1. Automated Health Checks

Run the infrastructure health check before commits:

```bash
make infra-check
# or
./scripts/prevent_infrastructure_issues.sh
```

**Checks Performed:**
- ✅ Tokio configuration validation (macros, rt-multi-thread features)
- ✅ Workspace member consistency
- ✅ Dependency chain validation
- ✅ Compilation verification
- ✅ Basic test functionality

### 2. CI/CD Integration

The infrastructure health check runs automatically on:
- All pushes to main/develop
- All pull requests
- Daily at 6 AM UTC (early detection)

### 3. Local Development Workflow

**Before Committing:**
```bash
# Run infrastructure check
make infra-check

# Run tests
make test

# Build verification
make build
```

## Common Issues Prevented

### Tokio Configuration Issues

**Problem:** Missing tokio features cause `#[tokio::test]` failures
**Prevention:** Automated check ensures all crates have required tokio features
**Required Features:** `macros`, `rt-multi-thread`, `sync`, `time`

### Dependency Chain Breaks

**Problem:** Workspace member removal breaks imports
**Prevention:** Validates that imported crates are in workspace members
**Check:** Scans all `use crate_name::` imports against workspace configuration

### Compilation Failures

**Problem:** Silent compilation errors in experimental crates
**Prevention:** Workspace-wide compilation check
**Scope:** All workspace members except explicitly excluded crates

## Development Guidelines

### When Adding Dependencies

1. **Check Workspace Membership:** Ensure target crate is in `Cargo.toml` workspace members
2. **Run Health Check:** `make infra-check` after adding dependencies
3. **Test Compilation:** Verify `cargo check --workspace` passes

### When Modifying Experimental Crates

1. **Isolate Changes:** Work in feature branches for experimental code
2. **Regular Health Checks:** Run `make infra-check` frequently
3. **Dependency Awareness:** Note that experimental crates may be excluded from CI

### When Adding Tokio Tests

1. **Feature Requirements:** Ensure crate has tokio with `macros` and `rt-multi-thread`
2. **Import Validation:** Verify `use tokio::test` works in tests
3. **Async Best Practices:** Follow established async patterns in codebase

## Troubleshooting

### Health Check Failures

**Tokio Configuration Error:**
```
❌ crate_name: Missing tokio macros feature
```
**Fix:** Add `features = ["macros", "rt-multi-thread"]` to tokio dependency

**Workspace Member Error:**
```
❌ Missing from workspace members: crate_name
```
**Fix:** Add `"crates/crate_name"` to `members` array in root Cargo.toml

**Dependency Error:**
```
❌ Imports from adapteros_types but missing dependency
```
**Fix:** Add `adapteros-types = { path = "../adapteros-types" }` to crate's Cargo.toml

### Emergency Bypass

If health checks block legitimate work:
```bash
# Document the override reason
echo "Override reason: [explanation]" > .health-check-override

# Run check with bypass
OVERRIDE_HEALTH_CHECK=1 make infra-check
```

## Integration with Existing Workflows

### Git Hooks (Recommended)

Add to `.git/hooks/pre-commit`:
```bash
#!/bin/bash
make infra-check
```

### IDE Integration

Configure your IDE to run `make infra-check` on file saves in Cargo.toml files.

### Team Workflow

- **Daily:** Run `make infra-check` before starting work
- **Pre-commit:** Always run infrastructure health check
- **CI:** Automated checks prevent broken commits

## Success Metrics

**Infrastructure Health Indicators:**
- ✅ `make infra-check` passes without overrides
- ✅ `cargo check --workspace` completes successfully
- ✅ Core test suites run without tokio errors
- ✅ No compilation failures in workspace members

**Prevention Effectiveness:**
- Zero tokio configuration regressions
- Zero dependency chain breaks
- Early detection of infrastructure issues
- Stable development velocity

## Maintenance

**Monthly Review:**
- Update health check script for new issue patterns
- Review CI failure patterns for new prevention needs
- Audit experimental crate exclusions

**Script Updates:**
- Add new validation checks as issues are discovered
- Update CI workflows to match local development checks

## Emergency Contacts

If infrastructure issues occur despite prevention measures:
1. Run `make infra-check` with verbose output
2. Check recent Cargo.toml changes
3. Review workspace member consistency
4. Contact infrastructure maintainers

---

**This prevention framework ensures the rectification effort was the last major infrastructure incident, establishing robust safeguards for continued stable development.**
