#!/usr/bin/env bash
# Shared build-target policy and cache guardrails for adapterOS scripts.

# Return 0 for truthy values, 1 otherwise.
aos_is_truthy() {
    local raw="${1:-}"
    local val
    val="$(echo "$raw" | tr '[:upper:]' '[:lower:]')"
    case "$val" in
        1|true|yes|on) return 0 ;;
        *) return 1 ;;
    esac
}

# Resolve repository root from known script conventions.
aos_repo_root() {
    if [ -n "${ROOT_DIR:-}" ] && [ -f "${ROOT_DIR}/Cargo.toml" ]; then
        echo "$ROOT_DIR"
        return 0
    fi
    if [ -n "${ROOT:-}" ] && [ -f "${ROOT}/Cargo.toml" ]; then
        echo "$ROOT"
        return 0
    fi

    local lib_dir
    lib_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    echo "$(cd "${lib_dir}/../.." && pwd)"
}

# Normalize a path to absolute by anchoring relative values to repo root.
aos_abs_path() {
    local raw_path="$1"
    if [[ "$raw_path" == /* ]]; then
        echo "$raw_path"
        return 0
    fi
    echo "$(aos_repo_root)/$raw_path"
}

# Root path for build targets.
# Default is the canonical workspace target dir to avoid split caches/locks.
aos_build_target_root() {
    local root_override="${AOS_BUILD_TARGET_ROOT:-}"
    if [ -n "$root_override" ]; then
        aos_abs_path "$root_override"
        return 0
    fi
    echo "$(aos_repo_root)/target"
}

# Resolve target dir for a logical build flow.
# By default all flows share one target root; explicit env overrides can diverge.
aos_target_dir_for_flow() {
    local flow="${1:-}"
    local override=""
    case "$flow" in
        ui) override="${AOS_UI_TARGET_DIR:-}" ;;
        server) override="${AOS_SERVER_TARGET_DIR:-}" ;;
        worker) override="${AOS_WORKER_TARGET_DIR:-}" ;;
        test) override="${AOS_TEST_TARGET_DIR:-}" ;;
        *) ;;
    esac

    if [ -n "$override" ]; then
        aos_abs_path "$override"
        return 0
    fi

    local root
    root="$(aos_build_target_root)"
    echo "$root"
}

# Export CARGO_TARGET_DIR for a flow and ensure directory exists.
aos_export_cargo_target() {
    local flow="${1:-}"
    local target_dir
    target_dir="$(aos_target_dir_for_flow "$flow")"
    mkdir -p "$target_dir"
    export CARGO_TARGET_DIR="$target_dir"
    echo "$target_dir"
}

# Resolve a binary from flow-partitioned and legacy target roots.
# Fallback order:
#   1) <flow-target>/<profile>/<binary>
#   2) <repo>/target/<profile>/<binary>
aos_resolve_binary() {
    local binary_name="$1"
    local profile="${2:-debug}"
    local flow="${3:-}"

    local flow_target
    flow_target="$(aos_target_dir_for_flow "$flow")"
    local repo_root
    repo_root="$(aos_repo_root)"

    local candidates=(
        "${flow_target}/${profile}/${binary_name}"
        "${repo_root}/target/${profile}/${binary_name}"
    )

    local candidate
    for candidate in "${candidates[@]}"; do
        if [ -x "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done

    # Default to the flow candidate for caller error messages.
    echo "${candidates[0]}"
    return 1
}

# Return a human-readable sccache mode summary.
aos_sccache_mode() {
    if ! aos_is_truthy "${AOS_BUILD_USE_SCCACHE:-1}"; then
        echo "disabled (AOS_BUILD_USE_SCCACHE=0)"
        return 0
    fi

    if [ "${RUSTC_WRAPPER+x}" = "x" ] && [ -z "${RUSTC_WRAPPER}" ]; then
        echo "disabled (RUSTC_WRAPPER=)"
        return 0
    fi

    if [ -n "${RUSTC_WRAPPER:-}" ]; then
        if [[ "$RUSTC_WRAPPER" == *"sccache"* ]]; then
            echo "enabled (RUSTC_WRAPPER=${RUSTC_WRAPPER})"
        else
            echo "custom-wrapper (RUSTC_WRAPPER=${RUSTC_WRAPPER})"
        fi
        return 0
    fi

    if command -v sccache >/dev/null 2>&1; then
        echo "enabled (cargo-config default)"
    else
        echo "disabled (sccache not installed)"
    fi
}

# Print effective build context for script observability.
aos_print_build_context() {
    local flow="${1:-}"
    local target_dir
    target_dir="$(aos_target_dir_for_flow "$flow")"
    echo "Build context: flow=${flow} target_dir=${target_dir} sccache=$(aos_sccache_mode)"
}

# Execute a build command with script-level sccache policy.
aos_run_build_command() {
    if aos_is_truthy "${AOS_BUILD_USE_SCCACHE:-1}"; then
        "$@"
    else
        # Explicit opt-out path for reproducible local troubleshooting.
        RUSTC_WRAPPER= "$@"
    fi
}

# List incremental directories under a target root.
aos_incremental_dirs_for_target_dir() {
    local target_dir="$1"
    if [ ! -d "$target_dir" ]; then
        return 0
    fi
    find "$target_dir" -type d -name incremental -prune 2>/dev/null | sort -u
}

# Cross-platform mtime fetch.
aos_stat_mtime() {
    local path="$1"
    stat -f '%m' "$path" 2>/dev/null || stat -c '%Y' "$path" 2>/dev/null || echo "0"
}

# Internal: evaluate and optionally prune incremental cache under a target dir.
# Returns 0 if healthy, 1 if warning/prune condition triggered.
aos_prune_incremental_for_target_dir() {
    local target_dir="$1"
    local label="${2:-target}"

    local warn_gb="${AOS_INCREMENTAL_WARN_GB:-6}"
    local prune_gb="${AOS_INCREMENTAL_PRUNE_GB:-10}"
    local max_age_hours="${AOS_INCREMENTAL_MAX_AGE_HOURS:-72}"
    local auto_prune="${AOS_AUTO_PRUNE_INCREMENTAL:-1}"

    [[ "$warn_gb" =~ ^[0-9]+$ ]] || warn_gb=6
    [[ "$prune_gb" =~ ^[0-9]+$ ]] || prune_gb=10
    [[ "$max_age_hours" =~ ^[0-9]+$ ]] || max_age_hours=72

    local dirs=()
    local dir
    while IFS= read -r dir; do
        [ -n "$dir" ] && dirs+=("$dir")
    done < <(aos_incremental_dirs_for_target_dir "$target_dir")

    if [ "${#dirs[@]}" -eq 0 ]; then
        return 0
    fi

    local total_kb=0
    local newest_mtime=0
    local kb=0
    local mtime=0
    for dir in "${dirs[@]}"; do
        kb=$(du -sk "$dir" 2>/dev/null | awk '{print $1}')
        kb=${kb:-0}
        total_kb=$((total_kb + kb))

        mtime=$(aos_stat_mtime "$dir")
        mtime=${mtime:-0}
        if [ "$mtime" -gt "$newest_mtime" ]; then
            newest_mtime="$mtime"
        fi
    done

    local now age_hours
    now=$(date +%s)
    if [ "$newest_mtime" -gt 0 ]; then
        age_hours=$(((now - newest_mtime) / 3600))
    else
        age_hours=0
    fi

    local size_gb=$((total_kb / 1024 / 1024))
    local warn_trigger=0
    if [ "$size_gb" -ge "$warn_gb" ] || [ "$age_hours" -ge "$max_age_hours" ]; then
        warn_trigger=1
        echo "WARNING: incremental cache (${label}) size=${size_gb}GB age=${age_hours}h (warn=${warn_gb}GB age=${max_age_hours}h)"
    fi

    if aos_is_truthy "$auto_prune" && { [ "$size_gb" -ge "$prune_gb" ] || [ "$age_hours" -ge "$max_age_hours" ]; }; then
        local removed=0
        for dir in "${dirs[@]}"; do
            rm -rf "$dir"
            removed=$((removed + 1))
        done
        echo "Pruned ${removed} incremental cache director$( [ "$removed" -eq 1 ] && echo "y" || echo "ies" ) for ${label}"
        return 1
    fi

    return "$warn_trigger"
}

# Evaluate and optionally prune incremental cache for a flow target.
aos_prune_incremental_if_needed() {
    local flow="${1:-}"
    local target_dir
    target_dir="$(aos_target_dir_for_flow "$flow")"
    aos_prune_incremental_for_target_dir "$target_dir" "$flow"
}
