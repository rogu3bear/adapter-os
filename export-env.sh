#!/bin/bash
# Export adapterOS environment variables from .env file with validation
# Usage: source ./export-env.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Source unified environment loader
source scripts/lib/env-loader.sh

# Check if .env exists
if ! check_env_file ".env"; then
    exit 1
fi

# Load .env file
load_env_file ".env" --no-override

# Validate configuration
echo -e "${FG_CYAN}Validating environment configuration...${FG_RESET}"
echo ""

VALIDATION_ERRORS=0
validate_env_config || VALIDATION_ERRORS=$?

# Summary
echo ""
if [ $VALIDATION_ERRORS -eq 0 ]; then
    echo -e "${FG_GREEN}✓ Environment variables validated successfully${FG_RESET}"
    echo ""
    print_env_summary
else
    echo -e "${FG_RED}✗ Validation failed with $VALIDATION_ERRORS error(s)${FG_RESET}"
    echo "  Fix the issues above before starting adapterOS"
    return 1 2>/dev/null || exit 1
fi
