#!/bin/bash
# Record environment variables affecting builds

set -e

echo "📋 Recording environment variables..."

# Create environment directory
mkdir -p target/environment

# Record all environment variables
echo "🔍 Full environment dump:"
env > target/environment/full_env.txt

# Record build-related environment variables
echo "🔍 Build-related environment variables:"
cat > target/environment/build_env.txt << EOF
# Build Environment Variables
# Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)

# Reproducibility
SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-not set}
CARGO_INCREMENTAL=${CARGO_INCREMENTAL:-not set}
RUSTC_WRAPPER=${RUSTC_WRAPPER:-not set}
CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-not set}

# Rust/Cargo
RUSTFLAGS=${RUSTFLAGS:-not set}
CARGO_BUILD_RUSTFLAGS=${CARGO_BUILD_RUSTFLAGS:-not set}
CARGO_BUILD_TARGET=${CARGO_BUILD_TARGET:-not set}
CARGO_BUILD_TARGET_DIR=${CARGO_BUILD_TARGET_DIR:-not set}
CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-not set}

# System
PATH=${PATH:-not set}
HOME=${HOME:-not set}
USER=${USER:-not set}
SHELL=${SHELL:-not set}

# CI/CD
CI=${CI:-not set}
GITHUB_ACTIONS=${GITHUB_ACTIONS:-not set}
GITHUB_WORKFLOW=${GITHUB_WORKFLOW:-not set}
GITHUB_RUN_ID=${GITHUB_RUN_ID:-not set}
GITHUB_SHA=${GITHUB_SHA:-not set}

# Platform
PLATFORM=${PLATFORM:-not set}
ARCH=${ARCH:-not set}
OS=${OS:-not set}

# Build tools
XCODE_VERSION=${XCODE_VERSION:-not set}
METAL_SDK_VERSION=${METAL_SDK_VERSION:-not set}
EOF

# Record Rust toolchain info
echo "🔍 Rust toolchain environment:"
cat > target/environment/rust_env.txt << EOF
# Rust Toolchain Environment
# Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)

# Versions
RUSTC_VERSION=$(rustc --version 2>/dev/null || echo "not available")
CARGO_VERSION=$(cargo --version 2>/dev/null || echo "not available")

# Toolchain
RUSTUP_TOOLCHAIN=${RUSTUP_TOOLCHAIN:-not set}
RUSTUP_HOME=${RUSTUP_HOME:-not set}
RUSTUP_UPDATE_ROOT=${RUSTUP_UPDATE_ROOT:-not set}

# Target
RUST_TARGET=${RUST_TARGET:-not set}
RUST_TARGET_PATH=${RUST_TARGET_PATH:-not set}

# Compiler
RUSTC=${RUSTC:-not set}
RUSTDOC=${RUSTDOC:-not set}
RUSTC_LINKER=${RUSTC_LINKER:-not set}
RUSTC_WRAPPER=${RUSTC_WRAPPER:-not set}

# Cargo
CARGO=${CARGO:-not set}
CARGO_HOME=${CARGO_HOME:-not set}
CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-not set}
CARGO_INCREMENTAL=${CARGO_INCREMENTAL:-not set}
CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-not set}
CARGO_BUILD_TARGET=${CARGO_BUILD_TARGET:-not set}
CARGO_BUILD_TARGET_DIR=${CARGO_BUILD_TARGET_DIR:-not set}
CARGO_BUILD_RUSTFLAGS=${CARGO_BUILD_RUSTFLAGS:-not set}
CARGO_BUILD_RUSTC=${CARGO_BUILD_RUSTC:-not set}
CARGO_BUILD_RUSTDOC=${CARGO_BUILD_RUSTDOC:-not set}
CARGO_BUILD_RUSTC_WRAPPER=${CARGO_BUILD_RUSTC_WRAPPER:-not set}
CARGO_BUILD_RUSTC_LINKER=${CARGO_BUILD_RUSTC_LINKER:-not set}
CARGO_BUILD_RUSTC_LINK_ARG=${CARGO_BUILD_RUSTC_LINK_ARG:-not set}
CARGO_BUILD_RUSTC_LINK_LIB=${CARGO_BUILD_RUSTC_LINK_LIB:-not set}
CARGO_BUILD_RUSTC_LINK_SEARCH=${CARGO_BUILD_RUSTC_LINK_SEARCH:-not set}
CARGO_BUILD_RUSTC_LINK_FRAMEWORK=${CARGO_BUILD_RUSTC_LINK_FRAMEWORK:-not set}
CARGO_BUILD_RUSTC_LINK_ARG=${CARGO_BUILD_RUSTC_LINK_ARG:-not set}
EOF

# Record system info
echo "🔍 System environment:"
cat > target/environment/system_env.txt << EOF
# System Environment
# Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)

# OS Info
OS_NAME=$(uname -s 2>/dev/null || echo "unknown")
OS_RELEASE=$(uname -r 2>/dev/null || echo "unknown")
OS_VERSION=$(uname -v 2>/dev/null || echo "unknown")
ARCHITECTURE=$(uname -m 2>/dev/null || echo "unknown")

# macOS Info
MACOS_VERSION=$(sw_vers -productVersion 2>/dev/null || echo "unknown")
MACOS_BUILD=$(sw_vers -buildVersion 2>/dev/null || echo "unknown")

# Xcode Info
XCODE_VERSION=$(xcodebuild -version 2>/dev/null | head -1 || echo "unknown")
XCODE_PATH=$(xcode-select -p 2>/dev/null || echo "unknown")

# Metal Info
METAL_SDK_VERSION=$(xcrun --show-sdk-version 2>/dev/null || echo "unknown")
METAL_COMPILER_VERSION=$(xcrun metal --version 2>/dev/null | head -1 || echo "unknown")

# CPU Info
CPU_BRAND=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo "unknown")
CPU_FEATURES=$(sysctl -n machdep.cpu.features 2>/dev/null || echo "unknown")

# Memory Info
MEMORY_SIZE=$(sysctl -n hw.memsize 2>/dev/null || echo "unknown")

# Build Tools
MAKE_VERSION=$(make --version 2>/dev/null | head -1 || echo "unknown")
GIT_VERSION=$(git --version 2>/dev/null || echo "unknown")
EOF

# Record build configuration
echo "🔍 Build configuration:"
cat > target/environment/build_config.txt << EOF
# Build Configuration
# Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)

# Cargo Configuration
CARGO_CONFIG_PATH=${CARGO_CONFIG_PATH:-not set}
CARGO_CONFIG_DIR=${CARGO_CONFIG_DIR:-not set}

# Build Flags
RUSTFLAGS=${RUSTFLAGS:-not set}
CARGO_BUILD_RUSTFLAGS=${CARGO_BUILD_RUSTFLAGS:-not set}

# Target Configuration
TARGET=${TARGET:-not set}
TARGET_DIR=${TARGET_DIR:-not set}

# Optimization
OPT_LEVEL=${OPT_LEVEL:-not set}
LTO=${LTO:-not set}
CODEGEN_UNITS=${CODEGEN_UNITS:-not set}

# Debug Info
DEBUG_INFO=${DEBUG_INFO:-not set}
STRIP=${STRIP:-not set}

# Linker
LINKER=${LINKER:-not set}
LINK_ARG=${LINK_ARG:-not set}
EOF

# Create consolidated environment report
echo "🔍 Creating consolidated environment report..."
cat > target/environment/environment_report.json << EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "source_date_epoch": "${SOURCE_DATE_EPOCH:-null}",
  "ci": "${CI:-false}",
  "platform": "${PLATFORM:-unknown}",
  "rustc_version": "$(rustc --version 2>/dev/null || echo unknown)",
  "cargo_version": "$(cargo --version 2>/dev/null || echo unknown)",
  "metal_sdk_version": "$(xcrun --show-sdk-version 2>/dev/null || echo unknown)",
  "macos_version": "$(sw_vers -productVersion 2>/dev/null || echo unknown)",
  "architecture": "$(uname -m 2>/dev/null || echo unknown)",
  "cpu_brand": "$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo unknown)",
  "environment_variables": {
    "SOURCE_DATE_EPOCH": "${SOURCE_DATE_EPOCH:-null}",
    "CARGO_INCREMENTAL": "${CARGO_INCREMENTAL:-null}",
    "RUSTC_WRAPPER": "${RUSTC_WRAPPER:-null}",
    "CARGO_TARGET_DIR": "${CARGO_TARGET_DIR:-null}",
    "RUSTFLAGS": "${RUSTFLAGS:-null}",
    "CARGO_BUILD_RUSTFLAGS": "${CARGO_BUILD_RUSTFLAGS:-null}",
    "CARGO_BUILD_TARGET": "${CARGO_BUILD_TARGET:-null}",
    "CARGO_BUILD_TARGET_DIR": "${CARGO_BUILD_TARGET_DIR:-null}",
    "CARGO_BUILD_JOBS": "${CARGO_BUILD_JOBS:-null}",
    "CI": "${CI:-null}",
    "GITHUB_ACTIONS": "${GITHUB_ACTIONS:-null}",
    "GITHUB_WORKFLOW": "${GITHUB_WORKFLOW:-null}",
    "GITHUB_RUN_ID": "${GITHUB_RUN_ID:-null}",
    "GITHUB_SHA": "${GITHUB_SHA:-null}",
    "PLATFORM": "${PLATFORM:-null}",
    "ARCH": "${ARCH:-null}",
    "OS": "${OS:-null}"
  }
}
EOF

echo "✅ Environment variables recorded in target/environment/"
echo "   Files:"
ls -la target/environment/
