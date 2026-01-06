#!/usr/bin/env bash
# CI Guard: Verify policy registry invariant
# This script ensures that the policy registry maintains the expected policy count
# and that PolicyId::all() stays synchronized with POLICY_INDEX.
#
# Exit codes:
#   0 - All checks passed
#   1 - Policy registry tests failed or count mismatch

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

echo "=== Policy Registry Guard ==="
echo ""

# Run the policy registry cross-check tests
echo "Step 1/3: Running PolicyId::all() ↔ POLICY_INDEX cross-check (registry.rs)..."
if ! cargo test -p adapteros-policy --lib registry::tests::test_policy_id_all_matches_policy_index -- --nocapture; then
    echo "❌ FAIL: registry.rs cross-check failed"
    exit 1
fi
echo "✓ registry.rs cross-check passed"
echo ""

echo "Step 2/3: Running policy count verification (registry.rs)..."
if ! cargo test -p adapteros-policy --lib registry::tests::test_policy_count -- --nocapture; then
    echo "❌ FAIL: policy count verification failed"
    exit 1
fi
echo "✓ Policy count verified"
echo ""

echo "Step 3/3: Running comprehensive policy validation tests..."
if ! cargo test -p adapteros-policy --test policy_validation_comprehensive test_all -- --nocapture; then
    echo "❌ FAIL: comprehensive validation tests failed"
    exit 1
fi
echo "✓ Comprehensive validation tests passed"
echo ""

# Verify no #[ignore] attributes in policy tests
echo "Bonus: Checking for ignored policy registry tests..."
IGNORED_COUNT=$(grep -rn '#\[ignore' crates/adapteros-policy/src/registry.rs crates/adapteros-policy/tests/policy_validation_comprehensive.rs 2>/dev/null | grep -c 'test_.*policy' || echo "0")

if [[ "$IGNORED_COUNT" =~ ^[0-9]+$ ]] && [ "$IGNORED_COUNT" -gt 0 ]; then
    echo "⚠️  WARNING: Found $IGNORED_COUNT ignored policy tests"
    grep -rn '#\[ignore' crates/adapteros-policy/src/registry.rs crates/adapteros-policy/tests/policy_validation_comprehensive.rs 2>/dev/null | grep 'test_.*policy' || true
    exit 1
fi
echo "✓ No ignored policy registry tests found"
echo ""

echo "=== Policy Registry Guard: PASSED ==="
echo ""
echo "Summary:"
echo "  ✓ PolicyId::all() matches POLICY_INDEX"
echo "  ✓ All policies are registered"
echo "  ✓ Policy IDs are sequential (1-max)"
echo "  ✓ All policies marked as implemented"
echo "  ✓ No tests are ignored"
echo ""
