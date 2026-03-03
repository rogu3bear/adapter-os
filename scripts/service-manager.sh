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
source "$PROJECT_ROOT/scripts/lib/ports.sh"
source "$PROJECT_ROOT/scripts/lib/model-config.sh"

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

aos_apply_port_pane_defaults
aos_resolve_model_runtime_env "$PROJECT_ROOT"

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
SHARED_SERVICE_LOCK_DIR="$PROJECT_ROOT/var/run/service-control.lock"
TRAINING_WORKER_DEGRADED_FILE="$PROJECT_ROOT/var/run/training-worker.degraded"
BACKEND_SUPERVISION_STATE_FILE="$PROJECT_ROOT/var/run/backend-supervision.state"
BACKEND_LAUNCHD_LABEL="${AOS_BACKEND_LAUNCHD_LABEL:-com.adapteros.backend}"
BACKEND_LAUNCHD_DOMAIN="gui/$(id -u)"
BACKEND_LAUNCHD_TARGET="${BACKEND_LAUNCHD_DOMAIN}/${BACKEND_LAUNCHD_LABEL}"

# Log file locations
LOG_DIR="$PROJECT_ROOT/var/logs"
BACKEND_LOG="$LOG_DIR/backend.log"
UI_LOG="$LOG_DIR/ui.log"
WORKER_LOG="$LOG_DIR/worker.log"
WORKER_RESTART_LOG="$LOG_DIR/worker-restarts.log"
SECD_LOG="$LOG_DIR/secd.log"
NODE_LOG="$LOG_DIR/node.log"
SCRIPT_LOG="$LOG_DIR/service-manager.log"

# Socket paths
SECD_SOCKET="$PROJECT_ROOT/var/run/aos-secd.sock"

# Port configuration for node
NODE_PORT="${AOS_NODE_PORT:-18083}"

# Canonical model runtime defaults resolved from shared policy.
DEFAULT_MODEL_DIR="${AOS_MODEL_PATH:-$PROJECT_ROOT/var/models/Qwen3.5-27B}"
DEFAULT_MANIFEST_PATH="${AOS_WORKER_MANIFEST:-${AOS_MANIFEST_PATH:-$PROJECT_ROOT/manifests/qwen35-27b-mlx-base-only.yaml}}"
# Manifest hash is loaded from .env (DEFAULT_MANIFEST_HASH or AOS_MANIFEST_HASH)
# No hardcoded fallback - .env is the single source of truth for the hash value
DEFAULT_MANIFEST_HASH="${DEFAULT_MANIFEST_HASH:-${AOS_MANIFEST_HASH:-}}"

# Worker database tracking
WORKER_ID_FILE="$PID_DIR/worker.id"
WORKER_START_TS_FILE="$PID_DIR/worker.start_ts"
WORKER_RESTART_COUNT_FILE="$PID_DIR/worker.restart_count"
DATABASE_PATH="${AOS_DATABASE_URL:-sqlite://var/aos-cp.sqlite3}"
# Extract SQLite path from DATABASE_URL if it's a sqlite URL.
# Accept both sqlite://... and sqlite:... forms for compatibility.
if [[ "$DATABASE_PATH" == sqlite://* ]]; then
    DATABASE_PATH="${DATABASE_PATH#sqlite://}"
elif [[ "$DATABASE_PATH" == sqlite:* ]]; then
    DATABASE_PATH="${DATABASE_PATH#sqlite:}"
fi
DATABASE_PATH="${DATABASE_PATH%%\?*}"
DATABASE_PATH="${DATABASE_PATH%%#*}"
# If still relative, make it absolute relative to PROJECT_ROOT
if [[ "$DATABASE_PATH" != /* ]]; then
    DATABASE_PATH="$PROJECT_ROOT/$DATABASE_PATH"
fi

# Port configuration
BACKEND_PORT="${AOS_SERVER_PORT:-18080}"
UI_PORT="${AOS_UI_PORT:-18081}"

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

# Archive a log file with a UTC timestamp suffix and prune older archives.
# Usage: archive_log_file <file> [keep_count]
archive_log_file() {
    local file="$1"
    local keep_count="${2:-6}"
    [ -f "$file" ] || return 0

    local ts archive
    ts="$(date -u +"%Y%m%dT%H%M%SZ")"
    archive="${file}.${ts}"

    mv "$file" "$archive"
    prune_log_archives "$file" "$keep_count"
}

# Keep only the newest N timestamped archives for a base log file.
# Usage: prune_log_archives <base_file> <keep_count>
prune_log_archives() {
    local base_file="$1"
    local keep_count="${2:-6}"
    [ "$keep_count" -gt 0 ] 2>/dev/null || return 0

    local -a archives
    mapfile -t archives < <(ls -1t "${base_file}".20* 2>/dev/null || true)
    local count="${#archives[@]}"
    if [ "$count" -le "$keep_count" ]; then
        return 0
    fi

    local i
    for ((i = keep_count; i < count; i++)); do
        rm -f "${archives[$i]}"
    done
}

# Rotate a log file if it exceeds a size threshold (bytes).
# Usage: rotate_log_if_large <file> <max_bytes> [keep_count]
rotate_log_if_large() {
    local file="$1"
    local max_bytes="$2"
    local keep_count="${3:-6}"
    [ -f "$file" ] || return 0
    local size
    size=$(wc -c < "$file" 2>/dev/null | tr -d ' ')
    if [ -n "$size" ] && [ "$size" -gt "$max_bytes" ]; then
        archive_log_file "$file" "$keep_count"
    fi
}

# Rotate a log file if it is non-empty.
# Usage: rotate_log <file> [keep_count]
rotate_log() {
    local file="$1"
    local keep_count="${2:-6}"
    if [ -s "$file" ]; then
        archive_log_file "$file" "$keep_count"
    fi
    return 0
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
    ensure_backend_log_alias
}

is_backend_launchd_managed() {
    if [ "${AOS_FORCE_SERVICE_MANAGER_BACKEND:-0}" = "1" ]; then
        return 1
    fi
    command -v launchctl >/dev/null 2>&1 || return 1
    launchctl print "$BACKEND_LAUNCHD_TARGET" >/dev/null 2>&1
}

kickstart_backend_launchd() {
    command -v launchctl >/dev/null 2>&1 || return 1
    launchctl kickstart -k "$BACKEND_LAUNCHD_TARGET" >/dev/null 2>&1
}

wait_for_backend_health() {
    local health_wait_secs="${1:-20}"
    if ! [[ "$health_wait_secs" =~ ^[0-9]+$ ]]; then
        health_wait_secs=20
    fi

    local waited=0
    while [ "$waited" -lt "$health_wait_secs" ]; do
        if curl -s "http://127.0.0.1:$BACKEND_PORT/healthz" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
        waited=$((waited + 1))
    done
    return 1
}

acquire_service_control_lock() {
    if [ "${AOS_SERVICE_CONTROL_LOCK_HELD:-0}" = "1" ]; then
        return 0
    fi
    mkdir -p "$PROJECT_ROOT/var/run"
    if mkdir "$SHARED_SERVICE_LOCK_DIR" 2>/dev/null; then
        trap 'rmdir "$SHARED_SERVICE_LOCK_DIR" >/dev/null 2>&1 || true' EXIT
        return 0
    fi

    if [ -d "$SHARED_SERVICE_LOCK_DIR" ]; then
        local holders
        holders="$(pgrep -f 'service-manager\.sh|aos-launchd-ensure\.sh' || true)"
        holders="$(printf '%s\n' "$holders" | awk -v self="$$" 'NF && $1 != self')"
        if [ -z "$holders" ]; then
            warning_msg "Detected stale service control lock; reaping"
            rmdir "$SHARED_SERVICE_LOCK_DIR" >/dev/null 2>&1 || true
            if mkdir "$SHARED_SERVICE_LOCK_DIR" 2>/dev/null; then
                trap 'rmdir "$SHARED_SERVICE_LOCK_DIR" >/dev/null 2>&1 || true' EXIT
                return 0
            fi
        fi
    fi

    warning_msg "Service control lock is held; another service operation is in progress"
    return 1
}

ensure_backend_log_alias() {
    local alias="$LOG_DIR/server.log"

    if [ -L "$alias" ]; then
        ln -sfn "backend.log" "$alias" 2>/dev/null || true
        return
    fi

    if [ -e "$alias" ]; then
        if [ ! -e "$BACKEND_LOG" ]; then
            mv "$alias" "$BACKEND_LOG"
        else
            local ts
            ts=$(date +%s)
            mv "$alias" "${alias}.legacy.${ts}"
        fi
    fi

    if [ ! -e "$alias" ]; then
        ln -sfn "backend.log" "$alias" 2>/dev/null || true
    fi
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

# Resolve model path used for startup checks.
# Precedence:
#   1) AOS_MODEL_PATH
#   2) AOS_MODEL_CACHE_DIR + AOS_BASE_MODEL_ID
#   3) DEFAULT_MODEL_DIR
resolve_model_path_for_preflight() {
    local model_path="${AOS_MODEL_PATH:-}"

    if [ -z "$model_path" ] && [ -n "${AOS_MODEL_CACHE_DIR:-}" ] && [ -n "${AOS_BASE_MODEL_ID:-}" ]; then
        model_path="${AOS_MODEL_CACHE_DIR%/}/${AOS_BASE_MODEL_ID}"
    fi

    if [ -z "$model_path" ]; then
        model_path="$DEFAULT_MODEL_DIR"
    fi

    if [[ "$model_path" != /* ]]; then
        model_path="$PROJECT_ROOT/${model_path#./}"
    fi

    printf '%s\n' "$model_path"
}

resolve_manifest_path_for_preflight() {
    local manifest_path="${AOS_WORKER_MANIFEST:-${AOS_MANIFEST_PATH:-$DEFAULT_MANIFEST_PATH}}"

    if [[ "$manifest_path" != /* ]]; then
        manifest_path="$PROJECT_ROOT/${manifest_path#./}"
    fi

    printf '%s\n' "$manifest_path"
}

blake3_hash_file() {
    local file_path="$1"

    if ! command -v b3sum >/dev/null 2>&1; then
        error_msg "b3sum not found (required for manifest compatibility checks)"
        return 1
    fi

    b3sum "$file_path" | awk '{print $1}'
}

read_manifest_base_fields() {
    local manifest_path="$1"
    python3 - "$manifest_path" <<'PY'
import json
import sys

manifest_path = sys.argv[1]
raw = open(manifest_path, "r", encoding="utf-8").read()

try:
    payload = json.loads(raw)
except Exception:
    try:
        import yaml
        payload = yaml.safe_load(raw)
    except Exception as exc:
        print(f"__ERROR__=Failed to parse manifest: {exc}")
        raise SystemExit(0)

if not isinstance(payload, dict):
    print("__ERROR__=Manifest is not a mapping")
    raise SystemExit(0)

base = payload.get("base")
if not isinstance(base, dict):
    print("__ERROR__=Manifest missing base section")
    raise SystemExit(0)

for key in ("model_id", "model_hash", "config_hash", "tokenizer_hash", "tokenizer_cfg_hash"):
    value = base.get(key, "")
    if value is None:
        value = ""
    print(f"{key}={value}")
PY
}

compute_model_identity_hash() {
    local model_path="$1"
    local config_file="$model_path/config.json"
    local first_weight_file
    first_weight_file="$(find "$model_path" -type f \( -name "*.safetensors" -o -name "*.bin" \) | sort | head -n 1)"

    if [ ! -f "$config_file" ]; then
        return 1
    fi

    if [ -n "$first_weight_file" ]; then
        cat "$config_file" "$first_weight_file" | b3sum | awk '{print $1}'
    else
        cat "$config_file" | b3sum | awk '{print $1}'
    fi
}

check_model_path_readiness() {
    local model_path
    model_path="$(resolve_model_path_for_preflight)"

    if [ ! -d "$model_path" ]; then
        error_msg "Model directory not found: $model_path"
        error_msg "Set AOS_MODEL_PATH or configure AOS_MODEL_CACHE_DIR + AOS_BASE_MODEL_ID"
        error_msg "Run ./scripts/download-model.sh to provision the canonical model path"
        return 1
    fi

    local required_files=("config.json" "tokenizer.json" "tokenizer_config.json")
    local missing=()
    local file
    for file in "${required_files[@]}"; do
        if [ ! -f "$model_path/$file" ]; then
            missing+=("$file")
        fi
    done

    if [ "${#missing[@]}" -gt 0 ]; then
        error_msg "Model directory missing required file(s): ${missing[*]}"
        error_msg "Path: $model_path"
        return 1
    fi

    local first_weight_file
    first_weight_file="$(find "$model_path" -type f \( -name "*.safetensors" -o -name "*.bin" \) | head -n 1)"
    if [ -z "$first_weight_file" ]; then
        error_msg "Model directory has no weight files (*.safetensors or *.bin): $model_path"
        error_msg "Run ./scripts/download-model.sh to fetch full model weights"
        return 1
    fi

    local partial_count
    partial_count="$(find "$model_path" -type f -name "*.part" 2>/dev/null | wc -l | tr -d ' ')"
    if [ "${partial_count:-0}" -gt 0 ]; then
        error_msg "Model directory contains incomplete download files (*.part): $partial_count"
        error_msg "Path: $model_path"
        error_msg "Wait for download completion before starting runtime"
        return 1
    fi

    local shard_index="$model_path/model.safetensors.index.json"
    if [ -f "$shard_index" ]; then
        local missing_shards
        missing_shards="$(python3 - "$shard_index" "$model_path" <<'PY'
import json
import pathlib
import sys

index_path = pathlib.Path(sys.argv[1])
model_dir = pathlib.Path(sys.argv[2])

try:
    payload = json.loads(index_path.read_text(encoding="utf-8"))
except Exception as exc:
    print(f"failed to parse model index: {exc}")
    raise SystemExit(0)

weight_map = payload.get("weight_map")
if not isinstance(weight_map, dict):
    raise SystemExit(0)

shards = sorted({str(v) for v in weight_map.values() if isinstance(v, str) and v.strip()})
missing = [name for name in shards if not (model_dir / name).is_file()]
if missing:
    print(", ".join(missing))
PY
)"
        if [ -n "$missing_shards" ]; then
            error_msg "Model index references missing shard file(s): $missing_shards"
            error_msg "Path: $model_path"
            return 1
        fi
    fi

    status_msg "Model path check passed: $model_path"
    return 0
}

check_model_manifest_compatibility() {
    local model_path manifest_path
    model_path="$(resolve_model_path_for_preflight)"
    manifest_path="$(resolve_manifest_path_for_preflight)"

    if [ ! -f "$manifest_path" ]; then
        error_msg "Manifest not found for model compatibility check: $manifest_path"
        error_msg "Set AOS_WORKER_MANIFEST or AOS_MANIFEST_PATH to a valid file"
        return 1
    fi

    local manifest_model_id=""
    local manifest_model_hash=""
    local manifest_config_hash=""
    local manifest_tokenizer_hash=""
    local manifest_tokenizer_cfg_hash=""
    local manifest_parse_error=""

    while IFS='=' read -r key value; do
        case "$key" in
            model_id) manifest_model_id="$value" ;;
            model_hash) manifest_model_hash="$value" ;;
            config_hash) manifest_config_hash="$value" ;;
            tokenizer_hash) manifest_tokenizer_hash="$value" ;;
            tokenizer_cfg_hash) manifest_tokenizer_cfg_hash="$value" ;;
            __ERROR__) manifest_parse_error="$value" ;;
        esac
    done < <(read_manifest_base_fields "$manifest_path")

    if [ -n "$manifest_parse_error" ]; then
        error_msg "$manifest_parse_error"
        error_msg "Manifest path: $manifest_path"
        return 1
    fi

    local expected_model_id="${AOS_BASE_MODEL_ID:-$manifest_model_id}"
    if [ -z "$expected_model_id" ]; then
        expected_model_id="$(basename "$model_path")"
    fi

    if [ -n "$manifest_model_id" ] && [ "$manifest_model_id" != "$expected_model_id" ]; then
        local manifest_id_lc expected_id_lc
        manifest_id_lc="$(printf '%s' "$manifest_model_id" | tr '[:upper:]' '[:lower:]')"
        expected_id_lc="$(printf '%s' "$expected_model_id" | tr '[:upper:]' '[:lower:]')"
        # Accept quantized/packaging suffixes like "...-MLX-4bit" while still
        # rejecting unrelated model families.
        if [[ "$expected_id_lc" != "$manifest_id_lc" && "$expected_id_lc" != "$manifest_id_lc"-* ]]; then
            error_msg "Manifest/model-id mismatch: manifest=$manifest_model_id expected=$expected_model_id"
            error_msg "Manifest path: $manifest_path"
            error_msg "Model path: $model_path"
            return 1
        fi
        warning_msg "Model-id differs by variant suffix; accepting manifest=$manifest_model_id expected=$expected_model_id"
    fi

    local actual_config_hash actual_tokenizer_hash actual_tokenizer_cfg_hash
    if ! actual_config_hash="$(blake3_hash_file "$model_path/config.json")"; then
        return 1
    fi
    if ! actual_tokenizer_hash="$(blake3_hash_file "$model_path/tokenizer.json")"; then
        return 1
    fi
    if ! actual_tokenizer_cfg_hash="$(blake3_hash_file "$model_path/tokenizer_config.json")"; then
        return 1
    fi

    if [ -n "$manifest_config_hash" ] && [ "$manifest_config_hash" != "$actual_config_hash" ]; then
        error_msg "Manifest config_hash mismatch: manifest=$manifest_config_hash actual=$actual_config_hash"
        return 1
    fi

    if [ -n "$manifest_tokenizer_hash" ] && [ "$manifest_tokenizer_hash" != "$actual_tokenizer_hash" ]; then
        error_msg "Manifest tokenizer_hash mismatch: manifest=$manifest_tokenizer_hash actual=$actual_tokenizer_hash"
        return 1
    fi

    if [ -n "$manifest_tokenizer_cfg_hash" ] && [ "$manifest_tokenizer_cfg_hash" != "$actual_tokenizer_cfg_hash" ]; then
        error_msg "Manifest tokenizer_cfg_hash mismatch: manifest=$manifest_tokenizer_cfg_hash actual=$actual_tokenizer_cfg_hash"
        return 1
    fi

    if [ "${AOS_PREFLIGHT_VALIDATE_MODEL_HASH:-0}" = "1" ] && [ -n "$manifest_model_hash" ]; then
        local actual_model_hash
        if ! actual_model_hash="$(compute_model_identity_hash "$model_path")"; then
            return 1
        fi
        if [ "$manifest_model_hash" != "$actual_model_hash" ]; then
            error_msg "Manifest model_hash mismatch: manifest=$manifest_model_hash actual=$actual_model_hash"
            error_msg "Set AOS_PREFLIGHT_VALIDATE_MODEL_HASH=0 to skip this expensive check"
            return 1
        fi
    fi

    status_msg "Model/manifest compatibility check passed: $(basename "$manifest_path")"
    return 0
}

# Run all preflight checks
# Usage: run_preflight_checks [--skip-disk] [--skip-memory] [--skip-db] [--skip-model]
run_preflight_checks() {
    local skip_disk=false
    local skip_memory=false
    local skip_db=false
    local skip_model=false

    for arg in "$@"; do
        case "$arg" in
            --skip-disk) skip_disk=true ;;
            --skip-memory) skip_memory=true ;;
            --skip-db) skip_db=true ;;
            --skip-model) skip_model=true ;;
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

    if [ "$skip_model" != "true" ]; then
        if ! check_model_path_readiness; then
            failed=true
        elif ! check_model_manifest_compatibility; then
            failed=true
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

record_worker_restart_event() {
    local reason="$1"
    local exit_code="${2:-na}"
    local attempt="${3:-0}"
    local ts
    ts="$(date -Iseconds)"

    mkdir -p "$LOG_DIR" "$PID_DIR"
    printf "[%s] reason=%s exit_code=%s attempt=%s\n" "$ts" "$reason" "$exit_code" "$attempt" >>"$WORKER_RESTART_LOG"
    if [[ "$attempt" =~ ^[0-9]+$ ]]; then
        echo "$attempt" > "$WORKER_RESTART_COUNT_FILE"
    fi
    return 0
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

# Ensure dev binaries are built at most once per script invocation.
DEV_BINARIES_ENSURED=0

build_debug_binary_if_missing() {
    local binary_label="$1"
    local binary_path="$2"
    shift 2
    local build_cmd_display="$*"

    if [ -f "$binary_path" ]; then
        return 0
    fi

    local safe_label="${binary_label// /-}"
    local build_log="$LOG_DIR/build-${safe_label}.log"
    status_msg "Dev mode: missing $binary_label binary; building ($build_cmd_display)"

    if ! "$@" >"$build_log" 2>&1; then
        error_msg "Failed to build $binary_label. Command: $build_cmd_display"
        error_msg "Build log: $build_log"
        return 1
    fi

    if [ ! -f "$binary_path" ]; then
        error_msg "Build completed but $binary_label binary is still missing at $binary_path"
        error_msg "Build log: $build_log"
        return 1
    fi

    return 0
}

ensure_dev_binaries_once() {
    if ! is_dev_mode; then
        return 0
    fi

    if [ "${DEV_BINARIES_ENSURED:-0}" = "1" ]; then
        return 0
    fi
    DEV_BINARIES_ENSURED=1

    ensure_dirs

    if ! command -v cargo >/dev/null 2>&1; then
        error_msg "cargo not found. Install Rust toolchain or disable dev flags in .env."
        return 1
    fi

    local worker_backend="${AOS_MODEL_BACKEND:-mlx}"
    local -a worker_build_cmd=(cargo build -p adapteros-lora-worker)
    if [ "$worker_backend" = "mlx" ]; then
        worker_build_cmd+=(--features mlx)
    elif [ "$worker_backend" = "coreml" ]; then
        worker_build_cmd+=(--features coreml-backend,mlx)
    fi

    build_debug_binary_if_missing \
        "backend" \
        "$PROJECT_ROOT/target/debug/aos-server" \
        cargo build -p adapteros-server || return 1

    build_debug_binary_if_missing \
        "worker" \
        "$PROJECT_ROOT/target/debug/aos-worker" \
        "${worker_build_cmd[@]}" || return 1

    if [ "${SKIP_SECD:-0}" != "1" ]; then
        build_debug_binary_if_missing \
            "secd" \
            "$PROJECT_ROOT/target/debug/aos-secd" \
            cargo build -p adapteros-secd || return 1
    fi

    if [ "${SKIP_NODE:-0}" != "1" ]; then
        build_debug_binary_if_missing \
            "node" \
            "$PROJECT_ROOT/target/debug/aos-node" \
            cargo build -p adapteros-node || return 1
    fi
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

    if is_backend_launchd_managed; then
        warning_msg "Backend is launchd-managed ($BACKEND_LAUNCHD_TARGET); routing start to launchctl"
        if ! kickstart_backend_launchd; then
            error_msg "Failed to kickstart launchd backend service ($BACKEND_LAUNCHD_TARGET)"
            return 1
        fi

        local health_wait_secs="${AOS_BACKEND_HEALTH_WAIT_SECS:-20}"
        if wait_for_backend_health "$health_wait_secs"; then
            success_msg "Backend started via launchd ($BACKEND_LAUNCHD_TARGET)"
        else
            warning_msg "Backend launchd service started, but health endpoint not ready after ${health_wait_secs}s"
        fi
        return 0
    fi

    # Fast path: backend already running by canonical PID file.
    if is_running "$BACKEND_PID_FILE" "aos-server"; then
        local pid=$(get_pid "$BACKEND_PID_FILE")
        warning_msg "Backend is already running (PID: $pid)"
        return 0
    fi

    # Fast path: backend listener already active even if PID file drifted.
    local port_pid
    port_pid=$(lsof -nP -i :"$BACKEND_PORT" -sTCP:LISTEN -t 2>/dev/null | head -1)
    if [ -n "${port_pid:-}" ] && kill -0 "$port_pid" 2>/dev/null; then
        local actual
        actual=$(ps -p "$port_pid" -o comm= 2>/dev/null | xargs basename 2>/dev/null || echo "")
        if [ -n "$actual" ] && [[ "$actual" == *"aos-server"* ]]; then
            echo "$port_pid" > "$BACKEND_PID_FILE"
            if curl -s "http://127.0.0.1:$BACKEND_PORT/healthz" > /dev/null 2>&1; then
                warning_msg "Backend is already running and healthy (PID: $port_pid)"
            else
                warning_msg "Backend is already running (health pending) (PID: $port_pid)"
            fi
            return 0
        fi
    fi

    # Run preflight checks unless bypassed
    if [ -z "${AOS_SKIP_PREFLIGHT:-}" ]; then
        if ! run_preflight_checks; then
            error_msg "Preflight checks failed. Set AOS_SKIP_PREFLIGHT=1 to bypass."
            return 1
        fi
    else
        warning_msg "Preflight checks bypassed (AOS_SKIP_PREFLIGHT is set)"
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

    # Wait for health endpoint readiness (best-effort, bounded).
    local health_wait_secs="${AOS_BACKEND_HEALTH_WAIT_SECS:-20}"
    if ! [[ "$health_wait_secs" =~ ^[0-9]+$ ]]; then
        health_wait_secs=20
    fi
    local waited=0
    while [ "$waited" -lt "$health_wait_secs" ]; do
        if curl -s "http://127.0.0.1:$BACKEND_PORT/healthz" > /dev/null 2>&1; then
            break
        fi
        if ! kill -0 "$pid" 2>/dev/null; then
            error_msg "Backend exited before health endpoint became ready. Check logs: $BACKEND_LOG"
            [ "${AOS_BOOT_VERBOSE:-0}" = "1" ] && tail -30 "$BACKEND_LOG"
            rm -f "$BACKEND_PID_FILE"
            return 1
        fi
        sleep 1
        waited=$((waited + 1))
    done

    if [ "$waited" -ge "$health_wait_secs" ]; then
        warning_msg "Backend process started but health endpoint not ready after ${health_wait_secs}s"
    fi

    success_msg "Backend started (PID: $pid, Port: $BACKEND_PORT)"
    return 0
}

restart_backend() {
    local mode="${1:-graceful}"

    if is_backend_launchd_managed; then
        status_msg "Restarting Backend Server via launchd..."
        if ! kickstart_backend_launchd; then
            error_msg "Failed to restart launchd backend service ($BACKEND_LAUNCHD_TARGET)"
            return 1
        fi

        local health_wait_secs="${AOS_BACKEND_HEALTH_WAIT_SECS:-20}"
        if wait_for_backend_health "$health_wait_secs"; then
            success_msg "Backend restarted via launchd ($BACKEND_LAUNCHD_TARGET)"
        else
            warning_msg "Backend launchd service restarted, but health endpoint not ready after ${health_wait_secs}s"
        fi
        return 0
    fi

    status_msg "Restarting Backend Server (mode: ${mode})..."
    stop_backend "$mode"
    start_backend
}

stop_backend() {
    local mode="${1:-graceful}"

    if is_backend_launchd_managed; then
        status_msg "Stopping Backend (launchd-managed: $BACKEND_LAUNCHD_TARGET)..."
        if launchctl bootout "$BACKEND_LAUNCHD_TARGET" 2>/dev/null; then
            success_msg "Backend stopped via launchctl"
        else
            warning_msg "launchctl bootout failed (service may already be stopped)"
        fi
        return 0
    fi

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
# Only two DB operations remain:
#   - cleanup_stale_workers: Marks workers as crashed when their PID is dead.
#     Operates on rows created by worker HTTP registration. Called from status.
# Future work: Move stale-worker cleanup to aosctl maintenance or a background task.
# =============================================================================

# Check if database is accessible
check_database() {
    if [ ! -f "$DATABASE_PATH" ]; then
        return 1
    fi
    sqlite3 "$DATABASE_PATH" "SELECT 1;" >/dev/null 2>&1 || return 1
    return 0
}

# Return latest worker status for a PID from the control-plane workers table.
# Echoes an empty string when unavailable or unknown.
worker_status_by_pid() {
    local pid="$1"
    if ! [[ "$pid" =~ ^[0-9]+$ ]]; then
        return 1
    fi
    if ! check_database; then
        return 1
    fi
    sqlite3 "$DATABASE_PATH" \
        "SELECT status FROM workers WHERE pid=$pid ORDER BY last_seen_at DESC LIMIT 1;" 2>/dev/null | head -n 1
}

# Determine whether a worker PID is considered ready by control-plane state.
# Echoes the ready status value on success.
worker_ready_status_by_pid() {
    local pid="$1"
    local status=""
    status="$(worker_status_by_pid "$pid")"
    case "$status" in
        # "registered" is transitional: worker has announced itself but may not
        # have bound the UDS socket yet. Treat only post-registration states as ready.
        ready|running|active|healthy)
            echo "$status"
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

# Escape single quotes for SQL (basic protection)
# Used by cleanup_stale_workers.
sql_escape() {
    echo "$1" | sed "s/'/''/g"
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
        if is_running "$WORKER_PID_FILE"; then
            local existing_pid
            existing_pid=$(get_pid "$WORKER_PID_FILE")
            warning_msg "Worker socket already in use (PID: $existing_pid)"
            return 0
        fi

        local existing_pid
        existing_pid=$(lsof -t "$worker_sock" 2>/dev/null | head -1)
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

    if is_running "$WORKER_PID_FILE"; then
        local pid
        pid=$(get_pid "$WORKER_PID_FILE")

        # PID exists but socket is missing. Keep a grace window for slow startup,
        # then recycle the stuck process so supervisor can recover.
        local now_ts started_ts elapsed stale_grace
        now_ts=$(date +%s)
        started_ts=0
        if [ -f "$WORKER_START_TS_FILE" ]; then
            started_ts=$(cat "$WORKER_START_TS_FILE" 2>/dev/null || echo 0)
        fi
        if ! [[ "$started_ts" =~ ^[0-9]+$ ]] || [ "$started_ts" -le 0 ]; then
            started_ts="$now_ts"
            echo "$started_ts" > "$WORKER_START_TS_FILE"
        fi

        elapsed=$((now_ts - started_ts))
        stale_grace="${AOS_WORKER_STALE_GRACE_SECS:-180}"
        if ! [[ "$stale_grace" =~ ^[0-9]+$ ]] || [ "$stale_grace" -lt 30 ]; then
            stale_grace=180
        fi

        if [ "$elapsed" -lt "$stale_grace" ]; then
            warning_msg "Worker is still initializing without socket (PID: $pid, elapsed=${elapsed}s, grace=${stale_grace}s)"
            return 0
        fi

        warning_msg "Worker stuck without socket; recycling process (PID: $pid, elapsed=${elapsed}s)"
        stop_process "$pid" "Worker" "$WORKER_TIMEOUT" "immediate" || true
        rm -f "$WORKER_PID_FILE"
        rm -f "$WORKER_START_TS_FILE"
        rm -f "$worker_sock"
    fi

    status_msg "Starting Inference Worker..."

    # Select binary based on dev mode (dev flags → debug binary, prod → release)
    local worker_bin=""
    if ! worker_bin=$(select_worker_binary); then
        return 1
    fi

    # Determine manifest and model paths (canonical dev model)
    local manifest_path="${AOS_WORKER_MANIFEST:-${AOS_MANIFEST_PATH:-$DEFAULT_MANIFEST_PATH}}"
    local manifest_hash="${AOS_MANIFEST_HASH:-$DEFAULT_MANIFEST_HASH}"
    local model_path
    model_path="$(resolve_model_path_for_preflight)"
    local uds_path="${AOS_WORKER_SOCKET:-$PROJECT_ROOT/var/run/worker.sock}"
    # Default to MLX; requires worker to be built with multi-backend/MLX features.
    # Override via AOS_MODEL_BACKEND=metal|coreml if MLX is unavailable.
    local backend="${AOS_MODEL_BACKEND:-mlx}"

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
    date +%s > "$WORKER_START_TS_FILE"

    # Wait for socket to be created (configurable timeout, default 300 seconds).
    # Must exceed strict-mode key/material initialization on cold boots since
    # the UDS socket is bound only after startup initialization completes.
    local waited=0
    local timeout="${AOS_WORKER_TIMEOUT:-300}"
    local log_interval=5
    while [ $waited -lt "$timeout" ]; do
        # Early exit: check if process died during startup
        if ! kill -0 "$pid" 2>/dev/null; then
            error_msg "Worker process died during startup. Check logs: $WORKER_LOG"
            [ "${AOS_BOOT_VERBOSE:-0}" = "1" ] && tail -25 "$WORKER_LOG"
            rm -f "$WORKER_PID_FILE"
            rm -f "$WORKER_START_TS_FILE"
            true  # Worker self-registers via HTTP; no shell-level status update
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
        # Final race check: socket may appear at the timeout boundary.
        if [ -S "$uds_path" ]; then
            success_msg "Worker started at timeout boundary (PID: $pid, Socket: $uds_path)"
            return 0
        fi

        # Process is running but socket never created - this is a failure
        error_msg "Worker process running but socket never created after ${timeout}s (PID: $pid, Socket: $uds_path)"
        error_msg "This usually indicates a startup error. Check logs: $WORKER_LOG"
        stop_process "$pid" "Worker" "$WORKER_TIMEOUT" "immediate" || true
        rm -f "$WORKER_PID_FILE"
        rm -f "$WORKER_START_TS_FILE"
        # Worker self-registers via HTTP; no shell-level status update
        true
        return 1
    else
        error_msg "Worker failed to start. Check logs: $WORKER_LOG"
        rm -f "$WORKER_PID_FILE"
        rm -f "$WORKER_START_TS_FILE"
        true  # Worker self-registers via HTTP; no shell-level status update
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
    if is_running "$WORKER_PID_FILE"; then
        pid=$(get_pid "$WORKER_PID_FILE")
    elif [ -n "$socket_pid" ]; then
        pid="$socket_pid"
        warning_msg "Found worker via socket (PID: $pid)"
    else
        status_msg "Worker is not running"
        rm -f "$worker_sock"
        rm -f "$WORKER_PID_FILE"
        rm -f "$WORKER_START_TS_FILE"
        # Worker self-registers via HTTP; no shell-level status update
        true
        return 0
    fi

    # Worker self-registers via HTTP; status transitions handled by control plane

    stop_process "$pid" "Worker" "$WORKER_TIMEOUT" "$mode"
    
    # Worker status transitions handled by control plane
    true
    
    # Clean up socket and PID file
    rm -f "$worker_sock"
    rm -f "$WORKER_PID_FILE"
    rm -f "$WORKER_START_TS_FILE"
}

# =============================================================================
# Worker Auto-Restart with Exponential Backoff
# =============================================================================

start_worker_with_restart() {
    local max_restarts=3
    local restart_count=0
    local backoff_base=5  # seconds

    mkdir -p "$PID_DIR"
    echo "0" > "$WORKER_RESTART_COUNT_FILE"

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
                record_worker_restart_event "transient_exit" "$exit_code" "$restart_count" || true

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
                record_worker_restart_event "unknown_exit" "$exit_code" "$restart_count" || true

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
            record_worker_restart_event "start_failure" "start_failed" "$restart_count" || true

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

    local backend_ownership="service-manager"
    local launchd_runs=""
    local launchd_last_exit=""
    if command -v launchctl >/dev/null 2>&1; then
        local launchd_dump
        launchd_dump=$(launchctl print "$BACKEND_LAUNCHD_TARGET" 2>/dev/null || true)
        if [ -n "${launchd_dump:-}" ]; then
            backend_ownership="launchd"
            launchd_runs=$(printf '%s\n' "$launchd_dump" | awk -F'= ' '/runs =/{gsub(/ /, "", $2); print $2; exit}')
            launchd_last_exit=$(printf '%s\n' "$launchd_dump" | awk -F'= ' '/last exit code =/{gsub(/ /, "", $2); print $2; exit}')
        fi
    fi

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
    echo -e "          ${WHITE}Ownership mode: ${backend_ownership}${NC}"

    local backend_restart_count=""
    local backend_last_restart_cause=""
    local backend_last_restart_ts=""
    if [ -f "$BACKEND_SUPERVISION_STATE_FILE" ]; then
        backend_restart_count=$(awk -F= '/^restart_count=/{print $2}' "$BACKEND_SUPERVISION_STATE_FILE" 2>/dev/null | tail -1)
        backend_last_restart_cause=$(awk -F= '/^last_restart_cause=/{print $2}' "$BACKEND_SUPERVISION_STATE_FILE" 2>/dev/null | tail -1)
        backend_last_restart_ts=$(awk -F= '/^last_restart_ts=/{print $2}' "$BACKEND_SUPERVISION_STATE_FILE" 2>/dev/null | tail -1)
    fi

    if [ "$backend_ownership" = "launchd" ] && [[ "${launchd_runs:-}" =~ ^[0-9]+$ ]]; then
        if [ "$launchd_runs" -gt 0 ]; then
            backend_restart_count=$((launchd_runs - 1))
        else
            backend_restart_count=0
        fi
    fi

    if [ -z "${backend_restart_count:-}" ]; then
        backend_restart_count=0
    fi

    if [ -z "${backend_last_restart_cause:-}" ]; then
        if [ "$backend_ownership" = "launchd" ] && [[ "${launchd_last_exit:-}" =~ ^[0-9]+$ ]] && [ "$launchd_last_exit" -ne 0 ]; then
            backend_last_restart_cause="launchd_auto_restart_exit_${launchd_last_exit}"
        else
            backend_last_restart_cause="none_recorded"
        fi
    fi

    if [ -n "${backend_last_restart_ts:-}" ]; then
        echo -e "          ${WHITE}Backend restarts: ${backend_restart_count} (last cause: ${backend_last_restart_cause} at ${backend_last_restart_ts})${NC}"
    else
        echo -e "          ${WHITE}Backend restarts: ${backend_restart_count} (last cause: ${backend_last_restart_cause})${NC}"
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
    local restart_count_hint=""
    
    # Clean up stale workers first
    cleanup_stale_workers || true

    if [ -f "$WORKER_RESTART_COUNT_FILE" ]; then
        local restart_count_value
        restart_count_value=$(cat "$WORKER_RESTART_COUNT_FILE" 2>/dev/null || echo "0")
        if [[ "$restart_count_value" =~ ^[0-9]+$ ]] && [ "$restart_count_value" -gt 0 ]; then
            restart_count_hint=" ${YELLOW}[restarts: $restart_count_value]${NC}"
        fi
    fi
    
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
        if is_running "$WORKER_PID_FILE"; then
            worker_pid=$(get_pid "$WORKER_PID_FILE")
        else
            worker_pid=$(lsof -t "$worker_sock" 2>/dev/null | head -1)
        fi
        if [ -n "$worker_pid" ] && kill -0 "$worker_pid" 2>/dev/null; then
            local status_line="${GREEN}[RUNNING]${NC} Inference Worker (PID: $worker_pid, Socket: $worker_sock)"
            if [ -n "$db_status" ]; then
                status_line="$status_line ${CYAN}[DB: $db_status]${NC}"
            fi
            if [ -n "$worker_id" ]; then
                status_line="$status_line ${WHITE}(ID: ${worker_id:0:8}...)${NC}"
            fi
            if [ -n "$restart_count_hint" ]; then
                status_line="$status_line$restart_count_hint"
            fi
            echo -e "$status_line"
        else
            echo -e "${YELLOW}[STALE]${NC} Worker socket exists but process not found"
        fi
    elif is_running "$WORKER_PID_FILE"; then
        local pid=$(get_pid "$WORKER_PID_FILE")
        local cp_lifecycle=""
        cp_lifecycle="$(worker_status_by_pid "$pid" || true)"
        local status_line=""
        if [ -n "$cp_lifecycle" ]; then
            case "$cp_lifecycle" in
                registered)
                    status_line="${YELLOW}[WARMING]${NC} Inference Worker (PID: $pid, Control Plane: registered (transitional), socket not ready)"
                    ;;
                ready|running|active|healthy|serving)
                    status_line="${YELLOW}[DEGRADED]${NC} Inference Worker (PID: $pid, Control Plane: $cp_lifecycle, socket not ready)"
                    ;;
                *)
                    status_line="${YELLOW}[WARMING]${NC} Inference Worker (PID: $pid, Control Plane: $cp_lifecycle, socket not ready)"
                    ;;
            esac
        else
            status_line="${YELLOW}[STARTING]${NC} Inference Worker (PID: $pid, socket not ready)"
        fi
        if [ -n "$db_status" ]; then
            status_line="$status_line ${CYAN}[DB: $db_status]${NC}"
        fi
        if [ -n "$restart_count_hint" ]; then
            status_line="$status_line$restart_count_hint"
        fi
        echo -e "$status_line"
    else
        local status_line="${WHITE}[STOPPED]${NC} Inference Worker (optional)"
        if [ -n "$db_status" ] && [ "$db_status" != "stopped" ]; then
            status_line="$status_line ${YELLOW}[DB: $db_status]${NC}"
        fi
        echo -e "$status_line"
    fi

    if [ -f "$TRAINING_WORKER_DEGRADED_FILE" ]; then
        local training_degraded_reason
        training_degraded_reason=$(head -n 1 "$TRAINING_WORKER_DEGRADED_FILE" 2>/dev/null || true)
        if [ -z "$training_degraded_reason" ]; then
            training_degraded_reason="managed training worker fallback active"
        fi
        echo -e "          ${YELLOW}Training worker degraded: ${training_degraded_reason}${NC}"
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
    echo "  $0 preflight [flags]      Run startup preflight checks"
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
    echo "  $0 preflight              # Run disk/memory/db/model checks"
}

# =============================================================================
# Main Entry Point
# =============================================================================

# Rotate service-manager.log at script start if it exceeds ~5MB
mkdir -p "$LOG_DIR"
ensure_backend_log_alias
rotate_log_if_large "$SCRIPT_LOG" 5242880

if [ $# -lt 1 ]; then
    usage
    exit 1
fi

COMMAND="$1"
SERVICE="${2:-}"
MODE="${3:-graceful}"

LOCK_REQUIRED=0
case "$COMMAND" in
    start|stop|restart|start-all|stop-all|start-backend)
        LOCK_REQUIRED=1
        ;;
esac

if [ "$LOCK_REQUIRED" -eq 1 ]; then
    if ! acquire_service_control_lock; then
        exit 1
    fi
fi

case "$COMMAND" in
    start)
        if ! ensure_dev_binaries_once; then
            exit 1
        fi
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
        if ! ensure_dev_binaries_once; then
            exit 1
        fi
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
        if ! ensure_dev_binaries_once; then
            exit 1
        fi
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
        if ! ensure_dev_binaries_once; then
            exit 1
        fi
        # Start backend only (shortcut)
        start_backend
        ;;
    preflight)
        run_preflight_checks "${@:2}"
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
