#!/bin/bash
# Hermetic Metal kernel build with toolchain validation
#
# This script ensures reproducible builds by validating the toolchain
# version before compiling Metal shaders. It fails fast if the environment
# doesn't match the approved configuration.

set -e

# Source shared toolchain detection logic
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

echo "🔨 Hermetic Metal Kernel Build"
echo ""

# Load toolchain configuration
TOOLCHAIN_CONFIG="toolchain.toml"
if [ ! -f "$TOOLCHAIN_CONFIG" ]; then
    echo "❌ Error: toolchain.toml not found"
    exit 1
fi

# Extract Xcode version
XCODE_VERSION=$(xcodebuild -version | head -n 1 | awk '{print $2}')
echo "📦 Detected Xcode version: $XCODE_VERSION"

# Parse allowed versions from TOML (simple grep-based parsing)
ALLOWED_VERSIONS=$(grep 'xcode = ' "$TOOLCHAIN_CONFIG" | sed 's/.*\[\(.*\)\].*/\1/' | tr -d '"' | tr ',' '\n')

# Check if current version is allowed
VERSION_ALLOWED=false
for allowed in $ALLOWED_VERSIONS; do
    if [[ "$XCODE_VERSION" == "$allowed"* ]]; then
        VERSION_ALLOWED=true
        break
    fi
done

if [ "$VERSION_ALLOWED" = false ]; then
    echo "❌ Error: Xcode version $XCODE_VERSION not in allowed list"
    echo "   Allowed versions:"
    echo "$ALLOWED_VERSIONS" | sed 's/^/     - /'
    exit 1
fi

echo "✅ Xcode version approved"
echo ""

# Check for required tools
if ! command -v xcrun &> /dev/null; then
    echo "❌ Error: xcrun not found (Xcode Command Line Tools required)"
    exit 1
fi

if ! command -v b3sum &> /dev/null; then
    echo "⚠️  Warning: b3sum not found, hash computation will be skipped"
    echo "   Install with: brew install b3sum"
fi

# Detect Metal toolchain
METAL_CMD=$(resolve_metal_toolchain)
SDK_PATH=$(get_sdk_path)
SDK_ARGS=""
if [ -n "$SDK_PATH" ]; then
    SDK_ARGS="-isysroot $SDK_PATH"
fi

if [ -n "$METAL_CMD" ]; then
    echo "  Using Metal toolchain from: $METAL_CMD"
else
    echo "  Using system Metal (xcrun)"
    METAL_CMD="xcrun"
    SDK_ARGS="-sdk macosx metal"
fi

# Compile Metal shaders with deterministic flags
echo "📦 Compiling aos_kernels.metal..."
$METAL_CMD $SDK_ARGS -c aos_kernels.metal -o aos_kernels.air \
    -std=metal3.1 -ffp-contract=off

if [ $? -ne 0 ]; then
    echo "❌ Compilation failed"
    exit 1
fi

echo "✅ Compilation successful"

# Link into metallib
echo "🔗 Linking metallib..."

METALLIB_CMD=$(resolve_metallib_toolchain)
METALLIB_ARGS=""

if [ -n "$METALLIB_CMD" ]; then
    echo "  Using metallib from: $METALLIB_CMD"
else
    echo "  Using system metallib (xcrun)"
    METALLIB_CMD="xcrun"
    METALLIB_ARGS="-sdk macosx metallib"
fi

$METALLIB_CMD $METALLIB_ARGS aos_kernels.air -o aos_kernels.metallib

if [ $? -ne 0 ]; then
    echo "❌ Linking failed"
    rm -f aos_kernels.air
    exit 1
fi

echo "✅ Linking successful"

# Clean up intermediate files
rm -f aos_kernels.air

# Compute BLAKE3 hash
if command -v b3sum &> /dev/null; then
    echo ""
    echo "🔐 Computing BLAKE3 hash..."
    HASH=$(b3sum aos_kernels.metallib | awk '{print $1}')
    echo "   Hash: $HASH"
    echo ""
    echo "✅ Build complete"
    echo "   Output: aos_kernels.metallib"
    echo "   BLAKE3: $HASH"
else
    echo ""
    echo "✅ Build complete"
    echo "   Output: aos_kernels.metallib"
fi

# Save hash to file for Rust embedding
HASH=$(b3sum aos_kernels.metallib | awk '{print $1}')
echo "$HASH" > kernel_hash.txt
echo "   Hash saved to: kernel_hash.txt"

# Copy to crate directory
mkdir -p ../crates/mplora-kernel-mtl/shaders
cp aos_kernels.metallib ../crates/mplora-kernel-mtl/shaders/
cp kernel_hash.txt ../crates/mplora-kernel-mtl/shaders/

echo ""
echo "📁 Copied to: ../crates/mplora-kernel-mtl/shaders/aos_kernels.metallib"
echo "📁 Hash copied to: ../crates/mplora-kernel-mtl/shaders/kernel_hash.txt"
