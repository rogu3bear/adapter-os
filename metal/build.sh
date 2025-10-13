#!/bin/bash
# Build Metal shaders offline for deterministic embedding
#
# This script compiles modular .metal → .air → .metallib
# The resulting .metallib is embedded in the binary with include_bytes!
# and hashed at compile time with BLAKE3
#
# References:
# - Metal Shader Compilation: https://developer.apple.com/documentation/metal/shader_compilation
# - Metal Performance Shaders: https://developer.apple.com/documentation/metalperformanceshaders

set -e

echo "🔨 Building modular Metal shaders for AdapterOS..."

# Create output directory
mkdir -p ../crates/adapteros-lora-kernel-mtl/shaders

# Compile modular kernels to AIR (Apple Intermediate Representation) with optimization
echo "📦 Compiling modular kernels..."

# Compile the unified kernel file that includes all modules
echo "  - adapteros_kernels.metal (includes all modules)"
xcrun -sdk macosx metal -c src/kernels/adapteros_kernels.metal -o adapteros_kernels.air -std=metal3.1

# Link into metallib
echo "🔗 Linking modular metallib..."
xcrun -sdk macosx metallib adapteros_kernels.air -o adapteros_kernels.metallib

echo "✅ Modular Metal library built: adapteros_kernels.metallib"
echo ""
echo "🔐 Hash (BLAKE3):"
HASH=$(b3sum adapteros_kernels.metallib | awk '{print $1}')
echo "   Hash: $HASH"

# Save hash to file for Rust embedding
echo "$HASH" > kernel_hash.txt
echo "   Saved to: kernel_hash.txt"

# Get Metal SDK and compiler versions
echo "🔍 Gathering build metadata..."
METAL_SDK_VERSION=$(xcrun --show-sdk-version 2>/dev/null || echo "unknown")
COMPILER_VERSION=$(xcrun metal --version 2>/dev/null | head -1 || echo "unknown")
BUILD_TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Update kernel registry with actual hash and build metadata
echo "📝 Updating kernel registry..."
python3 update_registry.py "$HASH" "$METAL_SDK_VERSION" "$COMPILER_VERSION"

# Copy to Rust crate
cp adapteros_kernels.metallib ../crates/adapteros-lora-kernel-mtl/shaders/
cp kernel_hash.txt ../crates/adapteros-lora-kernel-mtl/shaders/

# Clean up intermediate files
rm -f *.air

echo ""
echo "📁 Output: ../crates/adapteros-lora-kernel-mtl/shaders/adapteros_kernels.metallib"
echo "📁 Hash: ../crates/adapteros-lora-kernel-mtl/shaders/kernel_hash.txt"
echo ""
echo "To embed in Rust code:"
echo "  const METALLIB_BYTES: &[u8] = include_bytes!(\"adapteros_kernels.metallib\");"
echo "  const METALLIB_HASH: &str = include_str!(\"kernel_hash.txt\");"
