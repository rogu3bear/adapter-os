#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

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

if [ -z "${AOS_MANIFEST_PATH:-}" ]; then
    DEFAULT_MODEL_DIR="$PROJECT_ROOT/var/models/Qwen3.5-27B"
    DEFAULT_MANIFEST_PATH="$PROJECT_ROOT/manifests/qwen7b-4bit-mlx-base-only.yaml"
    FIXTURE_MANIFEST="$PROJECT_ROOT/crates/adapteros-server-api/tests/fixtures/mlx/Mistral-7B-Instruct-4bit/config.json"
    DEV_MANIFEST_JSON="$DEFAULT_MODEL_DIR/config.json"

    if [ -f "$DEFAULT_MANIFEST_PATH" ]; then
        export AOS_MANIFEST_PATH="$DEFAULT_MANIFEST_PATH"
    elif [ -f "$DEV_MANIFEST_JSON" ]; then
        export AOS_MANIFEST_PATH="$DEV_MANIFEST_JSON"
    elif [ -f "$FIXTURE_MANIFEST" ]; then
        export AOS_MANIFEST_PATH="$FIXTURE_MANIFEST"
    fi
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
