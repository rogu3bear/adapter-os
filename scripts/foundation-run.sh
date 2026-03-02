#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
source "$ROOT_DIR/scripts/lib/build-targets.sh"
source "$ROOT_DIR/scripts/lib/http.sh"

CLEAN_MODE="default"
HEADLESS=0
STRICT_READY="${FOUNDATION_RUN_STRICT_READY:-${FOUNDATION_SMOKE_STRICT_READY:-0}}"
ALLOW_DEV_BYPASS="${FOUNDATION_RUN_ALLOW_DEV_BYPASS:-${FOUNDATION_SMOKE_ALLOW_DEV_BYPASS:-0}}"
BUILD_SCOPE="${FOUNDATION_BUILD_SCOPE:-server}"

PID_FILE=""
LOG_FILE=""
OWNED_BACKEND_PID=""

usage() {
  cat <<'EOF'
Usage: scripts/foundation-run.sh [--full-clean] [--no-clean] [--headless] [--strict-ready] [--allow-dev-bypass] [--workspace] [--help]

Options:
  --full-clean  Run scripts/fresh-build.sh --full-clean before build/start.
  --no-clean    Skip scripts/fresh-build.sh.
  --headless    Skip UI smoke path (/). UI assets are still ensured before build.
  --strict-ready  Forward strict readiness checks to scripts/foundation-smoke.sh.
  --allow-dev-bypass  Allow readiness_mode=dev_bypass when strict-ready is enabled.
  --workspace   Build full workspace (default: server-only).
  --help        Show this help message.
EOF
}

log() {
  printf "[foundation-run] %s\n" "$*"
}

fail() {
  local message="$1"
  local hint="${2:-}"
  printf "[foundation-run] ERROR: %s\n" "$message" >&2
  if [ -n "$hint" ]; then
    printf "[foundation-run] HINT: %s\n" "$hint" >&2
  fi
  exit 1
}

require_cmd() {
  local cmd="$1"
  local hint="$2"
  command -v "$cmd" >/dev/null 2>&1 || fail "Missing command: $cmd" "$hint"
}

require_file() {
  local path="$1"
  local hint="$2"
  [ -f "$path" ] || fail "Missing required file: $path" "$hint"
}

is_truthy() {
  case "$(echo "${1:-}" | tr '[:upper:]' '[:lower:]')" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

ensure_sqlx_compile_schema_db() {
  local schema_db="target/sqlx-compile-schema.sqlite3"
  local migrations_dir="migrations"
  local migration

  if [ -f "$schema_db" ]; then
    shopt -s nullglob
    for migration in "$migrations_dir"/[0-9]*_*.sql; do
      if [ "$migration" -nt "$schema_db" ]; then
        rm -f "$schema_db"
        break
      fi
    done
    shopt -u nullglob
  fi

  if [ -f "$schema_db" ]; then
    echo "$schema_db"
    return
  fi

  require_cmd sqlite3 "Install sqlite3 to generate SQLx compile schema database."

  mkdir -p "$(dirname "$schema_db")"
  rm -f "$schema_db"

  shopt -s nullglob
  for migration in "$migrations_dir"/[0-9]*_*.sql; do
    sqlite3 -bail "$schema_db" <"$migration" >/dev/null
  done
  shopt -u nullglob

  echo "$schema_db"
}

sqlite_path_from_url() {
  local db_url="$1"
  case "$db_url" in
    sqlite://*)
      echo "${db_url#sqlite://}"
      ;;
    sqlite:*)
      echo "${db_url#sqlite:}"
      ;;
    *)
      echo ""
      ;;
  esac
}

read_server_port() {
  local config_path="$1"
  if [ -n "${AOS_SERVER_PORT:-}" ]; then
    echo "${AOS_SERVER_PORT}"
    return
  fi

  local port
  port="$(
    awk '
      $0 ~ /^\[server\]/ { in_server = 1; next }
      in_server && $0 ~ /^\[/ { in_server = 0 }
      in_server && $0 ~ /^[[:space:]]*port[[:space:]]*=/ {
        gsub(/#.*/, "", $0)
        split($0, a, "=")
        gsub(/[[:space:]]/, "", a[2])
        print a[2]
        exit
      }
    ' "$config_path" 2>/dev/null || true
  )"

  if [[ "$port" =~ ^[0-9]+$ ]]; then
    echo "$port"
  else
    echo "8080"
  fi
}

read_base_model_value() {
  local config_path="$1"
  local key="$2"
  local fallback="$3"
  local value

  value="$(
    awk -v wanted="$key" '
      $0 ~ /^\[base_model\]/ { in_base_model = 1; next }
      in_base_model && $0 ~ /^\[/ { in_base_model = 0 }
      in_base_model {
        line = $0
        sub(/#.*/, "", line)
        if (match(line, "^[[:space:]]*" wanted "[[:space:]]*=")) {
          sub(/^[^=]*=/, "", line)
          gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)
          gsub(/^"|"$/, "", line)
          print line
          exit
        }
      }
    ' "$config_path" 2>/dev/null || true
  )"

  if [ -n "$value" ]; then
    echo "$value"
  else
    echo "$fallback"
  fi
}

aos_path_is_tmp() {
  local candidate="${1:-}"
  local absolute="$candidate"

  [ -n "$absolute" ] || return 1

  case "$absolute" in
    /*) ;;
    *) absolute="$PWD/${absolute#./}" ;;
  esac

  case "$absolute" in
    /tmp/*|/private/tmp/*) return 0 ;;
    *) return 1 ;;
  esac
}

ensure_bootstrap_model_path() {
  local config_path="$1"
  local model_path="${AOS_MODEL_PATH:-}"
  local cache_root
  local base_model_id
  local derived_model_path=0

  if [ -z "$model_path" ]; then
    cache_root="${AOS_MODEL_CACHE_DIR:-$(read_base_model_value "$config_path" "cache_root" "var/models")}"
    base_model_id="${AOS_BASE_MODEL_ID:-$(read_base_model_value "$config_path" "id" "Qwen3.5-27B")}"
    model_path="${cache_root%/}/${base_model_id}"
    export AOS_MODEL_CACHE_DIR="$cache_root"
    export AOS_BASE_MODEL_ID="$base_model_id"
    derived_model_path=1
  fi

  if [ "$derived_model_path" -eq 1 ] && aos_path_is_tmp "$model_path"; then
    local fallback_cache_root="${AOS_BOOTSTRAP_MODEL_CACHE_DIR:-}"
    if [ -z "$fallback_cache_root" ]; then
      if [ -n "${XDG_CACHE_HOME:-}" ]; then
        fallback_cache_root="${XDG_CACHE_HOME%/}/adapteros/models"
      elif [ -n "${HOME:-}" ]; then
        fallback_cache_root="${HOME%/}/.cache/adapteros/models"
      else
        fail "Cannot derive non-/tmp model cache path for backend startup." "Set AOS_BOOTSTRAP_MODEL_CACHE_DIR to a writable non-/tmp directory."
      fi
    fi

    model_path="${fallback_cache_root%/}/${base_model_id}"
    export AOS_MODEL_CACHE_DIR="${fallback_cache_root%/}"
    log "Derived model cache path is under /tmp; using fallback cache root: ${AOS_MODEL_CACHE_DIR}"
  fi

  if [ ! -d "$model_path" ]; then
    log "Creating local model path for startup bootstrap: $model_path"
    mkdir -p "$model_path"
  fi

  export AOS_MODEL_PATH="$model_path"
}

ensure_dev_jwt_secret() {
  if [ -n "${AOS_DEV_JWT_SECRET:-}" ]; then
    return
  fi
  export AOS_DEV_JWT_SECRET="adapteros-local-dev-smoke-secret"
  log "AOS_DEV_JWT_SECRET not set; using deterministic local dev secret for backend startup."
}

ensure_bootstrap_manifest_path() {
  if [ -n "${AOS_MANIFEST_PATH:-}" ]; then
    if aos_path_is_tmp "${AOS_MANIFEST_PATH}"; then
      fail "AOS_MANIFEST_PATH points to /tmp, which backend startup rejects." "Set AOS_MANIFEST_PATH to a non-/tmp manifest path."
    fi
    return
  fi

  if ! aos_path_is_tmp "$ROOT_DIR"; then
    return
  fi

  local source_manifest="manifests/qwen7b-4bit-mlx-base-only.yaml"
  [ -f "$source_manifest" ] || fail "Default manifest not found: $source_manifest" "Set AOS_MANIFEST_PATH to a valid non-/tmp manifest path."

  local fallback_manifest_dir="${AOS_BOOTSTRAP_MANIFEST_DIR:-}"
  if [ -z "$fallback_manifest_dir" ]; then
    if [ -n "${XDG_CACHE_HOME:-}" ]; then
      fallback_manifest_dir="${XDG_CACHE_HOME%/}/adapteros/manifests"
    elif [ -n "${HOME:-}" ]; then
      fallback_manifest_dir="${HOME%/}/.cache/adapteros/manifests"
    else
      fail "Cannot derive non-/tmp manifest directory for backend startup." "Set AOS_BOOTSTRAP_MANIFEST_DIR to a writable non-/tmp directory."
    fi
  fi

  mkdir -p "$fallback_manifest_dir"
  local manifest_target="${fallback_manifest_dir%/}/$(basename "$source_manifest")"
  cp "$source_manifest" "$manifest_target"
  export AOS_MANIFEST_PATH="$manifest_target"
  log "Repo is under /tmp; using fallback manifest path: $AOS_MANIFEST_PATH"
}

resolve_server_binary() {
  local candidate=""
  if candidate="$(aos_resolve_binary aos-server debug server 2>/dev/null)" && [ -x "$candidate" ]; then
    echo "$candidate"
    return 0
  fi
  if candidate="$(aos_resolve_binary aos-server release server 2>/dev/null)" && [ -x "$candidate" ]; then
    echo "$candidate"
    return 0
  fi
  return 1
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM

  if [ -n "${OWNED_BACKEND_PID:-}" ] && kill -0 "$OWNED_BACKEND_PID" 2>/dev/null; then
    log "Stopping backend pid=$OWNED_BACKEND_PID"
    kill "$OWNED_BACKEND_PID" 2>/dev/null || true
    wait "$OWNED_BACKEND_PID" 2>/dev/null || true
  fi

  if [ -n "${OWNED_BACKEND_PID:-}" ] && [ -n "${PID_FILE:-}" ] && [ -f "$PID_FILE" ]; then
    local pid_in_file
    pid_in_file="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [ "$pid_in_file" = "$OWNED_BACKEND_PID" ]; then
      rm -f "$PID_FILE"
    fi
  fi

  exit "$exit_code"
}
trap cleanup EXIT INT TERM

while [ $# -gt 0 ]; do
  case "$1" in
    --full-clean)
      CLEAN_MODE="full"
      shift
      ;;
    --no-clean)
      CLEAN_MODE="none"
      shift
      ;;
    --headless)
      HEADLESS=1
      shift
      ;;
    --strict-ready)
      STRICT_READY=1
      shift
      ;;
    --allow-dev-bypass)
      ALLOW_DEV_BYPASS=1
      shift
      ;;
    --workspace)
      BUILD_SCOPE="workspace"
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      fail "Unknown option: $1" "Run scripts/foundation-run.sh --help for valid flags."
      ;;
  esac
done

[ -f "Cargo.toml" ] || fail "Run from repo root (missing Cargo.toml)." "cd to the adapter-os repository root and retry."

require_cmd bash "Install bash and retry."
require_cmd cargo "Install Rust/Cargo from https://rustup.rs/ and retry."
require_cmd curl "Install curl and retry."

require_file "scripts/fresh-build.sh" "Sync scripts/ and retry."
require_file "scripts/foundation-smoke.sh" "Create scripts/foundation-smoke.sh and retry."
require_file "scripts/ci/check_ui_assets.sh" "Sync scripts/ci/check_ui_assets.sh and retry."
require_file "scripts/build-ui.sh" "Sync scripts/build-ui.sh and retry."

mkdir -p var/run var/logs var/tmp

CONFIG_PATH="${AOS_CONFIG:-configs/cp.toml}"
[ -f "$CONFIG_PATH" ] || fail "Config not found: $CONFIG_PATH" "Set AOS_CONFIG to a valid repo-relative config path."

if [ "$CLEAN_MODE" = "none" ]; then
  log "Skipping clean (--no-clean)."
else
  CLEAN_ARGS=()
  if [ "$CLEAN_MODE" = "full" ]; then
    CLEAN_ARGS+=(--full-clean)
  fi
  log "Running clean step: scripts/fresh-build.sh ${CLEAN_ARGS[*]:-}"
  if [ "${#CLEAN_ARGS[@]}" -gt 0 ]; then
    if ! bash scripts/fresh-build.sh "${CLEAN_ARGS[@]}"; then
      fail "fresh-build step failed." "Fix cleanup failures first, or retry with --no-clean if you intentionally want to skip it."
    fi
  else
    if ! bash scripts/fresh-build.sh; then
      fail "fresh-build step failed." "Fix cleanup failures first, or retry with --no-clean if you intentionally want to skip it."
    fi
  fi
fi

log "Checking UI assets."
if ! bash scripts/ci/check_ui_assets.sh; then
  log "UI asset check failed; rebuilding UI assets."
  bash scripts/build-ui.sh || fail "UI build failed." "Run scripts/build-ui.sh directly and fix the reported error."
  bash scripts/ci/check_ui_assets.sh || fail "UI assets still invalid after rebuild." "Inspect scripts/ci/check_ui_assets.sh output and fix missing/broken static assets."
fi

if [ -n "${SQLX_OFFLINE+x}" ]; then
  SQLX_OFFLINE_VALUE="${SQLX_OFFLINE}"
else
  SQLX_OFFLINE_VALUE="false"
fi

if [ "$BUILD_SCOPE" = "workspace" ]; then
  BUILD_TARGET=(--workspace)
else
  BUILD_TARGET=(-p adapteros-server)
fi

if [ "$SQLX_OFFLINE_VALUE" = "false" ]; then
  SQLX_SCHEMA_DB="$(ensure_sqlx_compile_schema_db)"
  DATABASE_URL_VALUE="${DATABASE_URL:-}"
  if [ -z "$DATABASE_URL_VALUE" ]; then
    DATABASE_URL_VALUE="sqlite://${SQLX_SCHEMA_DB}"
  else
    SQLITE_DB_PATH="$(sqlite_path_from_url "$DATABASE_URL_VALUE")"
    if [ -n "$SQLITE_DB_PATH" ] && [ ! -f "$SQLITE_DB_PATH" ]; then
      fail "DATABASE_URL points to missing sqlite file (${SQLITE_DB_PATH})." "Fix DATABASE_URL or unset it to use the generated SQLx compile schema DB."
    fi
  fi
  log "Building (cargo build ${BUILD_TARGET[*]}) with SQLX_OFFLINE=false and DATABASE_URL=${DATABASE_URL_VALUE}."
  SQLX_OFFLINE=false DATABASE_URL="${DATABASE_URL_VALUE}" cargo build "${BUILD_TARGET[@]}" || fail "Build failed." "Fix compiler errors, then rerun scripts/foundation-run.sh."
else
  log "Building (cargo build ${BUILD_TARGET[*]}) with SQLX_OFFLINE=${SQLX_OFFLINE_VALUE}."
  SQLX_OFFLINE="${SQLX_OFFLINE_VALUE}" cargo build "${BUILD_TARGET[@]}" || fail "Build failed." "Fix compiler errors, then rerun scripts/foundation-run.sh."
fi

SERVER_BIN="$(resolve_server_binary || true)"
[ -n "$SERVER_BIN" ] || fail "Could not resolve backend binary." "Expected target/{debug,release}/aos-server; run cargo build -p adapteros-server and retry."

SERVER_PORT="$(read_server_port "$CONFIG_PATH")"
BASE_URL="${AOS_SERVER_URL:-http://127.0.0.1:${SERVER_PORT}}"

export AOS_HTTP_CONNECT_TIMEOUT_S="${FOUNDATION_RUN_CONNECT_TIMEOUT_SECONDS:-1}"
export AOS_HTTP_MAX_TIME_S="${FOUNDATION_RUN_HTTP_TIMEOUT_SECONDS:-2}"
export AOS_HTTP_TMP_DIR="var/tmp/http/foundation-run"

healthz_code() {
  local code="000"
  if ! aos_http_request GET "${BASE_URL%/}/healthz" >/dev/null; then
    code="${AOS_HTTP_STATUS:-000}"
  else
    code="${AOS_HTTP_STATUS:-000}"
  fi
  echo "$code"
}

PID_FILE="var/run/foundation-backend.pid"
LOG_FILE="var/logs/foundation-backend.log"

if [ -f "$PID_FILE" ]; then
  stale_pid="$(cat "$PID_FILE" 2>/dev/null || true)"
  if [ -n "$stale_pid" ] && kill -0 "$stale_pid" 2>/dev/null; then
    fail "PID file already points to a running backend (pid=$stale_pid)." "Stop that process or remove var/run/foundation-backend.pid before rerunning."
  fi
  rm -f "$PID_FILE"
fi

if [ "$(healthz_code)" = "200" ]; then
  fail "Backend already responds at ${BASE_URL%/}/healthz." "Stop the existing backend or rerun without --no-clean so fresh-build can clear it."
fi

ensure_bootstrap_model_path "$CONFIG_PATH"
ensure_dev_jwt_secret
ensure_bootstrap_manifest_path

log "Starting backend: $SERVER_BIN --config $CONFIG_PATH"
"$SERVER_BIN" --config "$CONFIG_PATH" >"$LOG_FILE" 2>&1 &
OWNED_BACKEND_PID="$!"
echo "$OWNED_BACKEND_PID" >"$PID_FILE"

START_TIMEOUT_SECONDS="${FOUNDATION_START_TIMEOUT_SECONDS:-30}"
elapsed=0
while [ "$elapsed" -lt "$START_TIMEOUT_SECONDS" ]; do
  if ! kill -0 "$OWNED_BACKEND_PID" 2>/dev/null; then
    fail "Backend exited during startup." "Check var/logs/foundation-backend.log for the crash reason."
  fi
  if [ "$(healthz_code)" = "200" ]; then
    break
  fi
  sleep 1
  elapsed=$((elapsed + 1))
done

if [ "$(healthz_code)" != "200" ]; then
  fail "Backend did not become reachable at ${BASE_URL%/}/healthz within ${START_TIMEOUT_SECONDS}s." "Inspect var/logs/foundation-backend.log and fix startup errors."
fi

SMOKE_CMD=(bash scripts/foundation-smoke.sh --server-url "$BASE_URL")
if [ "$HEADLESS" -eq 1 ]; then
  SMOKE_CMD+=(--headless)
fi
if is_truthy "$STRICT_READY"; then
  log "Smoke readiness mode: strict (allow_dev_bypass=$(is_truthy "$ALLOW_DEV_BYPASS" && echo true || echo false))"
  SMOKE_CMD+=(--strict-ready)
  if is_truthy "$ALLOW_DEV_BYPASS"; then
    SMOKE_CMD+=(--allow-dev-bypass)
  fi
fi
SMOKE_CMD+=(--no-start)

log "Running smoke checks."
"${SMOKE_CMD[@]}" || fail "Smoke checks failed." "Read var/logs/foundation-backend.log and rerun scripts/foundation-smoke.sh for focused debugging."

echo ""
echo "[foundation-run] Stabilization run complete."
echo "[foundation-run] Endpoint: ${BASE_URL%/}"
echo "[foundation-run] Health:   ${BASE_URL%/}/healthz"
echo "[foundation-run] Readyz:   ${BASE_URL%/}/readyz"
if [ "$HEADLESS" -eq 0 ]; then
  echo "[foundation-run] UI:       ${BASE_URL%/}/"
fi
echo "[foundation-run] Log:      var/logs/foundation-backend.log"
echo "[foundation-run] PID file: var/run/foundation-backend.pid"
echo "[foundation-run] Next: press Ctrl-C to stop and clean up this backend process."
echo ""

wait "$OWNED_BACKEND_PID"
