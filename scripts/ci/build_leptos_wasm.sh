#!/usr/bin/env bash
# CI Gate: Ensure Leptos UI builds to WASM in release mode

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
UI_DIR="$ROOT_DIR/crates/adapteros-ui"
RUST_TOOLCHAIN_FILE="$ROOT_DIR/rust-toolchain.toml"
PNPM_REQUIRED=""

cd "$ROOT_DIR"
export LC_ALL=C
export LANG=C

if [ -f "$RUST_TOOLCHAIN_FILE" ]; then
    RUST_TOOLCHAIN="$(sed -n 's/.*channel *= *"\\([^"]*\\)".*/\\1/p' "$RUST_TOOLCHAIN_FILE")"
    if [ -n "${RUST_TOOLCHAIN:-}" ]; then
        export RUSTUP_TOOLCHAIN="$RUST_TOOLCHAIN"
    fi
fi
RUSTUP_ARGS=()
if [ -n "${RUSTUP_TOOLCHAIN:-}" ]; then
    RUSTUP_ARGS=(--toolchain "$RUSTUP_TOOLCHAIN")
fi

if [ -f "$ROOT_DIR/crates/adapteros-ui/package.json" ]; then
    PNPM_REQUIRED="$(sed -n 's/.*"packageManager": "pnpm@\\([^"]*\\)".*/\\1/p' "$ROOT_DIR/crates/adapteros-ui/package.json")"
fi

echo "=== Leptos WASM Build Check ==="
echo ""

if [ ! -d "$UI_DIR" ]; then
    echo "ERROR: Leptos UI directory not found at $UI_DIR"
    exit 1
fi

if ! command -v rustup > /dev/null 2>&1; then
    echo "ERROR: rustup not found"
    echo "Install Rust toolchain with rustup and retry."
    exit 1
fi

if ! rustup target list --installed "${RUSTUP_ARGS[@]}" | grep -q "^wasm32-unknown-unknown$"; then
    echo "ERROR: wasm32-unknown-unknown target not installed"
    echo "Run: rustup target add wasm32-unknown-unknown"
    exit 1
fi

if ! command -v trunk > /dev/null 2>&1; then
    echo "ERROR: trunk not found"
    echo "Install with: cargo install trunk"
    exit 1
fi

if ! command -v pnpm > /dev/null 2>&1; then
    echo "ERROR: pnpm not found"
    echo "Install pnpm 9+ and retry."
    exit 1
fi

if [ -n "$PNPM_REQUIRED" ]; then
    PNPM_VERSION="$(pnpm --version)"
    if [ "$PNPM_VERSION" != "$PNPM_REQUIRED" ]; then
        echo "ERROR: pnpm version mismatch (required $PNPM_REQUIRED, found $PNPM_VERSION)."
        exit 1
    fi
fi

if [ ! -f "$UI_DIR/pnpm-lock.yaml" ]; then
    echo "ERROR: Missing pnpm lockfile at $UI_DIR/pnpm-lock.yaml"
    exit 1
fi

pnpm --dir "$UI_DIR" install --frozen-lockfile

cargo build --locked --target wasm32-unknown-unknown -p adapteros-ui

cd "$UI_DIR"
trunk build --release

OUTPUT_DIR="$ROOT_DIR/crates/adapteros-server/static"
if [ ! -d "$OUTPUT_DIR" ]; then
    echo "ERROR: Expected output directory not found at $OUTPUT_DIR"
    exit 1
fi
if [ ! -f "$OUTPUT_DIR/index.html" ]; then
    echo "ERROR: Expected index.html missing in $OUTPUT_DIR"
    exit 1
fi
if ! compgen -G "$OUTPUT_DIR/*.wasm" > /dev/null; then
    echo "ERROR: Expected wasm output missing in $OUTPUT_DIR"
    exit 1
fi

echo "OK: Leptos WASM build succeeded"
echo ""
