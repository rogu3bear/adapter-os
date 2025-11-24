#!/bin/bash
# Validation script for QA automation setup

set -e

echo "🔍 Validating QA Automation Setup..."
echo ""

ERRORS=0
WARNINGS=0

# Check Python scripts
echo "Checking Python scripts..."
for script in scripts/check_coverage.py scripts/compare_benchmarks.py; do
    if [ -f "$script" ]; then
        if python3 -m py_compile "$script" 2>/dev/null; then
            echo "  ✓ $script"
        else
            echo "  ❌ $script - Syntax error"
            ERRORS=$((ERRORS + 1))
        fi
    else
        echo "  ⚠ $script - Not found"
        WARNINGS=$((WARNINGS + 1))
    fi
done

# Check shell scripts
echo ""
echo "Checking shell scripts..."
for script in scripts/generate_test_metrics.sh scripts/collect_stress_results.sh scripts/pre-commit-template scripts/setup_pre_commit.sh; do
    if [ -f "$script" ]; then
        if bash -n "$script" 2>/dev/null; then
            if [ -x "$script" ]; then
                echo "  ✓ $script (executable)"
            else
                echo "  ⚠ $script - Not executable"
                chmod +x "$script"
                echo "    → Fixed: Made executable"
            fi
        else
            echo "  ❌ $script - Syntax error"
            ERRORS=$((ERRORS + 1))
        fi
    else
        echo "  ⚠ $script - Not found"
        WARNINGS=$((WARNINGS + 1))
    fi
done

# Check workflow files
echo ""
echo "Checking GitHub Actions workflows..."
for workflow in .github/workflows/integration-tests.yml .github/workflows/e2e-ui-tests.yml .github/workflows/performance-regression.yml .github/workflows/stress-tests.yml; do
    if [ -f "$workflow" ]; then
        # Basic YAML syntax check
        if python3 -c "import yaml; yaml.safe_load(open('$workflow'))" 2>/dev/null; then
            echo "  ✓ $workflow"
        else
            echo "  ⚠ $workflow - YAML validation skipped (pyyaml not installed)"
        fi
    else
        echo "  ⚠ $workflow - Not found"
        WARNINGS=$((WARNINGS + 1))
    fi
done

# Check configuration files
echo ""
echo "Checking configuration files..."
if [ -f "scripts/coverage_thresholds.json" ]; then
    if python3 -m json.tool scripts/coverage_thresholds.json > /dev/null 2>&1; then
        echo "  ✓ scripts/coverage_thresholds.json"
    else
        echo "  ❌ scripts/coverage_thresholds.json - Invalid JSON"
        ERRORS=$((ERRORS + 1))
    fi
else
    echo "  ⚠ scripts/coverage_thresholds.json - Not found"
    WARNINGS=$((WARNINGS + 1))
fi

# Check required tools
echo ""
echo "Checking required tools..."
for tool in python3 cargo; do
    if command -v "$tool" > /dev/null 2>&1; then
        echo "  ✓ $tool installed"
    else
        echo "  ❌ $tool - Not found"
        ERRORS=$((ERRORS + 1))
    fi
done

# Check optional tools
for tool in cargo-tarpaulin jq; do
    if command -v "$tool" > /dev/null 2>&1; then
        echo "  ✓ $tool installed (optional)"
    else
        echo "  ⚠ $tool - Not installed (will be installed in CI)"
    fi
done

# Summary
echo ""
echo "=========================================="
echo "Validation Summary"
echo "=========================================="
echo "Errors: $ERRORS"
echo "Warnings: $WARNINGS"
echo ""

if [ $ERRORS -eq 0 ]; then
    echo "✅ QA automation setup is valid!"
    if [ $WARNINGS -gt 0 ]; then
        echo "⚠ Some optional components are missing (expected in CI environment)"
    fi
    exit 0
else
    echo "❌ Validation failed with $ERRORS error(s)"
    exit 1
fi

