#!/bin/bash
# Build the Leptos UI for production

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
UI_DIR="$PROJECT_ROOT/crates/adapteros-ui"

echo "Building Leptos UI..."

# Ensure rustup toolchain is used
RUSTUP_TOOLCHAIN="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin"
if [ ! -d "$RUSTUP_TOOLCHAIN" ]; then
    echo "Error: Rustup toolchain not found at $RUSTUP_TOOLCHAIN"
    echo "Please install with: rustup target add wasm32-unknown-unknown"
    exit 1
fi

# Install npm dependencies if needed
if [ ! -d "$UI_DIR/node_modules" ]; then
    echo "Installing npm dependencies..."
    (cd "$UI_DIR" && pnpm install)
fi

# Build with trunk
echo "Building with Trunk..."
TRUNK="${TRUNK:-$HOME/.cargo/bin/trunk}"
if [ ! -x "$TRUNK" ]; then
    echo "Error: trunk not found at $TRUNK"
    echo "Install with: cargo install trunk"
    exit 1
fi

cd "$UI_DIR"
CARGO="$RUSTUP_TOOLCHAIN/bin/cargo" \
RUSTC="$RUSTUP_TOOLCHAIN/bin/rustc" \
    "$TRUNK" build "$@"

echo "Leptos UI built successfully!"
echo "Output: $PROJECT_ROOT/crates/adapteros-server/static/"
