#!/bin/bash
# Build Metal shaders offline for deterministic embedding
#
# This script compiles modular .metal → .air → .metallib
# The resulting .metallib is embedded in the binary with include_bytes!
# and hashed at compile time with BLAKE3
#
# Per Determinism Ruleset #2: Reproducible builds with SOURCE_DATE_EPOCH
#
# References:
# - Metal Shader Compilation: https://developer.apple.com/documentation/metal/shader_compilation
# - Metal Performance Shaders: https://developer.apple.com/documentation/metalperformanceshaders

set -e

# Set deterministic timestamp for reproducible builds
export SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-1704067200}

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
BUILD_TIMESTAMP=$(date -u -r ${SOURCE_DATE_EPOCH} +"%Y-%m-%dT%H:%M:%SZ")

# Log build metadata for reproducibility
echo "📋 Build Metadata:"
echo "   Metal SDK: $METAL_SDK_VERSION"
echo "   Compiler: $COMPILER_VERSION"
echo "   Timestamp: $BUILD_TIMESTAMP"
echo "   Source Date Epoch: ${SOURCE_DATE_EPOCH:-not set}"
echo "   Target: aarch64-apple-darwin"

# Create deterministic build metadata (Per Determinism Ruleset #2)
cat > build_metadata.json <<EOF
{
  "hash": "$HASH",
  "timestamp": "$BUILD_TIMESTAMP",
  "source_date_epoch": ${SOURCE_DATE_EPOCH},
  "metal_sdk_version": "$METAL_SDK_VERSION",
  "compiler_version": "$COMPILER_VERSION"
}
EOF

# Update kernel registry with actual hash and build metadata
echo "📝 Updating kernel registry..."
python3 update_registry.py "$HASH" "$METAL_SDK_VERSION" "$COMPILER_VERSION"

# Verify against baseline if it exists (Per Determinism Ruleset #2)
if [ -f baselines/kernel_hash.txt ]; then
    echo ""
    echo "🔍 Verifying against baseline hash..."
    BASELINE_HASH=$(cat baselines/kernel_hash.txt)
    
    if [ "$HASH" != "$BASELINE_HASH" ]; then
        echo "❌ KERNEL HASH MISMATCH!"
        echo "   Expected: $BASELINE_HASH"
        echo "   Got:      $HASH"
        echo ""
        echo "   This indicates non-deterministic kernel compilation."
        echo "   Please review changes or update baseline if intentional."
        exit 1
    else
        echo "✅ Hash matches baseline: $HASH"
    fi
else
    echo "⚠️  No baseline hash found. Creating baseline..."
    mkdir -p baselines
    echo "$HASH" > baselines/kernel_hash.txt
    echo "   Baseline saved to: baselines/kernel_hash.txt"
fi

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
