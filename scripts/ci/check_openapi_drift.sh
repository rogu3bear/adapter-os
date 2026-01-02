#!/usr/bin/env bash
# CI Gate: Ensure exported OpenAPI spec matches committed docs/api/openapi.json

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
export LC_ALL=C
export LANG=C

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

if [ ! -f "$OPENAPI_SPEC" ]; then
    echo "ERROR: Missing OpenAPI spec at $OPENAPI_SPEC"
    echo "Run: ./scripts/generate-sdks.sh --spec-only"
    exit 1
fi

mkdir -p "$(dirname "$TMP_SPEC")"
rm -f "$TMP_SPEC"

echo "Generating OpenAPI spec via export-openapi..."
cargo run --locked -p adapteros-server-api --bin export-openapi -- "$TMP_SPEC"

if [ ! -f "$TMP_SPEC" ]; then
    echo "ERROR: Export failed to create $TMP_SPEC"
    exit 1
fi

if ! diff -q "$OPENAPI_SPEC" "$TMP_SPEC" > /dev/null 2>&1; then
    echo "ERROR: OpenAPI spec drift detected."
    echo "Diff: $OPENAPI_SPEC vs $TMP_SPEC"
    echo "Update with: ./scripts/generate-sdks.sh --spec-only"
    diff -u "$OPENAPI_SPEC" "$TMP_SPEC" | head -200
    exit 1
fi

rm -f "$TMP_SPEC"
echo "OK: OpenAPI spec matches docs/api/openapi.json"
echo ""
