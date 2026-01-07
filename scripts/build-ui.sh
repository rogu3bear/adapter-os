#!/usr/bin/env bash
#
# Build UI WASM with CI-equivalent optimization
#
# Usage: ./scripts/build-ui.sh [--skip-opt]
#
# Requirements:
#   - trunk (cargo install trunk)
#   - wasm-opt (brew install binaryen OR apt install binaryen)
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
UI_DIR="$ROOT_DIR/crates/adapteros-ui"
STATIC_DIR="$ROOT_DIR/crates/adapteros-server/static"

# Parse args
SKIP_OPT=false
if [[ "${1:-}" == "--skip-opt" ]]; then
    SKIP_OPT=true
fi

echo "=== Building AdapterOS UI (WASM) ==="
echo "Directory: $UI_DIR"

# Check trunk
if ! command -v trunk &> /dev/null; then
    echo "Error: trunk not found. Install with: cargo install trunk"
    exit 1
fi

# Build with trunk
cd "$UI_DIR"
echo "Running: trunk build --release"
trunk build --release

# Find the WASM file
WASM_FILE=$(ls "$STATIC_DIR"/adapteros-ui-*_bg.wasm 2>/dev/null | head -1)
if [[ -z "$WASM_FILE" ]]; then
    echo "Error: WASM file not found in $STATIC_DIR"
    exit 1
fi

BEFORE_SIZE=$(stat -f%z "$WASM_FILE" 2>/dev/null || stat -c%s "$WASM_FILE" 2>/dev/null)
echo "Before optimization: $BEFORE_SIZE bytes ($(echo "scale=2; $BEFORE_SIZE / 1048576" | bc) MB)"

# Run wasm-opt if not skipped
if [[ "$SKIP_OPT" == "false" ]]; then
    if ! command -v wasm-opt &> /dev/null; then
        echo "Warning: wasm-opt not found. Install with: brew install binaryen"
        echo "Skipping optimization step."
    else
        echo "Running: wasm-opt -O4 --enable-bulk-memory"
        wasm-opt -O4 --enable-bulk-memory "$WASM_FILE" -o "${WASM_FILE}.opt"
        mv "${WASM_FILE}.opt" "$WASM_FILE"

        AFTER_SIZE=$(stat -f%z "$WASM_FILE" 2>/dev/null || stat -c%s "$WASM_FILE" 2>/dev/null)
        SAVINGS=$((BEFORE_SIZE - AFTER_SIZE))
        PCT=$(echo "scale=1; $SAVINGS * 100 / $BEFORE_SIZE" | bc)
        echo "After optimization:  $AFTER_SIZE bytes ($(echo "scale=2; $AFTER_SIZE / 1048576" | bc) MB)"
        echo "Reduction: $SAVINGS bytes ($PCT%)"
    fi
else
    echo "Skipping wasm-opt (--skip-opt)"
fi

# Compressed size
if command -v gzip &> /dev/null; then
    GZIP_SIZE=$(gzip -9 -c "$WASM_FILE" | wc -c | tr -d ' ')
    echo "Gzipped size: $GZIP_SIZE bytes ($(echo "scale=2; $GZIP_SIZE / 1048576" | bc) MB)"
fi

echo ""
echo "=== Build Complete ==="
echo "Output: $WASM_FILE"
