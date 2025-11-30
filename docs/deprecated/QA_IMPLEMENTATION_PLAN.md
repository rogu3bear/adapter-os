# QA Implementation Plan - AdapterOS v0.3-alpha

**Date:** 2025-11-24  
**Status:** Assessment Complete - Implementation Required  
**Priority:** High

---

## Executive Summary

AdapterOS has a solid QA foundation with integration tests, CI/CD workflows, and test infrastructure. However, several critical gaps prevent full QA automation and coverage enforcement. This document outlines what's needed to achieve production-ready QA.

---

## Current State Assessment

### ✅ What Exists

| Component | Status | Location |
|-----------|--------|----------|
| **Integration Test Framework** | ✅ Complete | `tests/common/test_harness.rs` |
| **Test Fixtures** | ✅ Complete | `tests/common/fixtures.rs` |
| **Team Test Templates** | ✅ Complete | `tests/integration/team_*.rs` |
| **CI/CD Workflows** | ✅ Partial | `.github/workflows/integration-tests.yml` |
| **Coverage Tooling** | ✅ Configured | Tarpaulin in CI |
| **E2E Tests (Rust)** | ✅ Complete | `tests/e2e/` |
| **E2E Tests (Cypress)** | ✅ Complete | `ui/e2e/cypress/` |
| **Documentation** | ✅ Complete | `docs/testing/` |

### ❌ What's Missing

| Component | Priority | Impact |
|-----------|----------|--------|
| **Coverage Enforcement** | 🔴 Critical | PRs merge with <80% coverage |
| **Coverage Reporting** | 🔴 Critical | No visibility into coverage trends |
| **Pre-commit Hooks** | 🟡 High | Developers can commit failing tests |
| **E2E CI Integration** | 🟡 High | Cypress tests not automated |
| **Performance Regression** | 🟡 High | Benchmarks don't fail on regression |
| **Test Metrics Dashboard** | 🟢 Medium | No centralized test reporting |
| **Stress Testing** | 🟢 Medium | Load tests not automated |

---

## Implementation Tasks

### Phase 1: Coverage Enforcement (Critical)

**Goal:** Block PRs if coverage <80% (per component targets)

#### Task 1.1: Add Coverage Threshold Enforcement

**File:** `.github/workflows/integration-tests.yml`

```yaml
# Add to coverage job
- name: Check coverage thresholds
  run: |
    cargo tarpaulin --test --out stdout --format json > coverage.json
    python3 scripts/check_coverage.py --threshold 80 --report coverage.json
  continue-on-error: false
```

**New File:** `scripts/check_coverage.py`
- Parse tarpaulin JSON output
- Check per-crate coverage against targets:
  - Core Backends: ≥80%
  - Inference Pipeline: ≥85%
  - Training Pipeline: ≥70%
  - Security/Crypto: ≥95%
  - API Handlers: ≥80%
- Fail if any crate below threshold
- Generate coverage report

**Estimated Effort:** 2-3 hours

#### Task 1.2: Integrate Codecov/Coveralls

**Option A: Codecov (Recommended)**
```yaml
# Add to coverage job
- name: Upload to Codecov
  uses: codecov/codecov-action@v3
  with:
    files: ./coverage/cobertura.xml
    flags: unittests
    fail_ci_if_error: true
    minimum_coverage: 80
```

**Option B: Coveralls**
```yaml
- name: Upload to Coveralls
  uses: coverallsapp/github-action@master
  with:
    path-to-lcov: ./coverage/lcov.info
    fail_ci_if_error: true
```

**Estimated Effort:** 1 hour

---

### Phase 2: Pre-commit Hooks (High Priority)

**Goal:** Prevent commits with failing tests or formatting issues

#### Task 2.1: Create Pre-commit Hook Script

**File:** `.git/hooks/pre-commit`

```bash
#!/bin/bash
set -e

echo "🔍 Running pre-commit checks..."

# Format check
echo "  ✓ Checking formatting..."
cargo fmt --all -- --check || {
    echo "❌ Code not formatted. Run: cargo fmt --all"
    exit 1
}

# Lint check
echo "  ✓ Running clippy..."
cargo clippy --workspace -- -D warnings || {
    echo "❌ Clippy errors found"
    exit 1
}

# Fast unit tests (lib only, no integration)
echo "  ✓ Running fast unit tests..."
cargo test --workspace --lib --quiet || {
    echo "❌ Unit tests failed"
    exit 1
}

echo "✅ Pre-commit checks passed"
```

**Task 2.2: Install Hook**

```bash
chmod +x .git/hooks/pre-commit
```

**Task 2.3: Add Setup Script**

**File:** `scripts/setup_pre_commit.sh`

```bash
#!/bin/bash
# Install pre-commit hook
cp scripts/pre-commit-template .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
echo "✅ Pre-commit hook installed"
```

**Estimated Effort:** 1 hour

---

### Phase 3: E2E CI Integration (High Priority)

**Goal:** Run Cypress tests automatically on PRs

#### Task 3.1: Add Cypress Workflow

**File:** `.github/workflows/e2e-ui-tests.yml`

```yaml
name: E2E UI Tests

on:
  pull_request:
    paths:
      - 'ui/**'
      - 'crates/adapteros-server-api/**'
  workflow_dispatch:

jobs:
  cypress:
    runs-on: macos-latest
    timeout-minutes: 30
    
    steps:
      - uses: actions/checkout@v4
      
      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: 'pnpm'
      
      - name: Install dependencies
        run: |
          cd ui
          pnpm install
      
      - name: Start backend server
        run: |
          cargo build --release -p adapteros-server-api
          ./target/release/aosctl serve --port 8080 &
          sleep 5
      
      - name: Run Cypress tests
        run: |
          cd ui
          pnpm cypress:run
      
      - name: Upload screenshots (on failure)
        uses: actions/upload-artifact@v3
        if: failure()
        with:
          name: cypress-screenshots
          path: ui/e2e/cypress/screenshots
```

**Estimated Effort:** 2-3 hours

---

### Phase 4: Performance Regression Detection (High Priority)

**Goal:** Fail CI if benchmarks regress >10%

#### Task 4.1: Add Benchmark Comparison

**File:** `.github/workflows/performance-regression.yml`

```yaml
name: Performance Regression

on:
  pull_request:
    paths:
      - 'crates/adapteros-lora-mlx-ffi/**'
      - 'crates/adapteros-lora-worker/**'
  workflow_dispatch:

jobs:
  benchmark:
    runs-on: macos-latest
    
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Need full history for comparison
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
      
      - name: Run benchmarks (current)
        run: |
          cargo bench -p adapteros-lora-mlx-ffi \
            --bench mlx_integration_benchmark \
            -- --output-format json > current-bench.json
      
      - name: Checkout baseline (main)
        run: |
          git checkout main
          cargo bench -p adapteros-lora-mlx-ffi \
            --bench mlx_integration_benchmark \
            -- --output-format json > baseline-bench.json
      
      - name: Compare benchmarks
        run: |
          python3 scripts/compare_benchmarks.py \
            --baseline baseline-bench.json \
            --current current-bench.json \
            --threshold 0.10  # 10% regression threshold
```

**New File:** `scripts/compare_benchmarks.py`
- Parse Criterion JSON output
- Compare current vs baseline
- Fail if any benchmark regressed >10%
- Generate comparison report

**Estimated Effort:** 3-4 hours

---

### Phase 5: Test Metrics Dashboard (Medium Priority)

**Goal:** Centralized test reporting and metrics

#### Task 5.1: Generate Test Metrics Report

**File:** `scripts/generate_test_metrics.sh`

```bash
#!/bin/bash
# Generate test metrics report

echo "# Test Metrics Report - $(date)" > test-metrics.md
echo "" >> test-metrics.md

# Test counts
echo "## Test Counts" >> test-metrics.md
cargo test --workspace --list 2>/dev/null | grep -c "test " | xargs echo "Total tests:" >> test-metrics.md

# Coverage summary
if command -v cargo-tarpaulin &> /dev/null; then
    echo "## Coverage Summary" >> test-metrics.md
    cargo tarpaulin --test --out stdout --format json | \
        jq -r '.packages[] | "\(.name): \(.coverage)%"' >> test-metrics.md
fi

# Test execution time
echo "## Test Execution Times" >> test-metrics.md
cargo test --workspace --quiet -- --list 2>/dev/null | \
    grep -c "test " | xargs echo "Total test cases:" >> test-metrics.md

echo "✅ Test metrics generated in test-metrics.md"
```

#### Task 5.2: Add Metrics to CI Summary

**File:** `.github/workflows/integration-tests.yml`

```yaml
- name: Generate test metrics
  run: |
    ./scripts/generate_test_metrics.sh
    cat test-metrics.md >> $GITHUB_STEP_SUMMARY
```

**Estimated Effort:** 2 hours

---

### Phase 6: Stress Testing Automation (Medium Priority)

**Goal:** Automated load/stress tests in CI

#### Task 6.1: Add Stress Test Workflow

**File:** `.github/workflows/stress-tests.yml`

```yaml
name: Stress Tests

on:
  schedule:
    - cron: '0 2 * * 0'  # Weekly on Sunday 2 AM
  workflow_dispatch:

jobs:
  stress:
    runs-on: macos-latest
    timeout-minutes: 60
    
    steps:
      - uses: actions/checkout@v4
      
      - name: Build release
        run: cargo build --release
      
      - name: Run stress tests
        run: |
          cargo test --test stress_tests --release \
            -- --nocapture --test-threads=1
      
      - name: Upload stress test results
        uses: actions/upload-artifact@v3
        with:
          name: stress-test-results
          path: var/stress-test-results.json
```

**Estimated Effort:** 2-3 hours

---

## Implementation Priority

### Week 1 (Critical)
1. ✅ Coverage threshold enforcement (Task 1.1)
2. ✅ Codecov integration (Task 1.2)
3. ✅ Pre-commit hooks (Task 2.1-2.3)

### Week 2 (High Priority)
4. ✅ E2E CI integration (Task 3.1)
5. ✅ Performance regression detection (Task 4.1)

### Week 3 (Medium Priority)
6. ✅ Test metrics dashboard (Task 5.1-5.2)
7. ✅ Stress testing automation (Task 6.1)

---

## Success Criteria

### Coverage Enforcement
- [ ] PRs blocked if coverage <80% (per component)
- [ ] Coverage trends visible in Codecov dashboard
- [ ] Coverage report generated on every PR

### Pre-commit Hooks
- [ ] All developers have hooks installed
- [ ] Formatting/linting errors caught before commit
- [ ] Fast unit tests run before commit

### E2E Automation
- [ ] Cypress tests run on every UI PR
- [ ] Screenshots uploaded on failure
- [ ] Tests complete in <30 minutes

### Performance Regression
- [ ] Benchmarks compared against baseline
- [ ] PRs blocked if regression >10%
- [ ] Performance report generated

### Test Metrics
- [ ] Weekly test metrics report generated
- [ ] Coverage trends tracked over time
- [ ] Test execution times monitored

---

## Maintenance

### Weekly Tasks
- Review test metrics dashboard
- Check coverage trends
- Review failed stress tests

### Monthly Tasks
- Update coverage thresholds if needed
- Review and optimize test execution times
- Update pre-commit hooks with new checks

---

## References

- [Verification Strategy](./VERIFICATION-STRATEGY.md) - Coverage targets and test requirements
- [Integration Test Guide](./INTEGRATION_TEST_GUIDE.md) - Test framework documentation
- [CI/CD Workflows](../.github/workflows/) - Existing workflows

---

**Document Status:** Ready for Implementation  
**Next Steps:** Prioritize Phase 1 tasks (coverage enforcement)

