#!/bin/bash
# =============================================================================
# Dev Flag Release Guard
# =============================================================================
#
# This script guards against the scenario where:
#   - Dev bypass flags are set (AOS_DEV_NO_AUTH=1, etc.)
#   - But a release binary is being used
#
# Release binaries reject dev flags as a security measure, causing startup
# failure. This guard catches the problem early in CI or local dev.
#
# Usage:
#   ./scripts/ci/dev_flag_release_guard.sh [--check-only]
#
# Exit codes:
#   0 - No conflict (safe to proceed)
#   1 - Dev flags set but only release binary exists (would fail at runtime)
#   2 - Script error
#
# Environment:
#   DEV_BYPASS_FLAGS can be extended in service-manager.sh
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$(dirname "$SCRIPT_DIR")")"

# Dev bypass flags (same list as service-manager.sh)
DEV_BYPASS_FLAGS=(
    "AOS_DEV_NO_AUTH"
    "AOS_DEV_SKIP_METALLIB_CHECK"
    "AOS_DEV_SKIP_DRIFT_CHECK"
)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Load .env if it exists (to detect flags that would be set at runtime)
load_env() {
    if [[ -f "$PROJECT_ROOT/.env" ]]; then
        set -a
        while IFS= read -r line || [[ -n "$line" ]]; do
            [[ -z "$line" || "$line" =~ ^[[:space:]]*# ]] && continue
            if [[ "$line" =~ ^[^#]*= ]]; then
                var_name="${line%%=*}"
                if [[ -z "${!var_name:-}" ]]; then
                    eval "export $line" 2>/dev/null || true
                fi
            fi
        done < "$PROJECT_ROOT/.env"
        set +a
    fi
}

# Check if any dev bypass flags are enabled
get_active_dev_flags() {
    local active=()
    for flag in "${DEV_BYPASS_FLAGS[@]}"; do
        local val="${!flag:-}"
        if [[ "$val" == "1" || "$val" == "true" || "$val" == "yes" ]]; then
            active+=("$flag")
        fi
    done
    echo "${active[*]}"
}

main() {
    local check_only=0
    if [[ "${1:-}" == "--check-only" ]]; then
        check_only=1
    fi

    # Load environment
    load_env

    local active_flags
    active_flags=$(get_active_dev_flags)

    local debug_server="$PROJECT_ROOT/target/debug/adapteros-server"
    local release_server="$PROJECT_ROOT/target/release/adapteros-server"

    local has_debug=0
    local has_release=0
    [[ -f "$debug_server" ]] && has_debug=1
    [[ -f "$release_server" ]] && has_release=1

    # Scenario 1: No dev flags - no problem
    if [[ -z "$active_flags" ]]; then
        echo -e "${GREEN}[PASS]${NC} No dev bypass flags active - any binary is safe"
        exit 0
    fi

    # Scenario 2: Dev flags set, debug binary exists - OK
    if [[ $has_debug -eq 1 ]]; then
        echo -e "${GREEN}[PASS]${NC} Dev flags ($active_flags) will use debug binary"
        exit 0
    fi

    # Scenario 3: Dev flags set, only release binary - PROBLEM
    if [[ $has_release -eq 1 ]]; then
        echo -e "${RED}[FAIL]${NC} Dev bypass flags are set but only release binary exists!"
        echo ""
        echo "  Active flags: $active_flags"
        echo "  Release binary: $release_server"
        echo ""
        echo "  Release binaries reject dev bypass flags for security."
        echo "  This configuration would fail at runtime with:"
        echo "    SECURITY VIOLATION: Dev bypass flags [...] in release build"
        echo ""
        echo "  Fix options:"
        echo "    1. Build debug binary:  cargo build -p adapteros-server"
        echo "    2. Remove dev flags from .env (for production use)"
        echo ""
        exit 1
    fi

    # Scenario 4: No binaries at all
    echo -e "${YELLOW}[WARN]${NC} No server binaries found (dev flags: $active_flags)"
    echo "  Build with: cargo build -p adapteros-server"
    exit 0
}

main "$@"
