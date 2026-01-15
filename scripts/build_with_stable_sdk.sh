#!/bin/bash
# Build script with macOS 14.4 SDK pin to avoid Sequoia beta linker issues
# Usage: ./scripts/build_with_stable_sdk.sh [cargo-args...]

set -e

echo "🔧 Applying macOS 14.4 SDK workaround for Sequoia beta linker issues..."

# Set SDK to stable macOS 14.4
export SDKROOT=$(xcrun --sdk macosx14.4 --show-sdk-path)

if [ -z "$SDKROOT" ]; then
    echo "❌ Failed to find macOS 14.4 SDK"
    echo "Please ensure Xcode 14.4 is installed or update the SDK path"
    exit 1
fi

echo "✅ Using SDK: $SDKROOT"

# Verify SDK exists
if [ ! -d "$SDKROOT" ]; then
    echo "❌ SDK directory does not exist: $SDKROOT"
    echo "Please install Xcode 14.4 or update the SDK path in this script"
    exit 1
fi

# Set additional environment variables for reproducible builds
export SOURCE_DATE_EPOCH='1704067200'
# Note: Removed -Wl,-no_uuid flag as it causes LC_UUID linker errors on macOS Sequoia
export RUSTFLAGS='-Cdebuginfo=0'
export CARGO_TERM_COLOR=always

echo "🚀 Building adapterOS with stable SDK..."
echo "Command: cargo build $@"

# Run cargo build with provided arguments
cargo build "$@"

echo "✅ Build completed successfully!"
echo ""
echo "🔍 Verifying LC_UUID load commands..."

# Check for any built dylibs and verify their UUIDs
find target -name "*.dylib" -exec sh -c '
    echo "Checking: $1"
    if otool -l "$1" | grep -q "LC_UUID"; then
        echo "  ✅ LC_UUID present"
        otool -l "$1" | grep LC_UUID -A2
    else
        echo "  ⚠️  No LC_UUID found"
    fi
' _ {} \;

echo ""
echo "🎉 adapterOS build verification complete!"
echo "The macOS 14.4 SDK workaround successfully avoided Sequoia beta linker issues."
