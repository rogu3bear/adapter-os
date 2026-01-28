#!/usr/bin/env bash
#
# CI Guard: Check for test artifact pollution
#
# This script fails if tests have left behind var/ directories inside crates
# or if var/tmp/ has accumulated garbage. Tests should use OS temp directories
# (via TempDir::new() or std::env::temp_dir()) instead of var/tmp.
#
# Usage: ./scripts/ci/check_test_artifacts.sh
#
set -euo pipefail

echo "Checking for test artifact pollution..."

POLLUTED=()
WARNINGS=()

# Check each crate for var/ directories (these should never exist)
for crate_dir in crates/*/; do
    if [ -d "${crate_dir}var" ]; then
        POLLUTED+=("${crate_dir}var")
    fi
done

# Check configs/var (should not exist)
if [ -d "configs/var" ]; then
    POLLUTED+=("configs/var")
fi

# Check root var/tmp (warn if not empty, but don't fail)
if [ -d "var/tmp" ]; then
    if [ -n "$(ls -A var/tmp 2>/dev/null)" ]; then
        WARNINGS+=("var/tmp is not empty - consider cleaning with: rm -rf var/tmp")
    fi
fi

# Check for stale test databases in var/
for db in var/*-test.sqlite3* var/*_test.sqlite3*; do
    if [ -f "$db" ] 2>/dev/null; then
        WARNINGS+=("Stale test database: $db")
    fi
done

# Report warnings (non-fatal)
if [ ${#WARNINGS[@]} -gt 0 ]; then
    echo ""
    echo "::warning::Test artifact warnings:"
    for warn in "${WARNINGS[@]}"; do
        echo "  - $warn"
    done
fi

# Report errors (fatal)
if [ ${#POLLUTED[@]} -gt 0 ]; then
    echo ""
    echo "::error::Test artifact pollution detected!"
    echo ""
    echo "The following directories should not exist:"
    for dir in "${POLLUTED[@]}"; do
        echo "  - $dir"
    done
    echo ""
    echo "Tests should use OS temp directories, not var/tmp inside crates."
    echo ""
    echo "Fix: Use TempDir::new() or TempDir::with_prefix() from the tempfile crate"
    echo "instead of PathBuf::from(\"var\").join(\"tmp\")"
    echo ""
    echo "To clean up:"
    echo "  find ./crates -type d -name \"var\" -not -path \"*/target/*\" -exec rm -rf {} +"
    exit 1
fi

echo "✓ No test artifact pollution detected"
