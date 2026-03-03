#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
source "$PROJECT_ROOT/scripts/lib/model-config.sh"

mkdir -p "$PROJECT_ROOT/var/logs"
mkdir -p "$PROJECT_ROOT/var/run"
mkdir -p "$PROJECT_ROOT/var/tmp"

# Load .env without overriding already-set launchd environment.
if [ -f "$PROJECT_ROOT/scripts/lib/env-loader.sh" ]; then
    # shellcheck disable=SC1091
    source "$PROJECT_ROOT/scripts/lib/env-loader.sh"
    export SCRIPT_DIR="$PROJECT_ROOT"
    load_env_file "$PROJECT_ROOT/.env" --no-override || true
fi

aos_resolve_model_runtime_env "$PROJECT_ROOT"

CONFIG_PATH="${AOS_CONFIG_PATH:-$PROJECT_ROOT/configs/cp.toml}"
if [ ! -f "$CONFIG_PATH" ]; then
    echo "[launchd-backend] Config path does not exist: $CONFIG_PATH" >&2
    exit 1
fi

DEBUG_BIN="$PROJECT_ROOT/target/debug/aos-server"
RELEASE_BIN="$PROJECT_ROOT/target/release/aos-server"
SERVER_BIN="${AOS_LAUNCHD_BACKEND_BIN:-}"

if [ -z "$SERVER_BIN" ]; then
    if [ "${AOS_DEV_NO_AUTH:-0}" = "1" ] && [ -x "$DEBUG_BIN" ]; then
        SERVER_BIN="$DEBUG_BIN"
    elif [ -x "$RELEASE_BIN" ]; then
        SERVER_BIN="$RELEASE_BIN"
    elif [ -x "$DEBUG_BIN" ]; then
        SERVER_BIN="$DEBUG_BIN"
    fi
fi

if [ -z "$SERVER_BIN" ] || [ ! -x "$SERVER_BIN" ]; then
    echo "[launchd-backend] No executable backend binary found." >&2
    echo "[launchd-backend] Build with: cargo build -p adapteros-server" >&2
    exit 1
fi

export DATABASE_URL="${DATABASE_URL:-sqlite://$PROJECT_ROOT/var/aos-cp.sqlite3}"
export RUST_LOG="${RUST_LOG:-info}"

if [ -z "${AOS_MANIFEST_PATH:-}" ] && [ -n "${AOS_WORKER_MANIFEST:-}" ]; then
    export AOS_MANIFEST_PATH="$AOS_WORKER_MANIFEST"
fi

if [ -z "${AOS_MANIFEST_PATH:-}" ] || [ ! -f "${AOS_MANIFEST_PATH:-}" ]; then
    echo "[launchd-backend] Manifest path missing or invalid: ${AOS_MANIFEST_PATH:-<unset>}" >&2
    exit 1
fi

if [ -d "$PROJECT_ROOT/models" ] && [ -z "${AOS_MLX_FFI_MODEL:-}" ]; then
    model_path="$(find "$PROJECT_ROOT/models" -maxdepth 1 -type d ! -name "models" | head -1 || true)"
    if [ -n "${model_path:-}" ]; then
        export AOS_MLX_FFI_MODEL="$model_path"
    fi
fi

DRIFT_ARGS=()
if [ "${AOS_DEV_SKIP_DRIFT_CHECK:-0}" = "1" ]; then
    DRIFT_ARGS+=(--skip-drift-check)
fi

echo "$$" > "$PROJECT_ROOT/var/backend.pid"
if [ "${#DRIFT_ARGS[@]}" -gt 0 ]; then
    exec "$SERVER_BIN" --config "$CONFIG_PATH" "${DRIFT_ARGS[@]}"
fi

exec "$SERVER_BIN" --config "$CONFIG_PATH"
