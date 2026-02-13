#!/bin/bash
# adapterOS Service Manager
# Manages starting, stopping, and status of adapterOS services
#
# Copyright (c) 2025 JKCA / James KC Auchterlonie. All rights reserved.
#
# Usage:
#   ./scripts/service-manager.sh start <service>     Start a service (backend, worker, secd, node, ui)
#   ./scripts/service-manager.sh stop all [mode]    Stop all services (graceful|fast|immediate)
#   ./scripts/service-manager.sh status             Show status of all services
#
# Called by launch.sh for coordinated service management.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# =============================================================================
# Load Environment
# =============================================================================
# Load .env file if it exists (before configuration section).
# IMPORTANT: do not override environment variables already set by the caller
# (e.g., `./start --worker-manifest ...`), so dev overrides work deterministically.
if [ -f "$PROJECT_ROOT/scripts/lib/env-loader.sh" ]; then
    # shellcheck disable=SC1091
    source "$PROJECT_ROOT/scripts/lib/env-loader.sh"
    # env-loader normalizes relative paths relative to SCRIPT_DIR; in this repo,
    # the canonical base for runtime paths is the workspace root (PROJECT_ROOT).
    export SCRIPT_DIR="$PROJECT_ROOT"
    load_env_file "$PROJECT_ROOT/.env" --no-override || true
elif [ -f "$PROJECT_ROOT/.env" ]; then
    # Fallback: best-effort source, without overriding already-set vars.
    set -a
    while IFS= read -r line || [[ -n "$line" ]]; do
        [[ -z "$line" || "$line" =~ ^[[:space:]]*# ]] && continue
        if [[ "$line" =~ ^[^#]*= ]]; then
            var_name="${line%%=*}"
            var_name="${var_name// /}"
            if [[ "$var_name" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
                eval "existing_value=\${$var_name:-}"
                if [ -z "${existing_value:-}" ]; then
                    eval "export $line" 2>/dev/null || true
                fi
            fi
        fi
    done < "$PROJECT_ROOT/.env"
    set +a
fi

# =============================================================================
# Configuration
# =============================================================================

# PID file locations
PID_DIR="$PROJECT_ROOT/var"
BACKEND_PID_FILE="$PID_DIR/backend.pid"
UI_PID_FILE="$PID_DIR/ui.pid"
WORKER_PID_FILE="$PID_DIR/worker.pid"
SECD_PID_FILE="$PID_DIR/secd.pid"
NODE_PID_FILE="$PID_DIR/node.pid"
QUARANTINE_DIR="$PID_DIR/quarantine"

# Log file locations
LOG_DIR="$PROJECT_ROOT/var/logs"
BACKEND_LOG="$LOG_DIR/backend.log"
UI_LOG="$LOG_DIR/ui.log"
WORKER_LOG="$LOG_DIR/worker.log"
SECD_LOG="$LOG_DIR/secd.log"
NODE_LOG="$LOG_DIR/node.log"
SCRIPT_LOG="$LOG_DIR/service-manager.log"

# Socket paths
SECD_SOCKET="$PROJECT_ROOT/var/run/aos-secd.sock"

# Port configuration for node
NODE_PORT="${AOS_NODE_PORT:-9443}"

# Canonical dev model (single source of truth)
DEFAULT_MODEL_DIR="/var/models/Llama-3.2-3B-Instruct-4bit"
DEFAULT_MANIFEST_PATH="$PROJECT_ROOT/manifests/mistral7b-4bit-mlx.yaml"
# Manifest hash is loaded from .env (DEFAULT_MANIFEST_HASH or AOS_MANIFEST_HASH)
# No hardcoded fallback - .env is the single source of truth for the hash value
DEFAULT_MANIFEST_HASH="${DEFAULT_MANIFEST_HASH:-${AOS_MANIFEST_HASH:-}}"

# Worker database tracking
WORKER_ID_FILE="$PID_DIR/worker.id"
DATABASE_PATH="${AOS_DATABASE_URL:-sqlite://var/aos-cp.sqlite3}"
# Extract SQLite path from DATABASE_URL if it's a sqlite:// URL
if [[ "$DATABASE_PATH" == sqlite://* ]]; then
    DATABASE_PATH="${DATABASE_PATH#sqlite://}"
fi
# If still relative, make it absolute relative to PROJECT_ROOT
if [[ "$DATABASE_PATH" != /* ]]; then
    DATABASE_PATH="$PROJECT_ROOT/$DATABASE_PATH"
fi

# Port configuration
BACKEND_PORT="${AOS_SERVER_PORT:-8080}"
UI_PORT="${AOS_UI_PORT:-3200}"

# Timeouts (in seconds)
GRACEFUL_TIMEOUT=120
FAST_TIMEOUT=30
FORCE_TIMEOUT=10
UI_TIMEOUT=15
WORKER_TIMEOUT=60
SECD_TIMEOUT=30
NODE_TIMEOUT=30

# =============================================================================
# Colors
# =============================================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
WHITE='\033[1;37m'
NC='\033[0m' # No Color

log_line() {
    local level="$1"; shift
    local msg="$*"
    mkdir -p "$LOG_DIR"
    local ts
    ts=$(date -Iseconds)
    local line="[$ts] [$level] [service-manager] $msg"
    echo -e "$line" >>"$SCRIPT_LOG"
}

# Rotate a log file to .prev if it exceeds a size threshold (bytes).
# Usage: rotate_log_if_large <file> <max_bytes>
rotate_log_if_large() {
    local file="$1"
    local max_bytes="$2"
    [ -f "$file" ] || return 0
    local size
    size=$(wc -c < "$file" 2>/dev/null | tr -d ' ')
    if [ -n "$size" ] && [ "$size" -gt "$max_bytes" ]; then
        mv "$file" "${file}.prev"
    fi
}

# Rotate a log file to .prev if it is non-empty.
# Usage: rotate_log <file>
rotate_log() {
    [ -s "$1" ] && mv "$1" "${1}.prev"
}

# =============================================================================
# Helper Functions
# =============================================================================

status_msg() {
    log_line INFO "$1"
    echo -e "${BLUE}[INFO]${NC} ${1}"
}

success_msg() {
    log_line INFO "$1"
    echo -e "${GREEN}[OK]${NC} ${1}"
}

warning_msg() {
    log_line WARN "$1"
    echo -e "${YELLOW}[WARN]${NC} ${1}"
}

error_msg() {
    log_line ERROR "$1"
    echo -e "${RED}[ERROR]${NC} ${1}"
}

# Shared port guard
PORT_GUARD_SCRIPT="$PROJECT_ROOT/scripts/port-guard.sh"
if [ -f "$PORT_GUARD_SCRIPT" ]; then
    # shellcheck disable=SC1090
    source "$PORT_GUARD_SCRIPT"
else
    warning_msg "Port guard script missing at $PORT_GUARD_SCRIPT; port cleanup will be manual."
    ensure_port_free() { return 0; }
fi

# Ensure directories exist
ensure_dirs() {
    mkdir -p "$PID_DIR"
    mkdir -p "$LOG_DIR"
    mkdir -p "$QUARANTINE_DIR"
    mkdir -p "$PROJECT_ROOT/var/tmp"
}

quarantine_artifact() {
    local path="$1"
    local reason="$2"
    ensure_dirs
    local ts
    ts=$(date +%s)
    local dest="$QUARANTINE_DIR/$(basename "$path").$ts"
    mv "$path" "$dest" 2>/dev/null || rm -f "$path"
    warning_msg "Quarantined $path ($reason) -> $dest"
}

cleanup_stale_artifacts() {
    local pid_files=("$BACKEND_PID_FILE" "$UI_PID_FILE" "$WORKER_PID_FILE")
    for pf in "${pid_files[@]}"; do
        if [ -f "$pf" ]; then
            local pid
            pid=$(cat "$pf" 2>/dev/null)
            if [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
                quarantine_artifact "$pf" "stale-pid"
            fi
        fi
    done
}

# =============================================================================
# Preflight Checks
# =============================================================================

# Check disk space (requires at least 10GB free by default)
# Usage: check_disk_space [min_gb]
check_disk_space() {
    local min_gb="${1:-10}"
    local target_dir="${PROJECT_ROOT}/var"

    # Create target dir if it doesn't exist
    mkdir -p "$target_dir"

    # Get available space in GB (macOS compatible)
    local available_gb
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS: df outputs in 512-byte blocks by default, use -g for GB
        available_gb=$(df -g "$target_dir" 2>/dev/null | tail -1 | awk '{print $4}')
    else
        # Linux: df -BG outputs in GB
        available_gb=$(df -BG "$target_dir" 2>/dev/null | tail -1 | awk '{print $4}' | tr -d 'G')
    fi

    if [ -z "$available_gb" ]; then
        warning_msg "Could not determine disk space; skipping check"
        return 0
    fi

    if [ "$available_gb" -lt "$min_gb" ]; then
        error_msg "Insufficient disk space: ${available_gb}GB available, ${min_gb}GB required"
        return 1
    fi

    status_msg "Disk space check passed: ${available_gb}GB available (>= ${min_gb}GB required)"
    return 0
}

# Check available memory (requires at least 8GB by default)
# Usage: check_memory [min_gb]
check_memory() {
    local min_gb="${1:-8}"
    local available_gb

    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS: Use vm_stat to get available memory (free + inactive + speculative + purgeable)
        local vm_stats
        vm_stats=$(vm_stat 2>/dev/null)
        local page_size
        page_size=$(printf '%s\n' "$vm_stats" | awk -F'page size of ' 'NR==1 {split($2,a," "); print a[1]}')
        if [ -z "$page_size" ]; then
            page_size=4096
        fi

        local pages_free
        local pages_inactive
        local pages_speculative
        local pages_purgeable
        pages_free=$(printf '%s\n' "$vm_stats" | awk '/Pages free/ {gsub("\\.","",$3); print $3}')
        pages_inactive=$(printf '%s\n' "$vm_stats" | awk '/Pages inactive/ {gsub("\\.","",$3); print $3}')
        pages_speculative=$(printf '%s\n' "$vm_stats" | awk '/Pages speculative/ {gsub("\\.","",$3); print $3}')
        pages_purgeable=$(printf '%s\n' "$vm_stats" | awk '/Pages purgeable/ {gsub("\\.","",$3); print $3}')

        if [ -n "$pages_free" ] && [ -n "$pages_inactive" ]; then
            pages_speculative=${pages_speculative:-0}
            pages_purgeable=${pages_purgeable:-0}
            local total_available=$((pages_free + pages_inactive + pages_speculative + pages_purgeable))
            available_gb=$((total_available * page_size / 1024 / 1024 / 1024))
        fi
    else
        # Linux: Use /proc/meminfo
        local available_kb=$(grep MemAvailable /proc/meminfo 2>/dev/null | awk '{print $2}')
        if [ -n "$available_kb" ]; then
            available_gb=$((available_kb / 1024 / 1024))
        fi
    fi

    if [ -z "$available_gb" ]; then
        warning_msg "Could not determine available memory; skipping check"
        return 0
    fi

    if [ "$available_gb" -lt "$min_gb" ]; then
        error_msg "Insufficient memory: ${available_gb}GB available, ${min_gb}GB required"
        return 1
    fi

    status_msg "Memory check passed: ${available_gb}GB available (>= ${min_gb}GB required)"
    return 0
}

# Check database integrity
# Usage: check_db_integrity
check_db_integrity() {
    if [ ! -f "$DATABASE_PATH" ]; then
        # Database doesn't exist yet; will be created on first run
        status_msg "Database not found at $DATABASE_PATH; will be created on first run"
        return 0
    fi

    # Run SQLite integrity check
    local result
    result=$(sqlite3 "$DATABASE_PATH" "PRAGMA integrity_check;" 2>&1)

    if [[ "$result" == "ok" ]]; then
        status_msg "Database integrity check passed"
        return 0
    else
        error_msg "Database integrity check failed: $result"
        error_msg "Database may be corrupted. Consider restoring from backup."
        return 1
    fi
}

check_bootstrap_state_hint() {
    if [ ! -f "$DATABASE_PATH" ]; then
        return 0
    fi

    if ! command -v sqlite3 >/dev/null 2>&1; then
        warning_msg "sqlite3 not found; skipping bootstrap state check"
        return 0
    fi

    local tenants plans nodes
    tenants=$(sqlite3 "$DATABASE_PATH" "SELECT COUNT(*) FROM tenants;" 2>/dev/null || true)
    plans=$(sqlite3 "$DATABASE_PATH" "SELECT COUNT(*) FROM plans;" 2>/dev/null || true)
    nodes=$(sqlite3 "$DATABASE_PATH" "SELECT COUNT(*) FROM nodes;" 2>/dev/null || true)

    if [[ ! "$tenants" =~ ^[0-9]+$ ]] || [[ ! "$plans" =~ ^[0-9]+$ ]] || [[ ! "$nodes" =~ ^[0-9]+$ ]]; then
        warning_msg "Bootstrap state check skipped (missing tables); run: aosctl db migrate"
        return 0
    fi

    if [ "$tenants" -eq 0 ] || [ "$plans" -eq 0 ] || [ "$nodes" -eq 0 ]; then
        warning_msg "Bootstrap state incomplete: tenants=$tenants plans=$plans nodes=$nodes"
        warning_msg "Run: aosctl db repair-bootstrap --dry-run"
    fi
}

# Run all preflight checks
# Usage: run_preflight_checks [--skip-disk] [--skip-memory] [--skip-db]
run_preflight_checks() {
    local skip_disk=false
    local skip_memory=false
    local skip_db=false

    for arg in "$@"; do
        case "$arg" in
            --skip-disk) skip_disk=true ;;
            --skip-memory) skip_memory=true ;;
            --skip-db) skip_db=true ;;
        esac
    done

    status_msg "Running preflight checks..."

    local failed=false

    if [ "$skip_disk" != "true" ]; then
        if ! check_disk_space 10; then
            failed=true
        fi
    fi

    if [ "$skip_memory" != "true" ]; then
        if ! check_memory 8; then
            failed=true
        fi
    fi

    if [ "$skip_db" != "true" ]; then
        if ! check_db_integrity; then
            failed=true
        else
            check_bootstrap_state_hint
        fi
    fi

    if [ "$failed" = "true" ]; then
        error_msg "Preflight checks failed. Set AOS_SKIP_PREFLIGHT=1 to bypass."
        return 1
    fi

    success_msg "All preflight checks passed"
    return 0
}

# Check if process is running by PID file
is_running() {
    local pid_file="$1"
    local expected_name="${2:-}"
    if [ -f "$pid_file" ]; then
        local pid=$(cat "$pid_file" 2>/dev/null)
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            if [ -n "$expected_name" ]; then
                local actual
                actual=$(ps -p "$pid" -o comm= 2>/dev/null | xargs basename 2>/dev/null || echo "")
                if [ -n "$actual" ] && [[ "$actual" != *"$expected_name"* ]]; then
                    rm -f "$pid_file"
                    return 1
                fi
            fi
            return 0
        else
            # Stale PID file, clean up
            rm -f "$pid_file"
            return 1
        fi
    fi
    return 1
}

# Get PID from file
get_pid() {
    local pid_file="$1"
    if [ -f "$pid_file" ]; then
        cat "$pid_file" 2>/dev/null
    fi
}

# Wait for process to stop with timeout
wait_for_stop() {
    local pid="$1"
    local timeout="$2"
    local start_time=$(date +%s)

    while [ $(($(date +%s) - start_time)) -lt "$timeout" ]; do
        if ! kill -0 "$pid" 2>/dev/null; then
            return 0
        fi
        sleep 1
    done

    return 1
}

# Stop a process gracefully with fallback to force kill
stop_process() {
    local pid="$1"
    local service_name="$2"
    local graceful_timeout="$3"
    local mode="${4:-graceful}"

    if [ -z "$pid" ] || ! kill -0 "$pid" 2>/dev/null; then
        return 0  # Already stopped
    fi

    case "$mode" in
        graceful)
            status_msg "Stopping $service_name (PID: $pid) gracefully..."
            if kill -TERM "$pid" 2>/dev/null; then
                if wait_for_stop "$pid" "$graceful_timeout"; then
                    success_msg "$service_name stopped gracefully"
                    return 0
                else
                    warning_msg "$service_name did not stop within ${graceful_timeout}s, forcing..."
                fi
            fi
            ;;
        fast)
            status_msg "Stopping $service_name (PID: $pid) quickly..."
            # Backend supports SIGUSR1 for fast shutdown
            if [ "$service_name" = "Backend" ]; then
                kill -USR1 "$pid" 2>/dev/null || true
                if wait_for_stop "$pid" "$FAST_TIMEOUT"; then
                    success_msg "$service_name stopped quickly"
                    return 0
                fi
            fi
            kill -TERM "$pid" 2>/dev/null || true
            if wait_for_stop "$pid" "$FAST_TIMEOUT"; then
                success_msg "$service_name stopped"
                return 0
            fi
            ;;
        immediate)
            status_msg "Stopping $service_name (PID: $pid) immediately..."
            # Backend supports SIGUSR2 for immediate shutdown
            if [ "$service_name" = "Backend" ]; then
                kill -USR2 "$pid" 2>/dev/null || true
                sleep 2
            fi
            ;;
    esac

    # Force kill if still running
    if kill -0 "$pid" 2>/dev/null; then
        kill -KILL "$pid" 2>/dev/null || true
        if wait_for_stop "$pid" "$FORCE_TIMEOUT"; then
            warning_msg "$service_name force stopped"
            return 0
        else
            error_msg "$service_name failed to stop"
            return 1
        fi
    fi

    return 0
}

# =============================================================================
# Dev Flag Detection
# =============================================================================

# List of environment variables that are development bypass flags.
# These flags must NOT be used with release binaries (which reject them).
DEV_BYPASS_FLAGS=(
    "AOS_DEV_NO_AUTH"
    "AOS_DEV_SKIP_METALLIB_CHECK"
    "AOS_DEV_SKIP_DRIFT_CHECK"
)

# Check if any dev bypass flags are enabled (set to 1 or true).
# Returns 0 (true) if dev mode, 1 (false) if prod mode.
is_dev_mode() {
    for flag in "${DEV_BYPASS_FLAGS[@]}"; do
        local val="${!flag:-}"
        if [[ "$val" == "1" || "$val" == "true" || "$val" == "yes" ]]; then
            return 0
        fi
    done
    return 1
}

# Get list of active dev flags (for logging/errors).
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

# Select the appropriate server binary based on dev flags.
# In dev mode (dev flags set): prefer debug binary (required for dev flags)
# In prod mode (no dev flags): prefer release binary (optimized)
#
# Returns: path to binary on stdout, exits with error if required binary missing.
# NOTE: Status messages go to stderr to keep stdout clean for command substitution.
select_server_binary() {
    local debug_bin="$PROJECT_ROOT/target/debug/aos-server"
    local release_bin="$PROJECT_ROOT/target/release/aos-server"

    if is_dev_mode; then
        local active_flags
        active_flags=$(get_active_dev_flags)
        if [ -f "$debug_bin" ]; then
            # Log to stderr so stdout only contains the path
            status_msg "Dev mode detected ($active_flags) → using debug binary" >&2
            echo "$debug_bin"
            return 0
        else
            error_msg "Dev bypass flags are set ($active_flags) but debug binary not found."
            error_msg "Release binaries reject dev flags for security. Build debug binary:"
            error_msg "  cargo build -p adapteros-server"
            error_msg ""
            error_msg "Or disable dev flags in .env to use release binary."
            return 1
        fi
    else
        # Prod mode: prefer release, fall back to debug
        if [ -f "$release_bin" ]; then
            echo "$release_bin"
            return 0
        elif [ -f "$debug_bin" ]; then
            # Log to stderr so stdout only contains the path
            warning_msg "No release binary found, using debug binary (slower)" >&2
            echo "$debug_bin"
            return 0
        else
            error_msg "No server binary found. Build with:"
            error_msg "  cargo build -p adapteros-server           # debug build"
            error_msg "  cargo build -p adapteros-server --release # release build"
            return 1
        fi
    fi
}

# Select the appropriate worker binary based on dev flags.
# Same logic as server binary selection.
# NOTE: Status messages go to stderr to keep stdout clean for command substitution.
select_worker_binary() {
    local debug_bin="$PROJECT_ROOT/target/debug/aos-worker"
    local release_bin="$PROJECT_ROOT/target/release/aos-worker"

    if is_dev_mode; then
        if [ -f "$debug_bin" ]; then
            echo "$debug_bin"
            return 0
        else
            error_msg "Dev mode active but debug worker binary not found."
            error_msg "Build with: cargo build -p adapteros-lora-worker"
            return 1
        fi
    else
        if [ -f "$release_bin" ]; then
            echo "$release_bin"
            return 0
        elif [ -f "$debug_bin" ]; then
            # Log to stderr so stdout only contains the path
            warning_msg "No release worker binary, using debug (slower)" >&2
            echo "$debug_bin"
            return 0
        else
            error_msg "Worker binary not found. Build with:"
            error_msg "  cargo build -p adapteros-lora-worker"
            return 1
        fi
    fi
}

# =============================================================================
# Service: Backend
# =============================================================================

start_backend() {
    ensure_dirs
    cleanup_stale_artifacts
    status_msg "Starting Backend Server (port $BACKEND_PORT)..."

    # Run preflight checks unless bypassed
    if [ -z "${AOS_SKIP_PREFLIGHT:-}" ]; then
        if ! run_preflight_checks; then
            error_msg "Preflight checks failed. Set AOS_SKIP_PREFLIGHT=1 to bypass."
            return 1
        fi
    else
        warning_msg "Preflight checks bypassed (AOS_SKIP_PREFLIGHT is set)"
    fi

    if is_running "$BACKEND_PID_FILE" "aos-server"; then
        local pid=$(get_pid "$BACKEND_PID_FILE")
        warning_msg "Backend is already running (PID: $pid)"
        return 0
    fi

    if ! ensure_port_free "$BACKEND_PORT" "Backend API"; then
        error_msg "Backend port $BACKEND_PORT is busy; unable to start."
        return 1
    fi

    # Select binary based on dev mode (dev flags → debug binary, prod → release)
    local server_bin=""
    # Capture stderr from select_server_binary without writing to /tmp.
    local select_binary_stderr="$PROJECT_ROOT/var/tmp/select_server_binary.$$.$RANDOM.stderr"
    : >"$select_binary_stderr"
    if ! server_bin=$(select_server_binary 2>"$select_binary_stderr"); then
        if [ -s "$select_binary_stderr" ]; then
            cat "$select_binary_stderr" >&2
        fi
        rm -f "$select_binary_stderr"
        return 1
    fi
    rm -f "$select_binary_stderr"

    # Set up environment
    export DATABASE_URL="${DATABASE_URL:-sqlite://$PROJECT_ROOT/var/aos-cp.sqlite3}"
    export RUST_LOG="${RUST_LOG:-info}"

    # Provide a default manifest path in dev so resolve_manifest_path succeeds
    if [ -z "${AOS_MANIFEST_PATH:-}" ]; then
        local dev_manifest_json="$DEFAULT_MODEL_DIR/config.json"
        local dev_manifest_yaml="$DEFAULT_MANIFEST_PATH"
        local fixture_manifest="$PROJECT_ROOT/crates/adapteros-server-api/tests/fixtures/mlx/Mistral-7B-Instruct-4bit/config.json"

        if [ -f "$dev_manifest_yaml" ]; then
            export AOS_MANIFEST_PATH="$dev_manifest_yaml"
        elif [ -f "$dev_manifest_json" ]; then
            export AOS_MANIFEST_PATH="$dev_manifest_json"
        elif [ -f "$fixture_manifest" ]; then
            export AOS_MANIFEST_PATH="$fixture_manifest"
        fi
    fi

    # Fail fast with a clear message when no manifest is available
    local manifest_path="${AOS_MANIFEST_PATH:-}"
    if [ -z "$manifest_path" ]; then
        error_msg "Manifest path required. Set AOS_MANIFEST_PATH or provide --manifest-path; expected dev manifest at $DEFAULT_MANIFEST_PATH."
        return 1
    fi
    if [ ! -f "$manifest_path" ]; then
        error_msg "Manifest path does not exist: $manifest_path. Provide a valid manifest via AOS_MANIFEST_PATH or CLI."
        return 1
    fi

    # Load model path if available
    if [ -d "$PROJECT_ROOT/models" ]; then
        local model_path=$(find "$PROJECT_ROOT/models" -maxdepth 1 -type d ! -name "models" | head -1)
        if [ -n "$model_path" ]; then
            export AOS_MLX_FFI_MODEL="$model_path"
        fi
    fi

    # Ensure database directory exists
    mkdir -p "$PROJECT_ROOT/var"

    # Start backend server
    # Drift checks remain enforced unless explicitly bypassed for development
    local drift_flag=""
    if [ "${AOS_DEV_SKIP_DRIFT_CHECK:-0}" = "1" ]; then
        drift_flag="--skip-drift-check"
    fi

    # Redirect stdin to avoid any accidental TTY coupling.
    # Note: We intentionally avoid `setsid` here because it is not available on
    # some macOS environments by default.
    rotate_log "$BACKEND_LOG"

    nohup "$server_bin" --config "${AOS_CONFIG_PATH:-$PROJECT_ROOT/configs/cp.toml}" \
        ${drift_flag:+$drift_flag} \
        > "$BACKEND_LOG" 2>&1 < /dev/null &
    local pid=$!
    disown "$pid" 2>/dev/null || true

    echo "$pid" > "$BACKEND_PID_FILE"

    # Give it a moment to start
    sleep 2

    # First check: verify process survived initial startup
    if ! kill -0 "$pid" 2>/dev/null; then
        error_msg "Backend failed to start. Check logs: $BACKEND_LOG"
        [ "${AOS_BOOT_VERBOSE:-0}" = "1" ] && tail -20 "$BACKEND_LOG"
        rm -f "$BACKEND_PID_FILE"
        return 1
    fi

    # Brief additional wait to catch fast-fail scenarios (migrations, config errors)
    sleep 1
    if ! kill -0 "$pid" 2>/dev/null; then
        error_msg "Backend crashed after startup (likely migration failure). Check logs: $BACKEND_LOG"
        [ "${AOS_BOOT_VERBOSE:-0}" = "1" ] && tail -30 "$BACKEND_LOG"
        rm -f "$BACKEND_PID_FILE"
        return 1
    fi

    success_msg "Backend started (PID: $pid, Port: $BACKEND_PORT)"
    return 0
}

restart_backend() {
    local mode="${1:-graceful}"
    status_msg "Restarting Backend Server (mode: ${mode})..."
    stop_backend "$mode"
    start_backend
}

stop_backend() {
    local mode="${1:-graceful}"

    local pid=""
    if is_running "$BACKEND_PID_FILE" "aos-server"; then
        pid=$(get_pid "$BACKEND_PID_FILE")
    else
        # PID files can be missing/stale if the backend was started outside the
        # service manager. Fall back to the active listener on BACKEND_PORT,
        # but only stop it if it looks like adapterOS.
        local port_pid
        port_pid=$(lsof -nP -i :"$BACKEND_PORT" -sTCP:LISTEN -t 2>/dev/null | head -1)
        if [ -n "${port_pid:-}" ] && kill -0 "$port_pid" 2>/dev/null; then
            local actual
            actual=$(ps -p "$port_pid" -o comm= 2>/dev/null | xargs basename 2>/dev/null || echo "")
            if [ -n "$actual" ] && [[ "$actual" == *"aos-server"* ]]; then
                warning_msg "Backend PID file missing/stale; stopping listener on port $BACKEND_PORT (PID: $port_pid)"
                pid="$port_pid"
            else
                warning_msg "Backend port $BACKEND_PORT is in use by non-adapterOS process ($actual); not stopping"
                return 0
            fi
        else
            status_msg "Backend is not running"
            return 0
        fi
    fi

    stop_process "$pid" "Backend" "$GRACEFUL_TIMEOUT" "$mode"
    rm -f "$BACKEND_PID_FILE"
}

# =============================================================================
# Service: UI (Leptos WASM - served from static/)
# =============================================================================

start_ui() {
    ensure_dirs

    # The Leptos UI is built to static/ and served by the backend server.
    # This function is a no-op but kept for backwards compatibility.
    status_msg "UI is served by the backend from static/ (Leptos WASM build)"
    status_msg "Build UI with: cd crates/adapteros-ui && trunk build --release"
    return 0
}

restart_ui() {
    # No-op: Leptos UI is served from static/ by the backend
    status_msg "UI is served by the backend from static/ (Leptos WASM build)"
    return 0
}

stop_ui() {
    # No-op: Leptos UI is served from static/ by the backend
    status_msg "UI is served by the backend (no separate process)"
    return 0
}

# =============================================================================
# Database Helper Functions
# =============================================================================

# Check if database is accessible
check_database() {
    if [ ! -f "$DATABASE_PATH" ]; then
        return 1
    fi
    sqlite3 "$DATABASE_PATH" "SELECT 1;" >/dev/null 2>&1 || return 1
    return 0
}

# Generate UUID with fallback
generate_uuid() {
    if command -v uuidgen > /dev/null 2>&1; then
        uuidgen | tr '[:upper:]' '[:lower:]'
    else
        # Fallback: use date + random
        echo "$(date +%s)-$(od -An -N4 -tu4 /dev/urandom | tr -d ' ')"
    fi
}

# Escape single quotes for SQL (basic protection)
sql_escape() {
    echo "$1" | sed "s/'/''/g"
}

# Validate foreign key exists
validate_foreign_key() {
    local table="$1"
    local column="$2"
    local value="$3"
    
    if ! check_database; then
        return 1
    fi
    
    local escaped_value=$(sql_escape "$value")
    local count=$(sqlite3 "$DATABASE_PATH" "SELECT COUNT(*) FROM $table WHERE $column='$escaped_value';" 2>&1)
    
    if [ $? -eq 0 ] && [ "$count" -gt 0 ]; then
        return 0
    fi
    return 1
}

# Ensure default plan exists, return plan_id
ensure_default_plan() {
    if ! check_database; then
        warning_msg "Database not accessible, skipping plan check"
        return 1
    fi

    # Validate tenant exists
    if ! validate_foreign_key "tenants" "id" "default"; then
        warning_msg "Default tenant does not exist in database"
        return 1
    fi

    # Check if any plan exists for default tenant (with retry for race conditions)
    local existing_plan=""
    local retries=3
    while [ $retries -gt 0 ]; do
        existing_plan=$(sqlite3 "$DATABASE_PATH" "SELECT id FROM plans WHERE tenant_id='default' LIMIT 1;" 2>&1)
        if [ $? -eq 0 ] && [ -n "$existing_plan" ]; then
            echo "$existing_plan"
            return 0
        fi
        sleep 0.1
        ((retries--))
    done

    # Try to create a default plan if manifest exists
    local manifest_hash=$(sqlite3 "$DATABASE_PATH" "SELECT hash_b3 FROM manifests WHERE tenant_id='default' LIMIT 1;" 2>&1)
    if [ $? -ne 0 ] || [ -z "$manifest_hash" ]; then
        warning_msg "No manifests found in database, cannot create default plan"
        return 1
    fi

    # Validate manifest exists
    if ! validate_foreign_key "manifests" "hash_b3" "$manifest_hash"; then
        warning_msg "Manifest hash not found in manifests table"
        return 1
    fi

    # Generate plan_id_b3 (use a deterministic hash based on manifest + tenant)
    local plan_id_b3=$(echo -n "default-plan-$manifest_hash" | shasum -a 256 | cut -d' ' -f1)
    local plan_id="plan-default-$(echo "$plan_id_b3" | cut -c1-8)"
    
    # Create minimal plan (escape values for SQL)
    local escaped_plan_id=$(sql_escape "$plan_id")
    local escaped_plan_id_b3=$(sql_escape "$plan_id_b3")
    local escaped_manifest_hash=$(sql_escape "$manifest_hash")
    local kernel_hashes_json='{"router":"default","executor":"default"}'
    local escaped_kernel_hashes=$(sql_escape "$kernel_hashes_json")
    local layout_hash_b3=$(echo -n "default-layout" | shasum -a 256 | cut -d' ' -f1)
    local escaped_layout_hash=$(sql_escape "$layout_hash_b3")
    local metadata_json='{"display_name":"Default Local Development Plan"}'
    local escaped_metadata=$(sql_escape "$metadata_json")

    # Try to insert plan (handle race condition)
    local insert_result=$(sqlite3 "$DATABASE_PATH" <<EOF 2>&1
INSERT INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3, metadata_json, created_at)
VALUES ('$escaped_plan_id', 'default', '$escaped_plan_id_b3', '$escaped_manifest_hash', '$escaped_kernel_hashes', '$escaped_layout_hash', '$escaped_metadata', datetime('now'));
EOF
)
    local insert_status=$?

    if [ $insert_status -eq 0 ] && [ -z "$insert_result" ]; then
        success_msg "Created default plan: $plan_id"
        echo "$plan_id"
        return 0
    else
        # Check if plan was created by another process (race condition)
        existing_plan=$(sqlite3 "$DATABASE_PATH" "SELECT id FROM plans WHERE tenant_id='default' LIMIT 1;" 2>&1)
        if [ $? -eq 0 ] && [ -n "$existing_plan" ]; then
            success_msg "Plan already exists (created by another process): $existing_plan"
            echo "$existing_plan"
            return 0
        fi
        
        if [ -n "$insert_result" ]; then
            warning_msg "Failed to create default plan: $insert_result"
        else
            warning_msg "Failed to create default plan (unknown error)"
        fi
        return 1
    fi
}

# Register worker in database
register_worker_in_db() {
    local pid="$1"
    local uds_path="$2"

    if ! check_database; then
        warning_msg "Database not accessible, skipping worker registration"
        return 1
    fi

    # Get or create default plan
    local plan_id=$(ensure_default_plan)
    if [ -z "$plan_id" ]; then
        warning_msg "Cannot register worker: no plan available"
        return 1
    fi

    # Validate plan exists
    if ! validate_foreign_key "plans" "id" "$plan_id"; then
        warning_msg "Plan validation failed: $plan_id"
        return 1
    fi

    # Generate worker ID
    local worker_id=$(generate_uuid)
    
    # Get default tenant and node
    local tenant_id="default"
    
    # Validate tenant exists
    if ! validate_foreign_key "tenants" "id" "$tenant_id"; then
        warning_msg "Tenant validation failed: $tenant_id"
        return 1
    fi
    
    # Get or create node
    local node_id=$(sqlite3 "$DATABASE_PATH" "SELECT id FROM nodes LIMIT 1;" 2>&1)
    if [ $? -ne 0 ] || [ -z "$node_id" ]; then
        # Create a local node if none exists
        local hostname_val=$(hostname)
        node_id="node-local-$(echo "$hostname_val" | tr '[:upper:]' '[:lower:]' | tr -d ' ' | tr -d '.' | cut -c1-20)"
        
        local escaped_node_id=$(sql_escape "$node_id")
        local escaped_hostname=$(sql_escape "$hostname_val")
        
        local node_result=$(sqlite3 "$DATABASE_PATH" <<EOF 2>&1
INSERT OR IGNORE INTO nodes (id, hostname, agent_endpoint, status, created_at)
VALUES ('$escaped_node_id', '$escaped_hostname', 'http://127.0.0.1:8081', 'active', datetime('now'));
EOF
)
        local node_status=$?
        
        if [ $node_status -eq 0 ]; then
            # Verify node was created
            if validate_foreign_key "nodes" "id" "$node_id"; then
                success_msg "Created local node: $node_id"
            else
                # Try to get existing node (race condition)
                node_id=$(sqlite3 "$DATABASE_PATH" "SELECT id FROM nodes LIMIT 1;" 2>&1)
                if [ -z "$node_id" ]; then
                    warning_msg "Failed to create or find node: $node_result"
                    return 1
                fi
            fi
        else
            warning_msg "Failed to create node: $node_result"
            # Try to get existing node
            node_id=$(sqlite3 "$DATABASE_PATH" "SELECT id FROM nodes LIMIT 1;" 2>&1)
            if [ -z "$node_id" ]; then
                return 1
            fi
        fi
    fi

    # Validate node exists
    if ! validate_foreign_key "nodes" "id" "$node_id"; then
        warning_msg "Node validation failed: $node_id"
        return 1
    fi

    # Escape values for SQL
    local escaped_worker_id=$(sql_escape "$worker_id")
    local escaped_tenant_id=$(sql_escape "$tenant_id")
    local escaped_node_id=$(sql_escape "$node_id")
    local escaped_plan_id=$(sql_escape "$plan_id")
    local escaped_uds_path=$(sql_escape "$uds_path")

    # Register worker
    local insert_result=$(sqlite3 "$DATABASE_PATH" <<EOF 2>&1
INSERT INTO workers (id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at)
VALUES ('$escaped_worker_id', '$escaped_tenant_id', '$escaped_node_id', '$escaped_plan_id', '$escaped_uds_path', $pid, 'starting', datetime('now'));
EOF
)
    local insert_status=$?

    if [ $insert_status -eq 0 ] && [ -z "$insert_result" ]; then
        echo "$worker_id" > "$WORKER_ID_FILE"
        success_msg "Registered worker in database: $worker_id"
        return 0
    else
        if [ -n "$insert_result" ]; then
            warning_msg "Failed to register worker in database: $insert_result"
        else
            warning_msg "Failed to register worker in database (unknown error)"
        fi
        return 1
    fi
}

# Update worker status in database
update_worker_status() {
    local new_status="$1"

    # Validate status value
    case "$new_status" in
        starting|serving|draining|stopped|crashed)
            ;;
        *)
            warning_msg "Invalid worker status: $new_status"
            return 1
            ;;
    esac

    if ! check_database; then
        return 1
    fi

    if [ ! -f "$WORKER_ID_FILE" ]; then
        # No worker_id file, worker wasn't registered
        return 0
    fi

    local worker_id=$(cat "$WORKER_ID_FILE" 2>/dev/null)
    if [ -z "$worker_id" ]; then
        return 0
    fi

    # Escape values for SQL
    local escaped_status=$(sql_escape "$new_status")
    local escaped_worker_id=$(sql_escape "$worker_id")

    local update_result=$(sqlite3 "$DATABASE_PATH" <<EOF 2>&1
UPDATE workers SET status='$escaped_status', last_seen_at=datetime('now') WHERE id='$escaped_worker_id';
EOF
)
    local update_status=$?

    if [ $update_status -eq 0 ] && [ -z "$update_result" ]; then
        if [ "$new_status" = "stopped" ] || [ "$new_status" = "crashed" ]; then
            rm -f "$WORKER_ID_FILE"
        fi
        return 0
    else
        if [ -n "$update_result" ]; then
            warning_msg "Failed to update worker status to $new_status: $update_result"
        else
            warning_msg "Failed to update worker status to $new_status (unknown error)"
        fi
        return 1
    fi
}

# Clean up stale worker records
cleanup_stale_workers() {
    if ! check_database; then
        return 1
    fi

    # Find workers in non-terminal status whose PID no longer exists
    local stale_workers=$(sqlite3 "$DATABASE_PATH" "SELECT id, pid FROM workers WHERE status NOT IN ('stopped', 'error') AND pid IS NOT NULL;" 2>&1)
    
    if [ $? -ne 0 ]; then
        warning_msg "Failed to query stale workers: $stale_workers"
        return 1
    fi
    
    if [ -z "$stale_workers" ]; then
        return 0
    fi

    # Process each stale worker
    echo "$stale_workers" | while IFS='|' read -r worker_id pid; do
        if [ -n "$worker_id" ] && [ -n "$pid" ]; then
            # Validate PID is numeric
            if ! [[ "$pid" =~ ^[0-9]+$ ]]; then
                continue
            fi
            
            # Check if process still exists
            if ! kill -0 "$pid" 2>/dev/null; then
                # Process is dead, mark worker as crashed
                local escaped_worker_id=$(sql_escape "$worker_id")
                local update_result=$(sqlite3 "$DATABASE_PATH" "UPDATE workers SET status='crashed', last_seen_at=datetime('now') WHERE id='$escaped_worker_id';" 2>&1)
                
                if [ $? -ne 0 ] && [ -n "$update_result" ]; then
                    warning_msg "Failed to mark worker $worker_id as crashed: $update_result"
                fi
            fi
        fi
    done

    return 0
}

# =============================================================================
# Service: Worker
# =============================================================================

start_worker() {
    ensure_dirs

    # Check if worker socket already exists and is in use
    local worker_sock="$PROJECT_ROOT/var/run/worker.sock"
    if [ -S "$worker_sock" ]; then
        local existing_pid=$(lsof -t "$worker_sock" 2>/dev/null | head -1)
        if [ -n "$existing_pid" ] && kill -0 "$existing_pid" 2>/dev/null; then
            warning_msg "Worker socket already in use (PID: $existing_pid)"
            echo "$existing_pid" > "$WORKER_PID_FILE"
            return 0
        else
            # Stale socket, remove it
            status_msg "Removing stale worker socket..."
            rm -f "$worker_sock"
        fi
    fi

    if is_running "$WORKER_PID_FILE" "aos-worker"; then
        local pid=$(get_pid "$WORKER_PID_FILE")
        warning_msg "Worker is already running (PID: $pid)"
        return 0
    fi

    status_msg "Starting Inference Worker..."

    # Determinism/stability: MLX worker support is gated by compiled features.
    # In dev mode, proactively build the worker with the MLX feature when MLX is requested.
    # This avoids the common failure mode where `cargo build -p adapteros-lora-worker` produced
    # a binary that can parse `--backend mlx` but cannot initialize MLX at runtime.
    local backend="${AOS_MODEL_BACKEND:-mlx}"
    if is_dev_mode && [ "$backend" = "mlx" ] && [ "${AOS_WORKER_AUTO_BUILD_MLX:-1}" != "0" ]; then
        if command -v cargo >/dev/null 2>&1; then
            status_msg "Ensuring worker built with MLX support (cargo build -p adapteros-lora-worker --features mlx)..."
            cargo build -p adapteros-lora-worker --features mlx >/dev/null 2>&1 || {
                error_msg "Failed to build worker with MLX support."
                error_msg "Try: cargo build -p adapteros-lora-worker --features mlx"
                return 1
            }
        else
            warning_msg "cargo not found; cannot auto-build MLX worker. Install Rust toolchain or build manually."
        fi
    fi

    # CoreML backend is also feature-gated (coreml-backend) and is not part of
    # the default worker feature set. In dev mode, proactively build it when
    # requested so demo boots don't silently fall back to Metal/CPU.
    if is_dev_mode && [ "$backend" = "coreml" ] && [ "${AOS_WORKER_AUTO_BUILD_COREML:-1}" != "0" ]; then
        if command -v cargo >/dev/null 2>&1; then
            status_msg "Ensuring worker built with CoreML+MLX support (cargo build -p adapteros-lora-worker --features coreml-backend,mlx)..."
            cargo build -p adapteros-lora-worker --features coreml-backend,mlx >/dev/null 2>&1 || {
                error_msg "Failed to build worker with CoreML+MLX support."
                error_msg "Try: cargo build -p adapteros-lora-worker --features coreml-backend,mlx"
                return 1
            }
        else
            warning_msg "cargo not found; cannot auto-build CoreML worker. Install Rust toolchain or build manually."
        fi
    fi

    # Select binary based on dev mode (dev flags → debug binary, prod → release)
    local worker_bin=""
    if ! worker_bin=$(select_worker_binary); then
        return 1
    fi

    # Determine manifest and model paths (default to 32B model)
    local manifest_path="${AOS_WORKER_MANIFEST:-$DEFAULT_MANIFEST_PATH}"
    local manifest_hash="${AOS_MANIFEST_HASH:-$DEFAULT_MANIFEST_HASH}"
    local model_path="${AOS_MODEL_PATH:-$DEFAULT_MODEL_DIR}"
    local uds_path="${AOS_WORKER_SOCKET:-$PROJECT_ROOT/var/run/worker.sock}"
    # Default to MLX; requires worker to be built with multi-backend/MLX features.
    # Override via AOS_MODEL_BACKEND=metal|coreml if MLX is unavailable.
    backend="${AOS_MODEL_BACKEND:-mlx}"

    if [ "$backend" = "mock" ]; then
        error_msg "Mock backend is sunset (no stubs). Set AOS_MODEL_BACKEND=mlx|coreml|metal."
        return 1
    fi
    
    # Auto-detect tokenizer path if not set
    local tokenizer_path="${AOS_TOKENIZER_PATH:-}"
    if [ -z "$tokenizer_path" ] && [ -d "$model_path" ]; then
        tokenizer_path="$model_path/tokenizer.json"
    fi

    # Validate paths - fail explicitly on missing files (not silent skip)
    if [ ! -f "$manifest_path" ]; then
        error_msg "Manifest not found: $manifest_path"
        error_msg "Set AOS_WORKER_MANIFEST or provide valid --manifest-path"
        return 1
    fi

    if [ ! -d "$model_path" ]; then
        error_msg "Model directory not found: $model_path"
        error_msg "Run ./scripts/download-model.sh or set AOS_MODEL_PATH"
        return 1
    fi

    # Guard common MLX failure mode: feature not built
    if [ "$backend" = "mlx" ]; then
        if ! "$worker_bin" --help 2>&1 | grep -qi "mlx"; then
            error_msg "Backend 'mlx' requested but worker binary likely built without MLX features. Rebuild with: cargo build -p adapteros-lora-worker --features mlx"
            return 1
        fi
    fi

    # Ensure socket directory exists
    mkdir -p "$(dirname "$uds_path")"

    # Set up environment (do not auto-enable dev-only metallib skip)
    export AOS_DEV_SKIP_METALLIB_CHECK="${AOS_DEV_SKIP_METALLIB_CHECK:-0}"
    export RUST_LOG="${RUST_LOG:-info,adapteros_lora_worker=info}"

    # Build worker command.
    # Only pass --manifest-hash when we actually have one; clap treats an empty
    # string as "Some(...)" and the worker will try to parse it as hex and fail.
    local -a worker_args=(
        "$worker_bin"
        --manifest "$manifest_path"
        --model-path "$model_path"
        --uds-path "$uds_path"
        --backend "$backend"
    )
    if [ -n "${manifest_hash:-}" ]; then
        export AOS_MANIFEST_HASH="$manifest_hash"
        worker_args+=(--manifest-hash "$manifest_hash")
    fi

    # Add tokenizer path if found
    if [ -n "$tokenizer_path" ] && [ -f "$tokenizer_path" ]; then
        worker_args+=(--tokenizer "$tokenizer_path")
    fi

    # Start worker (avoid eval, detach).
    rotate_log "$WORKER_LOG"
    nohup "${worker_args[@]}" > "$WORKER_LOG" 2>&1 < /dev/null &
    local pid=$!
    disown "$pid" 2>/dev/null || true

    echo "$pid" > "$WORKER_PID_FILE"

    # Wait for socket to be created (configurable timeout, default 150 seconds).
    # Must exceed the strict-mode verifying key load deadline (120s) since the
    # UDS socket is bound after key loading completes.
    local waited=0
    local timeout="${AOS_WORKER_TIMEOUT:-150}"
    local log_interval=5
    while [ $waited -lt "$timeout" ]; do
        # Early exit: check if process died during startup
        if ! kill -0 "$pid" 2>/dev/null; then
            error_msg "Worker process died during startup. Check logs: $WORKER_LOG"
            [ "${AOS_BOOT_VERBOSE:-0}" = "1" ] && tail -25 "$WORKER_LOG"
            rm -f "$WORKER_PID_FILE"
            update_worker_status "crashed" || true
            return 1
        fi

        if [ -S "$uds_path" ]; then
            success_msg "Worker started (PID: $pid, Socket: $uds_path)"

            # Worker self-registers with the control plane via HTTP during its
            # own startup sequence (registration.rs). Shell-level DB registration
            # was removed to avoid racing with the Rust state machine, which
            # enforces validated transitions and audit logging.

            return 0
        fi
        if (( waited % log_interval == 0 )); then
            status_msg "Waiting for worker socket... elapsed=${waited}s target=${uds_path} pid=${pid}"
        fi
        sleep 1
        ((waited++))
    done

    # Check if process is still running
    if kill -0 "$pid" 2>/dev/null; then
        # Process is running but socket never created - this is a failure
        error_msg "Worker process running but socket never created after ${timeout}s (PID: $pid, Socket: $uds_path)"
        error_msg "This usually indicates a startup error. Check logs: $WORKER_LOG"
        # Don't register worker in DB since it's not functional
        update_worker_status "failed" || true
        return 1
    else
        error_msg "Worker failed to start. Check logs: $WORKER_LOG"
        rm -f "$WORKER_PID_FILE"
        update_worker_status "crashed" || true
        return 1
    fi
}

restart_worker() {
    local mode="${1:-graceful}"
    status_msg "Restarting Worker (mode: ${mode})..."
    stop_worker "$mode"
    start_worker
}

stop_worker() {
    local mode="${1:-graceful}"

    # Also check for processes using the socket
    local worker_sock="$PROJECT_ROOT/var/run/worker.sock"
    local socket_pid=""
    if [ -S "$worker_sock" ]; then
        socket_pid=$(lsof -t "$worker_sock" 2>/dev/null | head -1)
    fi

    local pid=""
    if is_running "$WORKER_PID_FILE" "aos-worker"; then
        pid=$(get_pid "$WORKER_PID_FILE")
    elif [ -n "$socket_pid" ]; then
        pid="$socket_pid"
        warning_msg "Found worker via socket (PID: $pid)"
    else
        status_msg "Worker is not running"
        rm -f "$worker_sock"
        rm -f "$WORKER_PID_FILE"
        # Clean up any stale database records
        update_worker_status "stopped" || true
        return 0
    fi

    # Update status to draining before stopping
    update_worker_status "draining" || true

    stop_process "$pid" "Worker" "$WORKER_TIMEOUT" "$mode"
    
    # Update status to stopped after process terminates
    update_worker_status "stopped" || true
    
    # Clean up socket and PID file
    rm -f "$worker_sock"
    rm -f "$WORKER_PID_FILE"
}

# =============================================================================
# Worker Auto-Restart with Exponential Backoff
# =============================================================================

start_worker_with_restart() {
    local max_restarts=3
    local restart_count=0
    local backoff_base=5  # seconds

    while true; do
        # Start worker
        if start_worker; then
            # Worker started successfully, monitor it
            local pid=$(get_pid "$WORKER_PID_FILE")

            # Poll for worker exit (can't use wait on disowned process)
            while kill -0 "$pid" 2>/dev/null; do
                sleep 1
            done

            # Read exit code from worker's exit file (written on shutdown)
            local exit_code=2  # Default to transient/restart if file missing (e.g. SIGKILL)
            local exit_file="$AOS_VAR_DIR/worker.exit"
            if [ -f "$exit_file" ]; then
                exit_code=$(cat "$exit_file" 2>/dev/null || echo "2")
                rm -f "$exit_file"
            fi

            # Exit code meanings (from aos_worker.rs):
            # 0: Graceful shutdown (don't restart)
            # 1: Config/validation error (don't restart)
            # 2: Transient error (restart with backoff)
            # 3: Fatal error (don't restart)

            if [ "$exit_code" -eq 0 ]; then
                info "Worker exited gracefully (exit code 0), not restarting"
                break
            elif [ "$exit_code" -eq 1 ]; then
                error_msg "Worker exited with config error (exit code 1), not restarting. Fix configuration and restart manually."
                break
            elif [ "$exit_code" -eq 3 ]; then
                error_msg "Worker exited with fatal error (exit code 3), not restarting. Investigation required."
                break
            elif [ "$exit_code" -eq 2 ]; then
                # Transient error - restart with backoff
                restart_count=$((restart_count + 1))

                if [ "$restart_count" -ge "$max_restarts" ]; then
                    error_msg "Worker restart limit reached ($max_restarts attempts), giving up"
                    break
                fi

                # Exponential backoff: backoff_base * 2^(restart_count-1)
                local backoff=$((backoff_base * (1 << (restart_count - 1))))
                warning_msg "Worker crashed with transient error (exit code 2), restarting in ${backoff}s (attempt $restart_count/$max_restarts)"
                sleep "$backoff"
            else
                # Unknown exit code - treat as transient
                restart_count=$((restart_count + 1))

                if [ "$restart_count" -ge "$max_restarts" ]; then
                    error_msg "Worker restart limit reached ($max_restarts attempts), giving up"
                    break
                fi

                local backoff=$((backoff_base * (1 << (restart_count - 1))))
                warning_msg "Worker exited with unknown code $exit_code, restarting in ${backoff}s (attempt $restart_count/$max_restarts)"
                sleep "$backoff"
            fi
        else
            # Worker failed to start
            restart_count=$((restart_count + 1))

            if [ "$restart_count" -ge "$max_restarts" ]; then
                error_msg "Worker failed to start after $max_restarts attempts, giving up"
                return 1
            fi

            local backoff=$((backoff_base * (1 << (restart_count - 1))))
            warning_msg "Worker failed to start, retrying in ${backoff}s (attempt $restart_count/$max_restarts)"
            sleep "$backoff"
        fi
    done
}

# =============================================================================
# Service: SecD (Secure Enclave Daemon)
# =============================================================================

select_secd_binary() {
    local debug_bin="$PROJECT_ROOT/target/debug/aos-secd"
    local release_bin="$PROJECT_ROOT/target/release/aos-secd"

    if is_dev_mode; then
        if [ -f "$debug_bin" ]; then
            echo "$debug_bin"
            return 0
        else
            error_msg "Dev mode active but debug secd binary not found."
            error_msg "Build with: cargo build -p adapteros-secd"
            return 1
        fi
    else
        if [ -f "$release_bin" ]; then
            echo "$release_bin"
            return 0
        elif [ -f "$debug_bin" ]; then
            warning_msg "No release secd binary, using debug (slower)" >&2
            echo "$debug_bin"
            return 0
        else
            error_msg "SecD binary not found. Build with:"
            error_msg "  cargo build -p adapteros-secd"
            return 1
        fi
    fi
}

start_secd() {
    ensure_dirs

    # Check if secd socket already exists and is in use
    if [ -S "$SECD_SOCKET" ]; then
        local existing_pid=$(lsof -t "$SECD_SOCKET" 2>/dev/null | head -1)
        if [ -n "$existing_pid" ] && kill -0 "$existing_pid" 2>/dev/null; then
            warning_msg "SecD socket already in use (PID: $existing_pid)"
            echo "$existing_pid" > "$SECD_PID_FILE"
            return 0
        else
            # Stale socket, remove it
            status_msg "Removing stale secd socket..."
            rm -f "$SECD_SOCKET"
        fi
    fi

    if is_running "$SECD_PID_FILE" "aos-secd"; then
        local pid=$(get_pid "$SECD_PID_FILE")
        warning_msg "SecD is already running (PID: $pid)"
        return 0
    fi

    status_msg "Starting Secure Enclave Daemon..."

    # Select binary based on dev mode
    local secd_bin=""
    if ! secd_bin=$(select_secd_binary); then
        return 1
    fi

    # Ensure socket directory exists
    mkdir -p "$(dirname "$SECD_SOCKET")"

    # Set up environment
    export RUST_LOG="${RUST_LOG:-info,adapteros_secd=info}"

    # Start secd
    rotate_log "$SECD_LOG"
    nohup "$secd_bin" \
        --socket "$SECD_SOCKET" \
        --pid-file "$SECD_PID_FILE" \
        --heartbeat-file "$PROJECT_ROOT/var/run/aos-secd.heartbeat" \
        --database "$DATABASE_PATH" \
        > "$SECD_LOG" 2>&1 &
    local pid=$!

    # Wait for socket to be created
    local waited=0
    while [ $waited -lt "$SECD_TIMEOUT" ]; do
        # Early exit: check if process died during startup
        if ! kill -0 "$pid" 2>/dev/null; then
            error_msg "SecD process died during startup. Check logs: $SECD_LOG"
            [ "${AOS_BOOT_VERBOSE:-0}" = "1" ] && tail -25 "$SECD_LOG"
            rm -f "$SECD_PID_FILE"
            return 1
        fi

        if [ -S "$SECD_SOCKET" ]; then
            success_msg "SecD started (PID: $pid, Socket: $SECD_SOCKET)"
            return 0
        fi
        sleep 1
        ((waited++))
    done

    # Timeout
    if kill -0 "$pid" 2>/dev/null; then
        error_msg "SecD process running but socket never created after ${SECD_TIMEOUT}s"
        [ "${AOS_BOOT_VERBOSE:-0}" = "1" ] && tail -25 "$SECD_LOG"
    else
        error_msg "SecD failed to start. Check logs: $SECD_LOG"
    fi
    return 1
}

stop_secd() {
    local mode="${1:-graceful}"

    # Check for processes using the socket
    local socket_pid=""
    if [ -S "$SECD_SOCKET" ]; then
        socket_pid=$(lsof -t "$SECD_SOCKET" 2>/dev/null | head -1)
    fi

    local pid=""
    if is_running "$SECD_PID_FILE" "aos-secd"; then
        pid=$(get_pid "$SECD_PID_FILE")
    elif [ -n "$socket_pid" ]; then
        pid="$socket_pid"
        warning_msg "Found secd via socket (PID: $pid)"
    else
        status_msg "SecD is not running"
        rm -f "$SECD_SOCKET"
        rm -f "$SECD_PID_FILE"
        return 0
    fi

    stop_process "$pid" "SecD" "$SECD_TIMEOUT" "$mode"
    
    # Clean up socket and PID file
    rm -f "$SECD_SOCKET"
    rm -f "$SECD_PID_FILE"
    rm -f "$PROJECT_ROOT/var/run/aos-secd.heartbeat"
}

restart_secd() {
    local mode="${1:-graceful}"
    status_msg "Restarting SecD (mode: ${mode})..."
    stop_secd "$mode"
    start_secd
}

# =============================================================================
# Service: Node Agent
# =============================================================================

select_node_binary() {
    local debug_bin="$PROJECT_ROOT/target/debug/aos-node"
    local release_bin="$PROJECT_ROOT/target/release/aos-node"

    if is_dev_mode; then
        if [ -f "$debug_bin" ]; then
            echo "$debug_bin"
            return 0
        else
            error_msg "Dev mode active but debug node binary not found."
            error_msg "Build with: cargo build -p adapteros-node"
            return 1
        fi
    else
        if [ -f "$release_bin" ]; then
            echo "$release_bin"
            return 0
        elif [ -f "$debug_bin" ]; then
            warning_msg "No release node binary, using debug (slower)" >&2
            echo "$debug_bin"
            return 0
        else
            error_msg "Node binary not found. Build with:"
            error_msg "  cargo build -p adapteros-node"
            return 1
        fi
    fi
}

start_node() {
    ensure_dirs

    if is_running "$NODE_PID_FILE" "aos-node"; then
        local pid=$(get_pid "$NODE_PID_FILE")
        warning_msg "Node agent is already running (PID: $pid)"
        return 0
    fi

    # Check if port is in use
    if ! ensure_port_free "$NODE_PORT" "Node Agent"; then
        error_msg "Node port $NODE_PORT is busy; unable to start."
        return 1
    fi

    status_msg "Starting Node Agent (port $NODE_PORT)..."

    # Select binary based on dev mode
    local node_bin=""
    if ! node_bin=$(select_node_binary); then
        return 1
    fi

    # Set up environment
    export RUST_LOG="${RUST_LOG:-info,aos_node=info}"

    # Start node agent (development mode with TCP binding)
    rotate_log "$NODE_LOG"
    nohup "$node_bin" \
        --port "$NODE_PORT" \
        --cas-path "$PROJECT_ROOT/var/cas" \
        --kernel-path "$PROJECT_ROOT/var/kernels" \
        --plan-path "$PROJECT_ROOT/var/plans" \
        > "$NODE_LOG" 2>&1 &
    local pid=$!

    echo "$pid" > "$NODE_PID_FILE"

    # Give it a moment to start
    sleep 2

    # Check if process survived startup
    if ! kill -0 "$pid" 2>/dev/null; then
        error_msg "Node agent failed to start. Check logs: $NODE_LOG"
        [ "${AOS_BOOT_VERBOSE:-0}" = "1" ] && tail -20 "$NODE_LOG"
        rm -f "$NODE_PID_FILE"
        return 1
    fi

    # Wait for health endpoint
    local waited=0
    while [ $waited -lt "$NODE_TIMEOUT" ]; do
        if curl -sf --max-time 2 "http://localhost:$NODE_PORT/health" >/dev/null 2>&1; then
            success_msg "Node agent started (PID: $pid, Port: $NODE_PORT)"
            return 0
        fi
        sleep 1
        ((waited++))
    done

    # Health check timeout
    if kill -0 "$pid" 2>/dev/null; then
        warning_msg "Node agent running but health endpoint not responding after ${NODE_TIMEOUT}s"
        # Still consider it started since process is alive
        success_msg "Node agent started (PID: $pid, Port: $NODE_PORT) - health pending"
        return 0
    else
        error_msg "Node agent crashed during startup. Check logs: $NODE_LOG"
        rm -f "$NODE_PID_FILE"
        return 1
    fi
}

stop_node() {
    local mode="${1:-graceful}"

    if ! is_running "$NODE_PID_FILE" "aos-node"; then
        # Try to find by port
        local port_pid=$(lsof -nP -i :"$NODE_PORT" -sTCP:LISTEN -t 2>/dev/null | head -1)
        if [ -n "$port_pid" ]; then
            warning_msg "Found node via port (PID: $port_pid)"
            stop_process "$port_pid" "Node" "$NODE_TIMEOUT" "$mode"
        else
            status_msg "Node agent is not running"
        fi
        rm -f "$NODE_PID_FILE"
        return 0
    fi

    local pid=$(get_pid "$NODE_PID_FILE")
    stop_process "$pid" "Node" "$NODE_TIMEOUT" "$mode"
    rm -f "$NODE_PID_FILE"
}

restart_node() {
    local mode="${1:-graceful}"
    status_msg "Restarting Node agent (mode: ${mode})..."
    stop_node "$mode"
    start_node
}

# =============================================================================
# Status Command
# =============================================================================


show_status() {
    echo -e "${CYAN}
================================
   adapterOS Service Status
================================${NC}"
    echo ""

    # Backend status
    local backend_running=0
    if is_running "$BACKEND_PID_FILE" "aos-server"; then
        local pid=$(get_pid "$BACKEND_PID_FILE")
        echo -e "${GREEN}[RUNNING]${NC} Backend Server (PID: $pid, Port: $BACKEND_PORT)"
        backend_running=1
    else
        # Fallback for when PID files are missing/stale.
        local port_pid
        port_pid=$(lsof -nP -i :"$BACKEND_PORT" -sTCP:LISTEN -t 2>/dev/null | head -1)
        if [ -n "${port_pid:-}" ] && kill -0 "$port_pid" 2>/dev/null; then
            local actual
            actual=$(ps -p "$port_pid" -o comm= 2>/dev/null | xargs basename 2>/dev/null || echo "")
            if [ -n "$actual" ] && [[ "$actual" == *"aos-server"* ]]; then
                echo -e "${GREEN}[RUNNING]${NC} Backend Server (PID: $port_pid, Port: $BACKEND_PORT)"
                backend_running=1
            else
                echo -e "${RED}[STOPPED]${NC} Backend Server"
            fi
        else
            echo -e "${RED}[STOPPED]${NC} Backend Server"
        fi
    fi

    # Check if HTTP endpoint responds (best-effort)
    if [ "$backend_running" -eq 1 ]; then
        if curl -s "http://localhost:$BACKEND_PORT/healthz" > /dev/null 2>&1; then
            echo -e "          ${GREEN}Health endpoint responding${NC}"
        else
            echo -e "          ${YELLOW}Health endpoint not responding${NC}"
        fi
    fi

    # UI status
    if is_running "$UI_PID_FILE"; then
        local pid=$(get_pid "$UI_PID_FILE")
        echo -e "${GREEN}[RUNNING]${NC} Web UI (PID: $pid, Port: $UI_PORT)"

        # Check if UI responds
        if curl -s "http://localhost:$UI_PORT" > /dev/null 2>&1; then
            echo -e "          ${GREEN}Web UI responding${NC}"
        else
            echo -e "          ${YELLOW}Web UI initializing...${NC}"
        fi
    fi

    # Worker status
    local worker_sock="$PROJECT_ROOT/var/run/worker.sock"
    local worker_pid=""
    local db_status=""
    local worker_id=""
    
    # Clean up stale workers first
    cleanup_stale_workers || true
    
    # Get database status if available
    if check_database && [ -f "$WORKER_ID_FILE" ]; then
        worker_id=$(cat "$WORKER_ID_FILE" 2>/dev/null)
        if [ -n "$worker_id" ]; then
            local escaped_worker_id=$(sql_escape "$worker_id")
            db_status=$(sqlite3 "$DATABASE_PATH" "SELECT status FROM workers WHERE id='$escaped_worker_id';" 2>&1)
            if [ $? -ne 0 ]; then
                db_status=""
            fi
        fi
    fi
    
    if [ -S "$worker_sock" ]; then
        worker_pid=$(lsof -t "$worker_sock" 2>/dev/null | head -1)
        if [ -n "$worker_pid" ] && kill -0 "$worker_pid" 2>/dev/null; then
            local status_line="${GREEN}[RUNNING]${NC} Inference Worker (PID: $worker_pid, Socket: $worker_sock)"
            if [ -n "$db_status" ]; then
                status_line="$status_line ${CYAN}[DB: $db_status]${NC}"
            fi
            if [ -n "$worker_id" ]; then
                status_line="$status_line ${WHITE}(ID: ${worker_id:0:8}...)${NC}"
            fi
            echo -e "$status_line"
        else
            echo -e "${YELLOW}[STALE]${NC} Worker socket exists but process not found"
        fi
    elif is_running "$WORKER_PID_FILE" "aos-worker"; then
        local pid=$(get_pid "$WORKER_PID_FILE")
        local status_line="${YELLOW}[STARTING]${NC} Inference Worker (PID: $pid, socket not ready)"
        if [ -n "$db_status" ]; then
            status_line="$status_line ${CYAN}[DB: $db_status]${NC}"
        fi
        echo -e "$status_line"
    else
        local status_line="${WHITE}[STOPPED]${NC} Inference Worker (optional)"
        if [ -n "$db_status" ] && [ "$db_status" != "stopped" ]; then
            status_line="$status_line ${YELLOW}[DB: $db_status]${NC}"
        fi
        echo -e "$status_line"
    fi

    # SecD status
    if [ -S "$SECD_SOCKET" ]; then
        local secd_pid=$(lsof -t "$SECD_SOCKET" 2>/dev/null | head -1)
        if [ -n "$secd_pid" ] && kill -0 "$secd_pid" 2>/dev/null; then
            echo -e "${GREEN}[RUNNING]${NC} SecD (PID: $secd_pid, Socket: $SECD_SOCKET)"
        else
            echo -e "${YELLOW}[STALE]${NC} SecD socket exists but process not found"
        fi
    elif is_running "$SECD_PID_FILE" "aos-secd"; then
        local pid=$(get_pid "$SECD_PID_FILE")
        echo -e "${YELLOW}[STARTING]${NC} SecD (PID: $pid, socket not ready)"
    else
        echo -e "${WHITE}[STOPPED]${NC} SecD (optional)"
    fi

    # Node agent status
    if is_running "$NODE_PID_FILE" "aos-node"; then
        local pid=$(get_pid "$NODE_PID_FILE")
        if curl -sf --max-time 2 "http://localhost:$NODE_PORT/health" >/dev/null 2>&1; then
            echo -e "${GREEN}[RUNNING]${NC} Node Agent (PID: $pid, Port: $NODE_PORT)"
        else
            echo -e "${YELLOW}[STARTING]${NC} Node Agent (PID: $pid, health pending)"
        fi
    else
        # Check if something is listening on node port
        local port_pid=$(lsof -nP -i :"$NODE_PORT" -sTCP:LISTEN -t 2>/dev/null | head -1)
        if [ -n "$port_pid" ]; then
            echo -e "${GREEN}[RUNNING]${NC} Node Agent (PID: $port_pid, Port: $NODE_PORT)"
        else
            echo -e "${WHITE}[STOPPED]${NC} Node Agent (optional)"
        fi
    fi

    echo ""
    echo -e "${CYAN}================================${NC}"
}

# =============================================================================
# Stop All Services
# =============================================================================

stop_all() {
    local mode="${1:-graceful}"
    local mode_upper
    mode_upper=$(echo "$mode" | tr '[:lower:]' '[:upper:]')

    echo -e "${CYAN}
================================
   Stopping All Services
   Mode: ${mode_upper}
================================${NC}"
    echo ""

    # Stop in reverse order of startup

    # 1. Node agent (optional)
    stop_node "$mode" 2>/dev/null || true

    # 2. Worker
    stop_worker "$mode" 2>/dev/null || true

    # 3. SecD (optional)
    stop_secd "$mode" 2>/dev/null || true

    # 4. UI
    stop_ui "$mode"

    # 5. Backend (last, as others may depend on it)
    stop_backend "$mode"

    echo ""
    success_msg "All services stopped"
}

# =============================================================================
# Usage
# =============================================================================

usage() {
    echo "adapterOS Service Manager"
    echo ""
    echo "USAGE:"
    echo "  $0 start <service>        Start a service"
    echo "  $0 stop all [mode]        Stop all services"
    echo "  $0 stop <service> [mode]  Stop a specific service"
    echo "  $0 restart <service> [mode] Restart a service (stop then start)"
    echo "  $0 status                 Show status of all services"
    echo ""
    echo "SERVICES:"
    echo "  backend     Backend API server"
    echo "  ui          Static UI served by backend (no process); use trunk serve for dev"
    echo "  worker      Inference worker (ML model server)"
    echo "  secd        Secure Enclave Daemon"
    echo "  node        Node Agent (cluster management)"
    echo ""
    echo "STOP MODES (used by stop/restart commands):"
    echo "  graceful    Graceful shutdown with full cleanup (default)"
    echo "  fast        Fast shutdown, reduced cleanup"
    echo "  immediate   Immediate shutdown, minimal cleanup"
    echo ""
    echo "EXAMPLES:"
    echo "  $0 start backend          # Start backend server"
    echo "  $0 start ui               # No-op (UI is served by backend)"
    echo "  $0 start secd             # Start Secure Enclave Daemon"
    echo "  $0 start node             # Start Node Agent"
    echo "  $0 start-all              # Start backend + worker (UI served by backend)"
    echo "  $0 stop all               # Stop all services gracefully"
    echo "  $0 stop all fast          # Fast stop all services"
    echo "  $0 restart backend        # Restart backend gracefully"
    echo "  $0 restart ui fast        # Restart UI using fast stop"
    echo "  $0 status                 # Show service status"
}

# =============================================================================
# Main Entry Point
# =============================================================================

# Rotate service-manager.log at script start if it exceeds ~5MB
mkdir -p "$LOG_DIR"
rotate_log_if_large "$SCRIPT_LOG" 5242880

if [ $# -lt 1 ]; then
    usage
    exit 1
fi

COMMAND="$1"
SERVICE="${2:-}"
MODE="${3:-graceful}"

case "$COMMAND" in
    start)
        case "$SERVICE" in
            backend)
                start_backend
                ;;
            ui)
                start_ui
                ;;
            worker)
                start_worker
                ;;
            secd)
                start_secd
                ;;
            node)
                start_node
                ;;
            "")
                error_msg "Please specify a service to start"
                usage
                exit 1
                ;;
            *)
                error_msg "Unknown service: $SERVICE"
                usage
                exit 1
                ;;
        esac
        ;;
    stop)
        case "$SERVICE" in
            all)
                stop_all "$MODE"
                ;;
            backend)
                stop_backend "$MODE"
                ;;
            ui)
                stop_ui "$MODE"
                ;;
            worker)
                stop_worker "$MODE"
                ;;
            secd)
                stop_secd "$MODE"
                ;;
            node)
                stop_node "$MODE"
                ;;
            "")
                error_msg "Please specify a service to stop (or 'all')"
                usage
                exit 1
                ;;
            *)
                error_msg "Unknown service: $SERVICE"
                usage
                exit 1
                ;;
        esac
        ;;
    restart)
        case "$SERVICE" in
            backend)
                restart_backend "$MODE"
                ;;
            ui)
                restart_ui "$MODE"
                ;;
            worker)
                restart_worker "$MODE"
                ;;
            secd)
                restart_secd "$MODE"
                ;;
            node)
                restart_node "$MODE"
                ;;
            "")
                error_msg "Please specify a service to restart"
                usage
                exit 1
                ;;
            *)
                error_msg "Unknown service: $SERVICE"
                usage
                exit 1
                ;;
        esac
        ;;
    status)
        show_status
        ;;
    start-all)
        # Start all services (backend + UI + worker)
        echo -e "${CYAN}
================================
   Starting All Services
================================${NC}"
        echo ""
        start_backend && start_ui && start_worker
        echo ""
        show_status
        ;;
    stop-all)
        # Stop all services
        stop_all "${SERVICE:-graceful}"
        ;;
    start-backend)
        # Start backend only (shortcut)
        start_backend
        ;;
    help|-h|--help)
        usage
        ;;
    *)
        error_msg "Unknown command: $COMMAND"
        usage
        exit 1
        ;;
esac
