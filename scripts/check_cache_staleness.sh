#!/bin/bash
# Check and optionally prune Cargo incremental cache paths.
# Supports both legacy target/ and flow-partitioned target roots.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/lib/build-targets.sh"

# Backward compatibility for existing callers.
if [ -n "${AOS_CACHE_MAX_AGE_HOURS:-}" ] && [ -z "${AOS_INCREMENTAL_MAX_AGE_HOURS:-}" ]; then
    export AOS_INCREMENTAL_MAX_AGE_HOURS="$AOS_CACHE_MAX_AGE_HOURS"
fi

MAX_AGE_HOURS="${AOS_INCREMENTAL_MAX_AGE_HOURS:-72}"
WARN_GB="${AOS_INCREMENTAL_WARN_GB:-6}"
PRUNE_GB="${AOS_INCREMENTAL_PRUNE_GB:-10}"
AUTO_PRUNE="${AOS_AUTO_PRUNE_INCREMENTAL:-1}"

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

TARGET_LABELS=()
TARGET_DIRS=()

append_target() {
    local label="$1"
    local dir="$2"

    [ -n "$dir" ] || return 0
    dir="$(aos_abs_path "$dir")"

    local i
    for i in "${!TARGET_DIRS[@]}"; do
        if [ "${TARGET_DIRS[$i]}" = "$dir" ]; then
            return 0
        fi
    done

    TARGET_LABELS+=("$label")
    TARGET_DIRS+=("$dir")
}

collect_targets() {
    if [ -n "${CARGO_TARGET_DIR:-}" ]; then
        append_target "env:CARGO_TARGET_DIR" "$CARGO_TARGET_DIR"
    fi

    append_target "legacy" "$ROOT_DIR/target"
    append_target "flow-root" "$(aos_build_target_root)"
    append_target "flow-ui" "$(aos_target_dir_for_flow ui)"
    append_target "flow-server" "$(aos_target_dir_for_flow server)"
    append_target "flow-worker" "$(aos_target_dir_for_flow worker)"
    append_target "flow-test" "$(aos_target_dir_for_flow test)"
}

main() {
    local exit_code=0

    echo "Cache context: target_root=$(aos_build_target_root) sccache=$(aos_sccache_mode)"

    collect_targets

    if [ "${#TARGET_DIRS[@]}" -eq 0 ]; then
        return 0
    fi

    local i
    for i in "${!TARGET_DIRS[@]}"; do
        local label="${TARGET_LABELS[$i]}"
        local dir="${TARGET_DIRS[$i]}"

        if [ ! -d "$dir" ]; then
            continue
        fi

        if ! aos_prune_incremental_for_target_dir "$dir" "$label"; then
            warn "Incremental cache issue detected under $dir"
            warn "Thresholds: warn=${WARN_GB}GB prune=${PRUNE_GB}GB max-age=${MAX_AGE_HOURS}h auto-prune=${AUTO_PRUNE}"
            if ! aos_is_truthy "$AUTO_PRUNE"; then
                warn "Run: rm -rf '$dir'/debug/incremental '$dir'/*/debug/incremental"
            fi
            echo ""
            exit_code=1
        fi
    done

    return $exit_code
}

# Only run if executed directly (not sourced)
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
