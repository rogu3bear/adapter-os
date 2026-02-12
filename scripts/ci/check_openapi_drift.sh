#!/usr/bin/env bash
# CI Gate: Ensure exported OpenAPI spec matches committed docs/api/openapi.json
#
# This script performs two checks:
# 1. Verify utoipa version matches expected (prevents silent spec changes from dependency updates)
# 2. Verify generated OpenAPI spec matches committed docs/api/openapi.json
#
# Usage:
#   ./scripts/ci/check_openapi_drift.sh          # Check for drift (CI mode)
#   ./scripts/ci/check_openapi_drift.sh --fix    # Auto-fix drift by updating spec
#   ./scripts/ci/check_openapi_drift.sh --help   # Show this help

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
export LC_ALL=C
export LANG=C

# =============================================================================
# Configuration: Expected utoipa version
# This MUST match the pinned version in workspace Cargo.toml
# =============================================================================
EXPECTED_UTOIPA="5.4.0"

# Parse arguments
FIX_MODE=false
for arg in "$@"; do
    case "$arg" in
        --fix)
            FIX_MODE=true
            ;;
        --help|-h)
            echo "Usage: $0 [--fix] [--help]"
            echo ""
            echo "Options:"
            echo "  --fix   Auto-fix drift by copying generated spec to docs/api/openapi.json"
            echo "  --help  Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0           # Check for drift (exit 1 if drift detected)"
            echo "  $0 --fix     # Generate and update the committed spec"
            exit 0
            ;;
    esac
done

echo "=== OpenAPI Export Drift Check ==="
echo ""

OPENAPI_SPEC="$ROOT_DIR/docs/api/openapi.json"
TMP_SPEC="$ROOT_DIR/target/codegen/openapi.check.json"
RUST_TOOLCHAIN_FILE="$ROOT_DIR/rust-toolchain.toml"

if [ -f "$RUST_TOOLCHAIN_FILE" ]; then
    RUST_TOOLCHAIN="$(sed -n 's/.*channel *= *"\\([^"]*\\)".*/\\1/p' "$RUST_TOOLCHAIN_FILE")"
    if [ -n "${RUST_TOOLCHAIN:-}" ]; then
        export RUSTUP_TOOLCHAIN="$RUST_TOOLCHAIN"
    fi
fi

# =============================================================================
# Step 1: Verify utoipa version
# =============================================================================
echo "Checking utoipa version..."

# Extract utoipa version used by adapteros-server-api
# Use cargo metadata + jq if available, fallback to Cargo.lock grep
ACTUAL_UTOIPA=""
if command -v jq >/dev/null 2>&1; then
    # jq available: get the actual resolved utoipa version from packages
    ACTUAL_UTOIPA=$(cargo metadata --format-version 1 2>/dev/null \
        | jq -r '.packages[] | select(.name == "utoipa") | .version' \
        2>/dev/null || true)
fi

# Fallback: parse Cargo.lock directly
if [ -z "$ACTUAL_UTOIPA" ] || [ "$ACTUAL_UTOIPA" = "null" ]; then
    if [ -f "$ROOT_DIR/Cargo.lock" ]; then
        ACTUAL_UTOIPA=$(grep -A1 '^name = "utoipa"$' "$ROOT_DIR/Cargo.lock" \
            | grep '^version' \
            | head -1 \
            | sed 's/version = "\\([^"]*\\)"/\\1/')
    fi
fi

if [ -z "$ACTUAL_UTOIPA" ]; then
    echo "WARNING: Could not determine utoipa version (jq not available and Cargo.lock parse failed)"
    echo "         Skipping version check, proceeding with spec generation..."
    echo ""
else
    echo "  Expected: $EXPECTED_UTOIPA"
    echo "  Actual:   $ACTUAL_UTOIPA"

    if [ "$ACTUAL_UTOIPA" != "$EXPECTED_UTOIPA" ]; then
        echo ""
        echo "┌─────────────────────────────────────────────────────────────────────┐"
        echo "│ ERROR: utoipa version mismatch                                      │"
        echo "├─────────────────────────────────────────────────────────────────────┤"
        echo "│ Expected: $EXPECTED_UTOIPA"
        echo "│ Actual:   $ACTUAL_UTOIPA"
        echo "│                                                                     │"
        echo "│ The utoipa version is pinned to ensure deterministic OpenAPI spec.  │"
        echo "│ If you intentionally updated utoipa:                                │"
        echo "│   1. Update EXPECTED_UTOIPA in this script                          │"
        echo "│   2. Run: ./scripts/ci/check_openapi_drift.sh --fix                 │"
        echo "│   3. Commit both changes together                                   │"
        echo "└─────────────────────────────────────────────────────────────────────┘"
        exit 1
    fi
    echo "  ✓ utoipa version matches"
    echo ""
fi

# =============================================================================
# Step 2: Generate and compare OpenAPI spec
# =============================================================================

# Create spec directory if it doesn't exist
mkdir -p "$(dirname "$OPENAPI_SPEC")"
mkdir -p "$(dirname "$TMP_SPEC")"
rm -f "$TMP_SPEC"

echo "Generating OpenAPI spec via export-openapi..."
# Deterministic builds: rely on the committed SQLx offline cache.
# This avoids compile-time DB access (and failures) during spec generation.
SQLX_OFFLINE_DIR="${SQLX_OFFLINE_DIR:-$ROOT_DIR/crates/adapteros-db/.sqlx}"
env SQLX_OFFLINE=1 SQLX_OFFLINE_DIR="$SQLX_OFFLINE_DIR" \
  cargo run --locked -p adapteros-server-api --bin export-openapi -- "$TMP_SPEC"

if [ ! -f "$TMP_SPEC" ]; then
    echo "ERROR: Export failed to create $TMP_SPEC"
    exit 1
fi

# Fix mode: just copy the generated spec
if [ "$FIX_MODE" = true ]; then
    cp "$TMP_SPEC" "$OPENAPI_SPEC"
    rm -f "$TMP_SPEC"
    echo "FIXED: Updated $OPENAPI_SPEC"
    echo ""
    echo "Don't forget to commit the updated spec:"
    echo "  git add docs/api/openapi.json"
    echo "  git commit -m 'chore: update OpenAPI spec'"
    exit 0
fi

# Check mode: compare and report drift
if [ ! -f "$OPENAPI_SPEC" ]; then
    echo ""
    echo "┌─────────────────────────────────────────────────────────────────────┐"
    echo "│ ERROR: Missing OpenAPI spec                                         │"
    echo "├─────────────────────────────────────────────────────────────────────┤"
    echo "│ File not found: docs/api/openapi.json                               │"
    echo "│                                                                     │"
    echo "│ To fix, run:                                                        │"
    echo "│   ./scripts/ci/check_openapi_drift.sh --fix && git add docs/api/openapi.json │"
    echo "└─────────────────────────────────────────────────────────────────────┘"
    exit 1
fi

if ! diff -q "$OPENAPI_SPEC" "$TMP_SPEC" > /dev/null 2>&1; then
    echo ""
    echo "┌─────────────────────────────────────────────────────────────────────┐"
    echo "│ ERROR: OpenAPI spec drift detected                                  │"
    echo "├─────────────────────────────────────────────────────────────────────┤"
    echo "│ The committed docs/api/openapi.json does not match the generated    │"
    echo "│ spec. This usually means route annotations changed without updating │"
    echo "│ the spec file.                                                      │"
    echo "│                                                                     │"
    echo "│ To fix, run:                                                        │"
    echo "│   ./scripts/ci/check_openapi_drift.sh --fix && git add docs/api/openapi.json │"
    echo "└─────────────────────────────────────────────────────────────────────┘"
    echo ""
    echo "Diff (first 200 lines):"
    echo "────────────────────────────────────────"
    diff -u "$OPENAPI_SPEC" "$TMP_SPEC" | head -200 || true
    echo "────────────────────────────────────────"
    echo ""
    echo "┌─────────────────────────────────────────────────────────────────────┐"
    echo "│ Copy-paste fix command:                                             │"
    echo "│   ./scripts/ci/check_openapi_drift.sh --fix && git add docs/api/openapi.json │"
    echo "└─────────────────────────────────────────────────────────────────────┘"
    exit 1
fi

rm -f "$TMP_SPEC"
echo "OK: OpenAPI spec matches docs/api/openapi.json"
echo ""
