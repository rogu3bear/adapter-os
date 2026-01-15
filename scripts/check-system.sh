#!/usr/bin/env bash
# Quick system readiness checker for adapterOS
#
# Runs the preflight command and provides helpful output
#
# Copyright: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

set -euo pipefail

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo -e "${BLUE}╔═══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║           adapterOS System Readiness Check                   ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Check if aosctl is built
AOSCTL_BIN="$REPO_ROOT/target/release/aosctl"
AOSCTL_DEBUG="$REPO_ROOT/target/debug/aosctl"

if [ -f "$AOSCTL_BIN" ]; then
    CLI="$AOSCTL_BIN"
elif [ -f "$AOSCTL_DEBUG" ]; then
    CLI="$AOSCTL_DEBUG"
    echo -e "${YELLOW}⚠ Using debug build (slower). Build release version:${NC}"
    echo -e "${YELLOW}  cargo build --release -p adapteros-cli${NC}"
    echo ""
else
    echo -e "${YELLOW}→ Building CLI tool...${NC}"
    cd "$REPO_ROOT"
    if ! cargo build --release -p adapteros-cli 2>&1 | grep -E "(Finished|error)" ; then
        echo -e "${RED}✗ Failed to build CLI tool${NC}"
        echo ""
        echo -e "${BLUE}Try building manually:${NC}"
        echo "  cargo build --release -p adapteros-cli"
        exit 1
    fi
    CLI="$AOSCTL_BIN"
    echo ""
fi

# Run preflight check
if "$CLI" preflight "$@"; then
    echo ""
    echo -e "${GREEN}╔═══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║  ✅ System Ready - You can launch the server!                ║${NC}"
    echo -e "${GREEN}╚═══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${BLUE}Next steps:${NC}"
    echo "  1. Start server: ${YELLOW}cargo run --release -p adapteros-server-api${NC}"
    echo "  2. Or use UI:    ${YELLOW}cd crates/adapteros-ui && trunk build --release${NC}"
    echo "  3. Check health: ${YELLOW}aosctl doctor${NC}"
    echo ""
    exit 0
else
    EXIT_CODE=$?
    echo ""
    echo -e "${RED}╔═══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${RED}║  ❌ System Not Ready - Please fix issues above                ║${NC}"
    echo -e "${RED}╚═══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${BLUE}Common fixes:${NC}"
    echo "  Download model:     ${YELLOW}./scripts/download-model.sh${NC}"
    echo "  Initialize DB:      ${YELLOW}cargo run -p adapteros-cli -- db migrate${NC}"
    echo "  Install Xcode CLI:  ${YELLOW}xcode-select --install${NC}"
    echo ""
    echo -e "${BLUE}Re-run when fixed:${NC}"
    echo "  ${YELLOW}./scripts/check-system.sh${NC}"
    echo ""
    exit $EXIT_CODE
fi
