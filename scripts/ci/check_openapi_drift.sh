#!/usr/bin/env bash
# CI Gate: Ensure exported OpenAPI spec matches committed docs/api/openapi.json
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

# Create spec directory if it doesn't exist
mkdir -p "$(dirname "$OPENAPI_SPEC")"
mkdir -p "$(dirname "$TMP_SPEC")"
rm -f "$TMP_SPEC"

echo "Generating OpenAPI spec via export-openapi..."
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
    echo "ERROR: Missing OpenAPI spec at $OPENAPI_SPEC"
    echo ""
    echo "To fix, run:"
    echo "  ./scripts/ci/check_openapi_drift.sh --fix"
    exit 1
fi

if ! diff -q "$OPENAPI_SPEC" "$TMP_SPEC" > /dev/null 2>&1; then
    echo "ERROR: OpenAPI spec drift detected."
    echo ""
    echo "Diff: $OPENAPI_SPEC vs $TMP_SPEC"
    echo "────────────────────────────────────────"
    diff -u "$OPENAPI_SPEC" "$TMP_SPEC" | head -200
    echo "────────────────────────────────────────"
    echo ""
    echo "To fix, run:"
    echo "  ./scripts/ci/check_openapi_drift.sh --fix"
    echo ""
    exit 1
fi

rm -f "$TMP_SPEC"
echo "OK: OpenAPI spec matches docs/api/openapi.json"
echo ""
