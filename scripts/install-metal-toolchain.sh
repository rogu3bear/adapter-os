#!/usr/bin/env bash
# Install Metal Toolchain for AdapterOS
# Part of H1: Metal Kernel Compilation task
# Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

TMP_ROOT="${AOS_VAR_DIR:-${REPO_ROOT}/var}/tmp"
if [[ "$TMP_ROOT" == /tmp* || "$TMP_ROOT" == /private/tmp* ]]; then
    echo -e "${RED}Error: Refusing temporary directory under /tmp: ${TMP_ROOT}${NC}"
    exit 1
fi
mkdir -p "${TMP_ROOT}"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}AdapterOS Metal Toolchain Installer${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Check if running on macOS
if [[ "$(uname)" != "Darwin" ]]; then
    echo -e "${RED}Error: This script must be run on macOS${NC}"
    exit 1
fi

# Check if Xcode is installed
if ! command -v xcodebuild &> /dev/null; then
    echo -e "${RED}Error: Xcode Command Line Tools not installed${NC}"
    echo -e "${YELLOW}Install with: xcode-select --install${NC}"
    exit 1
fi

# Display Xcode version
XCODE_VERSION=$(xcodebuild -version | head -n 1)
echo -e "${GREEN}✓ Xcode installed: ${XCODE_VERSION}${NC}"

# Check if Metal compiler is available
if xcrun --find metal &> /dev/null; then
    METAL_PATH=$(xcrun --find metal)
    echo -e "${GREEN}✓ Metal compiler found: ${METAL_PATH}${NC}"
else
    echo -e "${RED}Error: Metal compiler not found${NC}"
    exit 1
fi

# Check if Metal Toolchain is already installed
echo ""
echo -e "${BLUE}Checking Metal Toolchain installation...${NC}"

# Test Metal compilation with a simple shader
TEST_DIR="$(mktemp -d "${TMP_ROOT}/metal-toolchain.XXXXXX")"
TEST_METAL="${TEST_DIR}/test.metal"
TEST_AIR="${TEST_DIR}/test.air"

cat > "${TEST_METAL}" << 'EOF'
#include <metal_stdlib>
using namespace metal;

kernel void test_kernel(device float* data [[buffer(0)]]) {
    uint tid = threadgroup_position_in_grid.x;
    data[tid] = 1.0f;
}
EOF

# Try to compile the test shader
if xcrun -sdk macosx metal -c "${TEST_METAL}" -o "${TEST_AIR}" 2>/dev/null; then
    echo -e "${GREEN}✓ Metal Toolchain is already installed${NC}"
    rm -rf "${TEST_DIR}"
    echo ""
    echo -e "${GREEN}========================================${NC}"
    echo -e "${GREEN}Metal Toolchain is ready!${NC}"
    echo -e "${GREEN}========================================${NC}"
    exit 0
else
    echo -e "${YELLOW}⚠ Metal Toolchain not installed${NC}"
fi

rm -rf "${TEST_DIR}"

# Install Metal Toolchain
echo ""
echo -e "${BLUE}Installing Metal Toolchain...${NC}"
echo -e "${YELLOW}This may take several minutes and requires internet access${NC}"
echo ""

# Download and install Metal Toolchain
if xcodebuild -downloadComponent MetalToolchain; then
    echo ""
    echo -e "${GREEN}✓ Metal Toolchain installed successfully${NC}"
else
    echo ""
    echo -e "${RED}✗ Failed to install Metal Toolchain${NC}"
    echo -e "${YELLOW}Manual installation steps:${NC}"
    echo -e "  1. Open Xcode"
    echo -e "  2. Go to Preferences > Components"
    echo -e "  3. Install 'Metal Toolchain'"
    echo -e "${YELLOW}Or run manually:${NC}"
    echo -e "  xcodebuild -downloadComponent MetalToolchain"
    exit 1
fi

# Verify installation
echo ""
echo -e "${BLUE}Verifying Metal Toolchain installation...${NC}"

TEST_DIR="$(mktemp -d "${TMP_ROOT}/metal-toolchain-verify.XXXXXX")"
TEST_METAL="${TEST_DIR}/verify.metal"
TEST_AIR="${TEST_DIR}/verify.air"

cat > "${TEST_METAL}" << 'EOF'
#include <metal_stdlib>
using namespace metal;

kernel void verify_kernel(device float* data [[buffer(0)]]) {
    uint tid = threadgroup_position_in_grid.x;
    data[tid] = 42.0f;
}
EOF

if xcrun -sdk macosx metal -c "${TEST_METAL}" -o "${TEST_AIR}" 2>&1; then
    echo -e "${GREEN}✓ Metal Toolchain verification successful${NC}"
    rm -rf "${TEST_DIR}"
else
    echo -e "${RED}✗ Metal Toolchain verification failed${NC}"
    rm -rf "${TEST_DIR}"
    exit 1
fi

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Installation Complete!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "${BLUE}Next steps:${NC}"
echo -e "  1. Build AdapterOS: ${YELLOW}cargo build${NC}"
echo -e "  2. Test Metal kernels: ${YELLOW}cargo test -p adapteros-lora-kernel-mtl${NC}"
echo ""
echo -e "${BLUE}For more information, see:${NC}"
echo -e "  ${YELLOW}docs/METAL_TOOLCHAIN_SETUP.md${NC}"
echo ""
