#!/bin/bash
# Check if Cargo incremental cache is stale (older than 48 hours)
# Warns user to clean cache to prevent disk bloat

set -euo pipefail

CACHE_DIR="${CARGO_TARGET_DIR:-target}/debug/incremental"
MAX_AGE_HOURS="${AOS_CACHE_MAX_AGE_HOURS:-48}"
MAX_AGE_SECONDS=$((MAX_AGE_HOURS * 3600))

# Colors
YELLOW='\033[0;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

warn() {
    echo -e "${YELLOW}⚠️  WARNING:${NC} $1" >&2
}

error() {
    echo -e "${RED}❌ ERROR:${NC} $1" >&2
}

check_cache_age() {
    if [[ ! -d "$CACHE_DIR" ]]; then
        return 0
    fi

    # Find oldest file in incremental cache
    local oldest_file
    oldest_file=$(find "$CACHE_DIR" -type f -print0 2>/dev/null | xargs -0 stat -f '%m %N' 2>/dev/null | sort -n | head -1 | cut -d' ' -f2-)

    if [[ -z "$oldest_file" ]]; then
        return 0
    fi

    local file_age
    file_age=$(stat -f '%m' "$oldest_file" 2>/dev/null || echo "0")
    local now
    now=$(date +%s)
    local age_seconds=$((now - file_age))
    local age_hours=$((age_seconds / 3600))

    if [[ $age_seconds -gt $MAX_AGE_SECONDS ]]; then
        local cache_size
        cache_size=$(du -sh "$CACHE_DIR" 2>/dev/null | cut -f1)

        warn "Cargo incremental cache is ${age_hours}h old (threshold: ${MAX_AGE_HOURS}h)"
        warn "Cache size: ${cache_size}"
        warn "Run 'cargo clean' or 'rm -rf target/debug/incremental' to free space"
        echo ""
        return 1
    fi

    return 0
}

check_cache_size() {
    if [[ ! -d "$CACHE_DIR" ]]; then
        return 0
    fi

    # Get size in bytes (macOS du -k gives KB)
    local size_kb
    size_kb=$(du -sk "$CACHE_DIR" 2>/dev/null | cut -f1)
    local size_gb=$((size_kb / 1024 / 1024))

    # Warn if cache exceeds 50GB. This script is informational: callers may
    # surface the message without failing the overall boot, so keep wording
    # non-alarming for demo/dev workflows.
    if [[ $size_gb -gt 50 ]]; then
        warn "Incremental cache is ${size_gb}GB - consider running 'cargo clean'"
        return 1
    elif [[ $size_gb -gt 20 ]]; then
        warn "Incremental cache is ${size_gb}GB"
    fi

    return 0
}

main() {
    local exit_code=0

    check_cache_age || exit_code=1
    check_cache_size || exit_code=1

    return $exit_code
}

# Only run if executed directly (not sourced)
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
