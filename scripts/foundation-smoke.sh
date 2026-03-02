#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
source "$ROOT_DIR/scripts/lib/build-targets.sh"
source "$ROOT_DIR/scripts/lib/http.sh"

HEADLESS=0
SERVER_URL=""
AUTO_START=1
STRICT_READY="${FOUNDATION_SMOKE_STRICT_READY:-0}"
ALLOW_DEV_BYPASS="${FOUNDATION_SMOKE_ALLOW_DEV_BYPASS:-0}"

OWNED_BACKEND_PID=""
PID_FILE=""
LOG_FILE=""
HEALTH_BODY=""
READY_BODY=""
ROOT_BODY=""

usage() {
  cat <<'EOF'
Usage: scripts/foundation-smoke.sh [--headless] [--server-url URL] [--no-start] [--strict-ready] [--allow-dev-bypass] [--help]

Options:
  --headless        Skip UI root-path check (/).
  --server-url URL  Override server URL (default: AOS_SERVER_URL or config-derived localhost URL).
  --no-start        Require an already-running server; do not auto-start one.
  --strict-ready    Enforce strict readiness checks (/readyz must be 200, fully healthy, non-dev-bypass by default).
  --allow-dev-bypass  Allow readiness_mode=dev_bypass in strict mode.
  --help            Show this help message.
EOF
}

log() {
  printf "[foundation-smoke] %s\n" "$*"
}

fail() {
  local message="$1"
  local hint="${2:-}"
  printf "[foundation-smoke] ERROR: %s\n" "$message" >&2
  if [ -n "$hint" ]; then
    printf "[foundation-smoke] HINT: %s\n" "$hint" >&2
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
        fail "Cannot derive non-/tmp model cache path for smoke startup." "Set AOS_BOOTSTRAP_MODEL_CACHE_DIR to a writable non-/tmp directory."
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
  log "AOS_DEV_JWT_SECRET not set; using deterministic local dev secret for smoke startup."
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
      fail "Cannot derive non-/tmp manifest directory for smoke startup." "Set AOS_BOOTSTRAP_MANIFEST_DIR to a writable non-/tmp directory."
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

while [ $# -gt 0 ]; do
  case "$1" in
    --headless)
      HEADLESS=1
      shift
      ;;
    --server-url)
      [ $# -ge 2 ] || fail "--server-url requires a value." "Pass a URL such as --server-url http://127.0.0.1:18080"
      SERVER_URL="$2"
      shift 2
      ;;
    --no-start)
      AUTO_START=0
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
    --help|-h)
      usage
      exit 0
      ;;
    *)
      fail "Unknown option: $1" "Run scripts/foundation-smoke.sh --help for valid flags."
      ;;
  esac
done

require_cmd curl "Install curl and retry."
require_cmd awk "Install awk and retry."
require_cmd python3 "Install python3 and retry."

mkdir -p var/tmp var/run var/logs

cleanup() {
  if [ -n "$OWNED_BACKEND_PID" ] && kill -0 "$OWNED_BACKEND_PID" 2>/dev/null; then
    log "Stopping backend pid=$OWNED_BACKEND_PID"
    kill "$OWNED_BACKEND_PID" 2>/dev/null || true
    wait "$OWNED_BACKEND_PID" 2>/dev/null || true
  fi

  if [ -n "$OWNED_BACKEND_PID" ] && [ -n "$PID_FILE" ] && [ -f "$PID_FILE" ]; then
    local pid_in_file
    pid_in_file="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [ "$pid_in_file" = "$OWNED_BACKEND_PID" ]; then
      rm -f "$PID_FILE"
    fi
  fi

  rm -f "$HEALTH_BODY" "$READY_BODY" "$ROOT_BODY" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

if [ -z "$SERVER_URL" ]; then
  CONFIG_PATH="${AOS_CONFIG:-configs/cp.toml}"
  require_file "$CONFIG_PATH" "Set AOS_CONFIG to a valid repo-relative config path."
  SERVER_PORT="$(read_server_port "$CONFIG_PATH")"
  SERVER_URL="${AOS_SERVER_URL:-http://127.0.0.1:${SERVER_PORT}}"
else
  CONFIG_PATH="${AOS_CONFIG:-configs/cp.toml}"
fi
BASE_URL="${SERVER_URL%/}"

SMOKE_TIMEOUT_SECONDS="${FOUNDATION_SMOKE_TIMEOUT_SECONDS:-45}"
CONNECT_TIMEOUT_SECONDS="${FOUNDATION_SMOKE_CONNECT_TIMEOUT_SECONDS:-1}"
HTTP_TIMEOUT_SECONDS="${FOUNDATION_SMOKE_HTTP_TIMEOUT_SECONDS:-4}"
START_TIMEOUT_SECONDS="${FOUNDATION_SMOKE_START_TIMEOUT_SECONDS:-45}"

export AOS_HTTP_CONNECT_TIMEOUT_S="$CONNECT_TIMEOUT_SECONDS"
export AOS_HTTP_MAX_TIME_S="$HTTP_TIMEOUT_SECONDS"
export AOS_HTTP_TMP_DIR="var/tmp/http/foundation-smoke"

if ! [[ "$SMOKE_TIMEOUT_SECONDS" =~ ^[0-9]+$ ]] || [ "$SMOKE_TIMEOUT_SECONDS" -lt 1 ]; then
  fail "FOUNDATION_SMOKE_TIMEOUT_SECONDS must be a positive integer." "Use a value under 60 seconds (default is 45)."
fi
if ! [[ "$START_TIMEOUT_SECONDS" =~ ^[0-9]+$ ]] || [ "$START_TIMEOUT_SECONDS" -lt 1 ]; then
  fail "FOUNDATION_SMOKE_START_TIMEOUT_SECONDS must be a positive integer." "Use a value under 60 seconds (default is 45)."
fi

HEALTH_BODY="var/tmp/foundation-smoke.healthz.$$.json"
READY_BODY="var/tmp/foundation-smoke.readyz.$$.json"
ROOT_BODY="var/tmp/foundation-smoke.root.$$.html"

request_code() {
  local url="$1"
  local body_file="$2"
  local code="000"
  if ! aos_http_request GET "$url" >/dev/null; then
    code="${AOS_HTTP_STATUS:-000}"
  else
    code="${AOS_HTTP_STATUS:-000}"
  fi
  if [ -n "${AOS_HTTP_BODY_PATH:-}" ] && [ -f "$AOS_HTTP_BODY_PATH" ]; then
    cp "$AOS_HTTP_BODY_PATH" "$body_file"
  else
    : > "$body_file"
  fi
  printf "%s" "$code"
}

body_snippet() {
  local body_file="$1"
  aos_http__snippet "$body_file" 200
}

parse_readyz_fields() {
  local body_file="$1"
  python3 - "$body_file" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    data = json.load(fh)

checks = data.get("checks") or {}
db = (checks.get("db") or {}).get("ok")
worker = (checks.get("worker") or {}).get("ok")
models_seeded = (checks.get("models_seeded") or {}).get("ok")
mode_raw = data.get("readiness_mode")
if isinstance(mode_raw, dict):
    mode = mode_raw.get("mode", "")
elif isinstance(mode_raw, str):
    mode = mode_raw
else:
    mode = ""

def bool_text(value):
    if value is True:
        return "true"
    if value is False:
        return "false"
    return ""

print("ready=" + bool_text(data.get("ready")))
print("db_ok=" + bool_text(db))
print("worker_ok=" + bool_text(worker))
print("models_seeded_ok=" + bool_text(models_seeded))
print("mode=" + mode)
PY
}

start_or_attach_server() {
  local initial_health_code
  initial_health_code="$(request_code "$BASE_URL/healthz" "$HEALTH_BODY")"
  if [ "$initial_health_code" = "200" ]; then
    log "Using existing server at $BASE_URL"
    return 0
  fi

  if [ "$AUTO_START" -eq 0 ]; then
    fail "Server is not reachable at $BASE_URL/healthz." "Start the server first, or rerun without --no-start."
  fi

  require_file "$CONFIG_PATH" "Set AOS_CONFIG to a valid repo-relative config path."
  local server_bin
  server_bin="$(resolve_server_binary || true)"
  [ -n "$server_bin" ] || fail "Backend binary not found." "Run cargo build -p adapteros-server, then rerun scripts/foundation-smoke.sh."

  ensure_bootstrap_model_path "$CONFIG_PATH"
  ensure_dev_jwt_secret
  ensure_bootstrap_manifest_path

  PID_FILE="var/run/foundation-smoke-backend.pid"
  LOG_FILE="var/logs/foundation-smoke-backend.log"

  if [ -f "$PID_FILE" ]; then
    local existing_pid
    existing_pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [ -n "$existing_pid" ] && kill -0 "$existing_pid" 2>/dev/null; then
      fail "PID file already points to running process (pid=$existing_pid)." "Stop that process or remove var/run/foundation-smoke-backend.pid."
    fi
    rm -f "$PID_FILE"
  fi

  log "Starting backend: $server_bin --config $CONFIG_PATH"
  "$server_bin" --config "$CONFIG_PATH" >"$LOG_FILE" 2>&1 &
  OWNED_BACKEND_PID="$!"
  echo "$OWNED_BACKEND_PID" >"$PID_FILE"

  local elapsed=0
  while [ "$elapsed" -lt "$START_TIMEOUT_SECONDS" ]; do
    if ! kill -0 "$OWNED_BACKEND_PID" 2>/dev/null; then
      fail "Backend exited during startup." "Check var/logs/foundation-smoke-backend.log for crash details."
    fi
    local health_code
    health_code="$(request_code "$BASE_URL/healthz" "$HEALTH_BODY")"
    if [ "$health_code" = "200" ]; then
      return 0
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done

  fail "Backend did not become healthy within ${START_TIMEOUT_SECONDS}s." "Inspect var/logs/foundation-smoke-backend.log."
}

log "Server: $BASE_URL"
log "Timeout budget: ${SMOKE_TIMEOUT_SECONDS}s"
if is_truthy "$STRICT_READY"; then
  log "Readiness mode: strict (allow_dev_bypass=$(is_truthy "$ALLOW_DEV_BYPASS" && echo true || echo false))"
elif is_truthy "$ALLOW_DEV_BYPASS"; then
  log "Readiness mode: non-strict (ignoring --allow-dev-bypass)"
fi
start_or_attach_server

# 1) /healthz should become 200 within budget.
health_code="000"
deadline_ts=$(( $(date +%s) + SMOKE_TIMEOUT_SECONDS ))
while [ "$(date +%s)" -lt "$deadline_ts" ]; do
  health_code="$(request_code "$BASE_URL/healthz" "$HEALTH_BODY")"
  if [ "$health_code" = "200" ]; then
    break
  fi
  sleep 1
done
if [ "$health_code" != "200" ]; then
  fail "/healthz expected 200, got $health_code." "Check var/logs/foundation-backend.log (foundation-run) or var/logs/foundation-smoke-backend.log (auto-start)."
fi
log "OK /healthz -> 200"

# 2) /readyz should satisfy readiness contract within budget.
ready_ok=0
ready_code="000"
ready_value=""
db_ok=""
worker_ok=""
models_seeded_ok=""
ready_mode=""
ready_error="/readyz did not satisfy readiness contract."
ready_deadline_ts=$(( $(date +%s) + SMOKE_TIMEOUT_SECONDS ))
while [ "$(date +%s)" -lt "$ready_deadline_ts" ]; do
  ready_code="$(request_code "$BASE_URL/readyz" "$READY_BODY")"
  if [ "$ready_code" != "200" ] && [ "$ready_code" != "503" ]; then
    ready_error="/readyz expected 200 or 503, got $ready_code."
    sleep 1
    continue
  fi

  ready_fields="$(parse_readyz_fields "$READY_BODY" 2>/dev/null || true)"
  if [ -z "$ready_fields" ]; then
    ready_error="/readyz response is not valid JSON (status $ready_code)."
    sleep 1
    continue
  fi

  ready_value="$(printf '%s\n' "$ready_fields" | awk -F= '/^ready=/{print $2}')"
  db_ok="$(printf '%s\n' "$ready_fields" | awk -F= '/^db_ok=/{print $2}')"
  worker_ok="$(printf '%s\n' "$ready_fields" | awk -F= '/^worker_ok=/{print $2}')"
  models_seeded_ok="$(printf '%s\n' "$ready_fields" | awk -F= '/^models_seeded_ok=/{print $2}')"
  ready_mode="$(printf '%s\n' "$ready_fields" | awk -F= '/^mode=/{print $2}')"

  if [ -z "$ready_value" ]; then
    ready_error="/readyz response missing boolean ready field (status $ready_code)."
    sleep 1
    continue
  fi

  if is_truthy "$STRICT_READY"; then
    if [ "$ready_code" != "200" ]; then
      ready_error="/readyz strict mode requires HTTP 200, got $ready_code."
      sleep 1
      continue
    fi
    if [ "$ready_value" != "true" ]; then
      ready_error="/readyz strict mode requires ready=true, got ready=$ready_value."
      sleep 1
      continue
    fi
    if [ "$db_ok" != "true" ]; then
      ready_error="/readyz strict mode requires checks.db.ok=true."
      sleep 1
      continue
    fi
    if [ "$worker_ok" != "true" ]; then
      ready_error="/readyz strict mode requires checks.worker.ok=true."
      sleep 1
      continue
    fi
    if [ "$models_seeded_ok" != "true" ]; then
      ready_error="/readyz strict mode requires checks.models_seeded.ok=true."
      sleep 1
      continue
    fi
    if [ "$ready_mode" = "dev_bypass" ] && ! is_truthy "$ALLOW_DEV_BYPASS"; then
      ready_error="/readyz strict mode rejected readiness_mode=dev_bypass."
      sleep 1
      continue
    fi
  else
    if [ "$ready_code" = "200" ] && [ "$ready_value" != "true" ]; then
      ready_error="/readyz returned 200 with ready=$ready_value."
      sleep 1
      continue
    fi
    if [ "$ready_code" = "503" ] && [ "$ready_value" != "false" ]; then
      ready_error="/readyz returned 503 with ready=$ready_value."
      sleep 1
      continue
    fi
  fi

  ready_ok=1
  break
done

if [ "$ready_ok" -ne 1 ]; then
  fail "$ready_error" "Inspect /readyz JSON/body: $(body_snippet "$READY_BODY")"
fi
log "OK /readyz -> $ready_code (ready=$ready_value mode=${ready_mode:-unknown} db_ok=${db_ok:-unknown} worker_ok=${worker_ok:-unknown} models_seeded_ok=${models_seeded_ok:-unknown})"

# 3) Non-headless mode validates static root path.
if [ "$HEADLESS" -eq 0 ]; then
  root_code="$(request_code "$BASE_URL/" "$ROOT_BODY")"
  if ! [[ "$root_code" =~ ^[0-9]{3}$ ]]; then
    fail "/ returned invalid HTTP status: $root_code." "Confirm backend is reachable at $BASE_URL."
  fi
  if [ "$root_code" -lt 200 ] || [ "$root_code" -ge 400 ]; then
    fail "/ expected 2xx/3xx, got $root_code (body: $(body_snippet "$ROOT_BODY"))." "Rebuild UI assets via scripts/build-ui.sh or rerun scripts/foundation-run.sh without --headless."
  fi
  log "OK / -> $root_code"
else
  log "Headless mode: skipped / check."
fi

log "PASS"
