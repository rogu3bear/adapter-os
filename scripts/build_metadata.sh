#!/bin/bash
# Build metadata collection for reproducibility

set -e

echo "🔍 Collecting build metadata..."

# Create metadata directory
mkdir -p target/metadata

# Collect Rust toolchain info
echo "📋 Rust Toolchain:"
rustc --version > target/metadata/rustc_version.txt
rustc --version --verbose > target/metadata/rustc_verbose.txt
cargo --version > target/metadata/cargo_version.txt

# Extract commit hash
RUSTC_COMMIT=$(rustc --version --verbose | grep "commit-hash" | awk '{print $2}')
echo "$RUSTC_COMMIT" > target/metadata/rustc_commit.txt

# Collect Metal info
echo "📋 Metal Toolchain:"
xcrun --show-sdk-version > target/metadata/metal_sdk_version.txt 2>/dev/null || echo "unknown" > target/metadata/metal_sdk_version.txt
xcrun metal --version > target/metadata/metal_compiler_version.txt 2>/dev/null || echo "unknown" > target/metadata/metal_compiler_version.txt

# Collect system info
echo "📋 System Info:"
uname -a > target/metadata/system_info.txt
sw_vers > target/metadata/macos_version.txt 2>/dev/null || echo "unknown" > target/metadata/macos_version.txt

# Collect environment variables
echo "📋 Environment Variables:"
env | grep -E "(SOURCE_DATE_EPOCH|CARGO_|RUSTC_|RUST_)" > target/metadata/env_vars.txt || true

# Collect build configuration
echo "📋 Build Configuration:"
echo "Target: aarch64-apple-darwin" > target/metadata/build_config.txt
echo "Mode: release" >> target/metadata/build_config.txt
echo "Optimization: 3" >> target/metadata/build_config.txt
echo "LTO: enabled" >> target/metadata/build_config.txt
echo "Codegen units: 1" >> target/metadata/build_config.txt

# Collect dependency info
echo "📋 Dependencies:"
cargo tree --format "{p} {f}" > target/metadata/dependency_tree.txt 2>/dev/null || true

# Create consolidated metadata file
echo "📋 Creating consolidated metadata..."
cat > target/metadata/build_metadata.json << EOF
{
  "build_timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "source_date_epoch": "${SOURCE_DATE_EPOCH:-null}",
  "rustc_version": "$(rustc --version)",
  "rustc_commit": "$RUSTC_COMMIT",
  "cargo_version": "$(cargo --version)",
  "metal_sdk_version": "$(xcrun --show-sdk-version 2>/dev/null || echo unknown)",
  "metal_compiler_version": "$(xcrun metal --version 2>/dev/null | head -1 || echo unknown)",
  "target_triple": "aarch64-apple-darwin",
  "build_mode": "release",
  "optimization_level": "3",
  "lto_enabled": true,
  "codegen_units": 1,
  "system_info": "$(uname -a)",
  "macos_version": "$(sw_vers -productVersion 2>/dev/null || echo unknown)"
}
EOF

echo "✅ Build metadata collected in target/metadata/"
echo "   Files:"
ls -la target/metadata/
