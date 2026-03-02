#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

AOS_SERVER_URL="${AOS_SERVER_URL:-http://127.0.0.1:18080}"
AOS_SERVER_URL="${AOS_SERVER_URL%/}"
AOS_TENANT_ID="${AOS_TENANT_ID:-default}"
AOS_CONFIG="${AOS_CONFIG:-configs/cp.toml}"

AUTH_HEADER=("-H" "Authorization: Bearer dev-bypass" "-H" "X-Tenant-ID: ${AOS_TENANT_ID}")
AUTO_START=1
HEADLESS=0
STARTED_STACK=0

OWNED_BACKEND_PID=""
PID_FILE=""
LOG_FILE=""
HTTP_STATUS=""
HTTP_BODY_FILE=""

usage() {
  cat <<'USAGE'
Usage: scripts/functional-path-smoke.sh [--server-url URL] [--no-start] [--headless] [--help]

Options:
  --server-url URL  Override server URL (default: AOS_SERVER_URL or http://127.0.0.1:18080)
  --no-start        Require an already-running server.
  --headless        Accepted for compatibility (no UI checks are performed).
  --help            Show this help.
USAGE
}

log() {
  printf '[functional-smoke] %s\n' "$*"
}

die() {
  local msg="$1"
  local hint="${2:-}"
  printf '[functional-smoke] ERROR: %s\n' "$msg" >&2
  if [ -n "$hint" ]; then
    printf '[functional-smoke] HINT: %s\n' "$hint" >&2
  fi
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "Missing command: $1" "Install $1 and retry."
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
    ' "$config_path" 2>/dev/null
  )"

  if [ -n "$value" ]; then
    echo "$value"
  else
    echo "$fallback"
  fi
}

ensure_bootstrap_model_path() {
  local model_path="${AOS_MODEL_PATH:-}"

  if [ -z "$model_path" ]; then
    local cache_root
    local base_model_id
    cache_root="${AOS_MODEL_CACHE_DIR:-$(read_base_model_value "$AOS_CONFIG" "cache_root" "var/models")}"
    base_model_id="${AOS_BASE_MODEL_ID:-$(read_base_model_value "$AOS_CONFIG" "id" "Qwen3.5-27B")}"
    model_path="${cache_root%/}/${base_model_id}"
    export AOS_MODEL_CACHE_DIR="$cache_root"
    export AOS_BASE_MODEL_ID="$base_model_id"
  fi

  if [ -z "${AOS_BASE_MODEL_ID:-}" ]; then
    export AOS_BASE_MODEL_ID="$(basename "$model_path")"
  fi

  if [ ! -d "$model_path" ]; then
    mkdir -p "$model_path"
  fi

  export AOS_MODEL_PATH="$model_path"
}

normalize_runtime_path() {
  local path_value="$1"

  [ -n "$path_value" ] || return 1

  if [[ "$path_value" = /* ]]; then
    printf '%s\n' "$path_value"
    return 0
  fi

  printf '%s\n' "${ROOT_DIR}/${path_value}"
}

cleanup() {
  local rc=$?
  if [ "$STARTED_STACK" -eq 1 ]; then
    log "Stopping owned stack"
    if ! "${ROOT_DIR}/start" down >/dev/null 2>&1; then
      log "WARNING: start down failed; services may still be running"
    fi
  fi

  if [ -n "$OWNED_BACKEND_PID" ] && kill -0 "$OWNED_BACKEND_PID" 2>/dev/null; then
    log "Stopping backend pid=$OWNED_BACKEND_PID"
    kill "$OWNED_BACKEND_PID"
    set +e
    wait "$OWNED_BACKEND_PID" 2>/dev/null
    set -e
  fi

  if [ -n "$OWNED_BACKEND_PID" ] && [ -n "$PID_FILE" ] && [ -f "$PID_FILE" ]; then
    local pid_in_file
    pid_in_file=""
    if [ -f "$PID_FILE" ]; then
      pid_in_file="$(cat "$PID_FILE" 2>/dev/null)"
    fi
    if [ "$pid_in_file" = "$OWNED_BACKEND_PID" ]; then
      rm -f "$PID_FILE"
    fi
  fi

  if [ -n "$HTTP_BODY_FILE" ] && [ -f "$HTTP_BODY_FILE" ]; then
    rm -f "$HTTP_BODY_FILE"
  fi

  exit "$rc"
}
trap cleanup EXIT INT TERM

http_request() {
  local method="$1"
  local url="$2"
  local body="${3:-}"
  local out_file
  local status

  out_file="$(mktemp "${ROOT_DIR}/var/tmp/functional-smoke.http.XXXXXX")"

  if [ -n "$body" ]; then
    status="$(curl -sS -o "$out_file" -w '%{http_code}' -X "$method" "${AUTH_HEADER[@]}" -H 'Content-Type: application/json' -d "$body" "$url")"
  else
    status="$(curl -sS -o "$out_file" -w '%{http_code}' -X "$method" "${AUTH_HEADER[@]}" "$url")"
  fi

  if [ -n "$HTTP_BODY_FILE" ] && [ -f "$HTTP_BODY_FILE" ]; then
    rm -f "$HTTP_BODY_FILE"
  fi

  HTTP_STATUS="$status"
  HTTP_BODY_FILE="$out_file"
}

worker_uds_request() {
  local method="$1"
  local path="$2"
  local body="${3:-}"
  local socket_path="${AOS_WORKER_SOCKET:-${ROOT_DIR}/var/run/worker.sock}"
  local out_file
  local status

  [ -S "$socket_path" ] || die "Worker socket not available: ${socket_path}" "Start local worker or set AOS_WORKER_SOCKET."

  out_file="$(mktemp "${ROOT_DIR}/var/tmp/functional-smoke.worker-http.XXXXXX")"
  if [ -n "$body" ]; then
    status="$(curl -sS --unix-socket "$socket_path" -o "$out_file" -w '%{http_code}' -X "$method" -H 'Content-Type: application/json' -d "$body" "http://localhost${path}")"
  else
    status="$(curl -sS --unix-socket "$socket_path" -o "$out_file" -w '%{http_code}' -X "$method" "http://localhost${path}")"
  fi

  if [ -n "$HTTP_BODY_FILE" ] && [ -f "$HTTP_BODY_FILE" ]; then
    rm -f "$HTTP_BODY_FILE"
  fi

  HTTP_STATUS="$status"
  HTTP_BODY_FILE="$out_file"
}

healthz_code() {
  curl -sS \
    -o /dev/null \
    -w "%{http_code}" \
    --connect-timeout 1 \
    --max-time 5 \
    "${AOS_SERVER_URL}/healthz" 2>/dev/null || echo "000"
}

assert_worker_command_succeeded() {
  local action="$1"
  local parsed
  local ok
  local message

  if [ "$HTTP_STATUS" != "200" ]; then
    die "Worker command failed for ${action} (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
  fi

  parsed="$(python3 - "$HTTP_BODY_FILE" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)

print("success=" + str(bool(data.get("success"))).lower())
print("message=" + str(data.get("message") or ""))
PY
)" || die "Failed to parse worker response for ${action}" "Response: $(cat "$HTTP_BODY_FILE")"

  ok="$(printf '%s\n' "$parsed" | awk -F= '/^success=/{print $2}')"
  message="$(printf '%s\n' "$parsed" | awk -F= '/^message=/{print $2}')"
  [ "$ok" = "true" ] || die "Worker reported failure for ${action}" "Message: ${message}"
}

prime_worker_adapter() {
  local adapter_id="$1"
  local adapter_hash="$2"
  local preload_payload
  local swap_payload

  preload_payload="$(python3 - "$adapter_id" "$adapter_hash" <<'PY'
import json
import sys

print(json.dumps({
    "type": "preload",
    "adapter_id": sys.argv[1],
    "hash": sys.argv[2],
}))
PY
)"
  worker_uds_request POST "/adapter/command" "$preload_payload"
  assert_worker_command_succeeded "preload:${adapter_id}"

  swap_payload="$(python3 - "$adapter_id" <<'PY'
import json
import sys

print(json.dumps({
    "type": "swap",
    "add_ids": [sys.argv[1]],
    "remove_ids": [],
    "expected_stack_hash": None,
}))
PY
)"
  worker_uds_request POST "/adapter/command" "$swap_payload"
  assert_worker_command_succeeded "swap:${adapter_id}"

  worker_uds_request POST "/adapter/command" '{"type":"verify_stack"}'
  assert_worker_command_succeeded "verify_stack"
}

fetch_adapter_runtime_fields() {
  local adapter_id="$1"
  local detail_summary

  http_request GET "${AOS_SERVER_URL}/v1/adapters/${adapter_id}/detail"
  if [ "$HTTP_STATUS" != "200" ]; then
    die "Adapter detail lookup failed for ${adapter_id} (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
  fi

  detail_summary="$(python3 - "$HTTP_BODY_FILE" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)

print("hash_b3=" + str(data.get("hash_b3") or data.get("content_hash_b3") or ""))
print("base_model_id=" + str(data.get("base_model_id") or ""))
print("tier=" + str(data.get("tier") or "warm"))
print("rank=" + str(data.get("rank") or ""))
print("alpha=" + str(data.get("alpha") or ""))
print("lora_strength=" + str(data.get("lora_strength") or ""))
print("aos_file_path=" + str(data.get("aos_file_path") or ""))
PY
)" || die "Failed to parse adapter detail for ${adapter_id}" "Response: $(cat "$HTTP_BODY_FILE")"

  printf '%s\n' "$detail_summary"
}

fetch_model_runtime_fields() {
  local model_id="$1"
  local model_summary

  http_request GET "${AOS_SERVER_URL}/internal/models"
  if [ "$HTTP_STATUS" != "200" ]; then
    die "Model registry lookup failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
  fi

  model_summary="$(python3 - "$HTTP_BODY_FILE" "$model_id" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)

target = sys.argv[2]
items = data.get("models", []) if isinstance(data, dict) else (data if isinstance(data, list) else [])
match = None

for item in items:
    if not isinstance(item, dict):
        continue
    if str(item.get("id") or "") == target:
        match = item
        break

if match is None:
    for item in items:
        if not isinstance(item, dict):
            continue
        if str(item.get("name") or "") == target:
            match = item
            break

if match is None:
    print("missing=true")
else:
    print("missing=false")
    print("id=" + str(match.get("id") or ""))
    print("name=" + str(match.get("name") or ""))
    print("model_path=" + str(match.get("model_path") or ""))
PY
)" || die "Failed to parse model registry payload" "Response: $(cat "$HTTP_BODY_FILE")"

  missing="$(printf '%s\n' "$model_summary" | awk -F= '/^missing=/{print $2}')"
  if [ "$missing" = "true" ]; then
    die "Model not found for selected adapter base model id: ${model_id}" "Verify /internal/models contains this model id."
  fi

  printf '%s\n' "$model_summary"
}

resolve_manifest_yaml() {
  local candidate
  local candidates=()
  local model_manifest=""

  if model_manifest="$(resolve_model_manifest_candidate)"; then
    candidates+=("$model_manifest")
  fi

  if [ -n "${AOS_WORKER_MANIFEST:-}" ]; then
    candidates+=("${AOS_WORKER_MANIFEST}")
  fi
  if [ -n "${AOS_MANIFEST_PATH:-}" ]; then
    candidates+=("${AOS_MANIFEST_PATH}")
  fi

  candidates+=(
    "${ROOT_DIR}/manifests/mistral7b-instruct-v0.3-mlx-4bit.yaml"
    "${ROOT_DIR}/manifests/llama3.2-3b-instruct-4bit.yaml"
    "${ROOT_DIR}/manifests/qwen7b-4bit-mlx-base-only.yaml"
  )

  for candidate in "${candidates[@]}"; do
    if [ -n "$candidate" ] && [ -f "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  return 1
}

resolve_model_manifest_candidate() {
  local model_id_lc
  model_id_lc="$(printf '%s' "${AOS_BASE_MODEL_ID:-}" | tr '[:upper:]' '[:lower:]')"

  if [[ "$model_id_lc" == *qwen* ]]; then
    if [ -f "${ROOT_DIR}/manifests/qwen7b-4bit-mlx-base-only.yaml" ]; then
      printf '%s\n' "${ROOT_DIR}/manifests/qwen7b-4bit-mlx-base-only.yaml"
      return 0
    fi
    if [ -f "${ROOT_DIR}/manifests/qwen7b-mlx.yaml" ]; then
      printf '%s\n' "${ROOT_DIR}/manifests/qwen7b-mlx.yaml"
      return 0
    fi
  elif [[ "$model_id_lc" == *mistral* ]]; then
    if [ -f "${ROOT_DIR}/manifests/mistral7b-instruct-v0.3-mlx-4bit.yaml" ]; then
      printf '%s\n' "${ROOT_DIR}/manifests/mistral7b-instruct-v0.3-mlx-4bit.yaml"
      return 0
    fi
  elif [[ "$model_id_lc" == *llama* ]]; then
    if [ -f "${ROOT_DIR}/manifests/llama3.2-3b-instruct-4bit.yaml" ]; then
      printf '%s\n' "${ROOT_DIR}/manifests/llama3.2-3b-instruct-4bit.yaml"
      return 0
    fi
  fi

  return 1
}

resolve_adapter_object_bundle() {
  local adapter_hash="$1"
  local hash_prefix_2="${adapter_hash:0:2}"
  local hash_prefix_8="${adapter_hash:2:8}"
  local candidate
  local candidates=(
    "${ROOT_DIR}/var/datasets/adapters/objects/${hash_prefix_2}/${hash_prefix_8}/${adapter_hash}.aos"
    "${ROOT_DIR}/var/adapters/objects/${hash_prefix_2}/${hash_prefix_8}/${adapter_hash}.aos"
  )

  for candidate in "${candidates[@]}"; do
    if [ -f "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  return 1
}

resolve_adapter_bundle_from_db() {
  local adapter_id="$1"
  local db_path
  local escaped_adapter_id
  local aos_file_path

  db_path="${AOS_DB_PATH:-${ROOT_DIR}/var/aos-cp.sqlite3}"
  [ -f "$db_path" ] || return 1
  command -v sqlite3 >/dev/null 2>&1 || return 1

  escaped_adapter_id="${adapter_id//\'/\'\'}"
  aos_file_path="$(sqlite3 "$db_path" "SELECT aos_file_path FROM adapters WHERE adapter_id = '${escaped_adapter_id}' ORDER BY updated_at DESC LIMIT 1;" 2>/dev/null | head -n 1)"

  [ -n "$aos_file_path" ] || return 1

  if [ -f "$aos_file_path" ]; then
    printf '%s\n' "$aos_file_path"
    return 0
  fi

  if [[ "$aos_file_path" != /* ]] && [ -f "${ROOT_DIR}/${aos_file_path}" ]; then
    printf '%s\n' "${ROOT_DIR}/${aos_file_path}"
    return 0
  fi

  return 1
}

ensure_worker_adapter_bundle() {
  local adapter_id="$1"
  local adapter_hash="$2"
  local source_hint="${3:-}"
  local dest_dir="${ROOT_DIR}/var/adapters/repo/${AOS_TENANT_ID}"
  local dest_path="${dest_dir}/${adapter_id}.aos"
  local source_path=""

  mkdir -p "$dest_dir"

  if [ -f "$dest_path" ]; then
    printf '%s\n' "$dest_path"
    return 0
  fi

  if [ -n "$source_hint" ] && [ -f "$source_hint" ]; then
    source_path="$source_hint"
  else
    if source_path="$(resolve_adapter_object_bundle "$adapter_hash")"; then
      :
    elif source_path="$(resolve_adapter_bundle_from_db "$adapter_id")"; then
      :
    else
      source_path=""
    fi
    [ -n "$source_path" ] || die "Unable to locate adapter bundle for ${adapter_id}" "Checked object stores and SQLite adapter metadata (hash=${adapter_hash})."
  fi

  cp "$source_path" "$dest_path"
  printf '%s\n' "$dest_path"
}

build_runtime_worker_manifest() {
  local template_path="$1"
  local output_path="$2"
  local adapter_id="$3"
  local adapter_hash="$4"
  local adapter_tier="$5"
  local adapter_rank="$6"
  local adapter_alpha="$7"
  local adapter_lora_strength="$8"

  python3 - "$template_path" "$output_path" "$adapter_id" "$adapter_hash" "$adapter_tier" "$adapter_rank" "$adapter_alpha" "$adapter_lora_strength" <<'PY'
import json
import sys
import yaml

template_path = sys.argv[1]
output_path = sys.argv[2]
adapter_id = sys.argv[3]
adapter_hash = sys.argv[4]
adapter_tier_raw = (sys.argv[5] or "").strip().lower()
adapter_rank_raw = sys.argv[6] or "0"
adapter_alpha_raw = sys.argv[7] or "0"
adapter_lora_strength_raw = sys.argv[8] or ""

if adapter_tier_raw in {"persistent", "ephemeral"}:
    adapter_tier = adapter_tier_raw
elif adapter_tier_raw in {"warm", "hot", "ready", "active", "resident"}:
    adapter_tier = "persistent"
else:
    adapter_tier = "persistent"

try:
    adapter_rank = int(float(adapter_rank_raw))
except Exception:
    raise SystemExit(f"invalid adapter rank: {adapter_rank_raw}")

try:
    adapter_alpha = float(adapter_alpha_raw)
except Exception:
    raise SystemExit(f"invalid adapter alpha: {adapter_alpha_raw}")

if adapter_rank <= 0:
    raise SystemExit(f"invalid adapter rank: {adapter_rank_raw}")
if adapter_alpha <= 0:
    raise SystemExit(f"invalid adapter alpha: {adapter_alpha_raw}")

with open(template_path, "r", encoding="utf-8") as fh:
    manifest = yaml.safe_load(fh)

if not isinstance(manifest, dict):
    raise SystemExit("manifest template is not a mapping")

adapter_entry = {
    "id": adapter_id,
    "hash": adapter_hash,
    "tier": adapter_tier,
    "rank": adapter_rank,
    "alpha": adapter_alpha,
    "target_modules": [],
}

if adapter_lora_strength_raw:
    try:
        adapter_entry["lora_strength"] = float(adapter_lora_strength_raw)
    except Exception:
        pass

manifest["adapters"] = [adapter_entry]

with open(output_path, "w", encoding="utf-8") as fh:
    yaml.safe_dump(manifest, fh, sort_keys=False)
PY
}

restart_stack_with_worker_manifest() {
  local worker_manifest="$1"
  local manifest_seed
  local runtime_plan_id

  if [ "$STARTED_STACK" -ne 1 ]; then
    return 0
  fi

  manifest_seed="$(python3 - "$worker_manifest" <<'PY'
import hashlib
import pathlib
import sys

payload = pathlib.Path(sys.argv[1]).read_bytes()
print(hashlib.sha256(payload).hexdigest()[:16])
PY
)"
  runtime_plan_id="functional-smoke-runtime-${manifest_seed}"

  log "Restarting owned stack with runtime worker manifest"
  "${ROOT_DIR}/start" down >/dev/null 2>&1 || die "Failed to stop stack before runtime manifest restart"
  AOS_MANIFEST_PATH="$worker_manifest" AOS_WORKER_MANIFEST="$worker_manifest" PLAN_ID="$runtime_plan_id" SKIP_SECD=1 SKIP_NODE=1 "${ROOT_DIR}/start" up --quick || die "Failed to restart stack with runtime worker manifest" "Check var/logs/backend.log and var/logs/worker.log."

  export AOS_MANIFEST_PATH="$worker_manifest"
  export AOS_WORKER_MANIFEST="$worker_manifest"
  export PLAN_ID="$runtime_plan_id"

  if [ "$(healthz_code)" != "200" ]; then
    die "Server not reachable after runtime manifest restart"
  fi
}

ensure_model_loaded_for_inference() {
  local model_id="$1"
  local model_status
  local started
  local timeout

  [ -n "$model_id" ] || die "Missing base model id for selected adapter"

  http_request POST "${AOS_SERVER_URL}/v1/models/${model_id}/load"
  case "$HTTP_STATUS" in
    200|202|409) ;;
    *)
      die "Model load request failed for ${model_id} (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
      ;;
  esac

  started="$(date +%s)"
  timeout="${FUNCTIONAL_SMOKE_MODEL_LOAD_TIMEOUT_SECONDS:-45}"
  while true; do
    http_request GET "${AOS_SERVER_URL}/v1/models/${model_id}/status"
    if [ "$HTTP_STATUS" != "200" ]; then
      die "Model status check failed for ${model_id} (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
    fi

    model_status="$(python3 - "$HTTP_BODY_FILE" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)

status = data.get("status") or data.get("model_load_status") or data.get("state") or ""
print(status)
PY
)"

    case "$model_status" in
      ready)
        return 0
        ;;
      error|failed|unloaded|no-model)
        die "Model ${model_id} is not ready for inference (status=${model_status})" "Response: $(cat "$HTTP_BODY_FILE")"
        ;;
    esac

    if [ $(( $(date +%s) - started )) -gt "$timeout" ]; then
      die "Model ${model_id} did not become ready within ${timeout}s (last_status=${model_status})" "Response: $(cat "$HTTP_BODY_FILE")"
    fi

    sleep 2
  done
}

while [ $# -gt 0 ]; do
  case "$1" in
    --server-url)
      [ $# -ge 2 ] || die "--server-url requires a value"
      AOS_SERVER_URL="${2%/}"
      shift 2
      ;;
    --no-start)
      AUTO_START=0
      shift
      ;;
    --headless)
      HEADLESS=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      die "Unknown option: $1" "Run scripts/functional-path-smoke.sh --help for valid flags."
      ;;
  esac
done

require_cmd curl
require_cmd python3
require_cmd awk

mkdir -p var/tmp var/run var/logs

if ! [ -f "$AOS_CONFIG" ]; then
  die "Config file not found: $AOS_CONFIG" "Set AOS_CONFIG to a valid config path."
fi

SMOKE_TIMEOUT_SECONDS="${FUNCTIONAL_SMOKE_TIMEOUT_SECONDS:-55}"
START_TIMEOUT_SECONDS="${FUNCTIONAL_SMOKE_START_TIMEOUT_SECONDS:-30}"

if ! [[ "$SMOKE_TIMEOUT_SECONDS" =~ ^[0-9]+$ ]] || [ "$SMOKE_TIMEOUT_SECONDS" -lt 1 ]; then
  die "FUNCTIONAL_SMOKE_TIMEOUT_SECONDS must be a positive integer"
fi
if ! [[ "$START_TIMEOUT_SECONDS" =~ ^[0-9]+$ ]] || [ "$START_TIMEOUT_SECONDS" -lt 1 ]; then
  die "FUNCTIONAL_SMOKE_START_TIMEOUT_SECONDS must be a positive integer"
fi

if [ "$(healthz_code)" != "200" ]; then
  if [ "$AUTO_START" -eq 0 ]; then
    die "Server not reachable at ${AOS_SERVER_URL}/healthz" "Start the server first, or rerun without --no-start."
  fi

  require_cmd cargo

  server_port="$(python3 - "$AOS_SERVER_URL" <<'PY'
import sys
from urllib.parse import urlparse

u = urlparse(sys.argv[1])
if not u.scheme or not u.netloc:
    raise SystemExit(1)
port = u.port
if port is None:
    if u.scheme == "http":
        port = 80
    elif u.scheme == "https":
        port = 443
    else:
        raise SystemExit(1)
print(port)
PY
)" || die "Unable to parse server URL: ${AOS_SERVER_URL}" "Use --server-url with an explicit URL."

  export AOS_SERVER_PORT="$server_port"
  export AOS_TENANT_ID
  export AOS_DEV_NO_AUTH=1
  export AOS_MODEL_BACKEND="${AOS_MODEL_BACKEND:-mlx}"
  ensure_bootstrap_model_path

  if model_manifest_candidate="$(resolve_model_manifest_candidate)"; then
    export AOS_MANIFEST_PATH="$model_manifest_candidate"
    export AOS_WORKER_MANIFEST="$model_manifest_candidate"
  fi

  plan_manifest_seed="${AOS_WORKER_MANIFEST:-${AOS_MANIFEST_PATH:-default-manifest}}"
  plan_suffix="$(python3 - "$plan_manifest_seed" <<'PY'
import hashlib
import pathlib
import sys

seed = sys.argv[1]
path = pathlib.Path(seed)
if path.is_file():
    data = path.read_bytes()
else:
    data = seed.encode("utf-8")
print(hashlib.sha256(data).hexdigest()[:16])
PY
)"
  export PLAN_ID="${PLAN_ID:-functional-smoke-${plan_suffix}}"

  if [ "${AOS_MODEL_BACKEND}" = "mlx" ]; then
    log "Ensuring worker binary supports MLX backend"
    cargo build -p adapteros-lora-worker --features mlx >/dev/null
  fi

  log "Using plan ID: ${PLAN_ID}"
  log "Starting stack: ./start up --quick"
  SKIP_SECD=1 SKIP_NODE=1 "${ROOT_DIR}/start" up --quick
  STARTED_STACK=1

  if [ "$(healthz_code)" != "200" ]; then
    die "Stack started but /healthz is not reachable at ${AOS_SERVER_URL}" "Check var/logs/backend.log and var/logs/worker.log."
  fi
else
  log "Using running server at ${AOS_SERVER_URL}"
fi

log "Checking /readyz"
http_request GET "${AOS_SERVER_URL}/readyz"
if [ "$HTTP_STATUS" != "200" ] && [ "$HTTP_STATUS" != "503" ]; then
  die "/readyz expected 200 or 503, got ${HTTP_STATUS}" "Response: $(cat "$HTTP_BODY_FILE")"
fi

log "Selecting adapter from /v1/adapters"
http_request GET "${AOS_SERVER_URL}/v1/adapters"
if [ "$HTTP_STATUS" != "200" ]; then
  die "Failed to list adapters (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
fi

adapter_csv="$(python3 - "$HTTP_BODY_FILE" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)

items = data.get('adapters', []) if isinstance(data, dict) else (data if isinstance(data, list) else [])
out = []
for item in items:
    aid = item.get('adapter_id')
    if aid:
      out.append(aid)
print(','.join(out))
PY
)"

if [ -z "$adapter_csv" ]; then
  die "No adapters found" "Run scripts/golden_path_adapter_chat.sh first to create a trained adapter."
fi

adapter_one="${adapter_csv%%,*}"
adapter_two=""
if [ "$adapter_csv" != "$adapter_one" ]; then
  adapter_two="${adapter_csv#*,}"
  adapter_two="${adapter_two%%,*}"
fi

selected_adapter="$adapter_one"
swap_performed=0
if [ -n "$adapter_two" ] && [ "$adapter_two" != "$adapter_one" ]; then
  log "Performing swap ${adapter_one} -> ${adapter_two}"
  swap_body="$(python3 - <<'PY'
import json
print(json.dumps({
    "old_adapter_id": "__OLD__",
    "new_adapter_id": "__NEW__",
    "dry_run": False
}))
PY
)"
  swap_body="${swap_body/__OLD__/${adapter_one}}"
  swap_body="${swap_body/__NEW__/${adapter_two}}"
  http_request POST "${AOS_SERVER_URL}/v1/adapters/swap" "$swap_body"

  if [ "$HTTP_STATUS" = "200" ]; then
    swap_performed=1
    selected_adapter="$adapter_two"
    log "Swap succeeded; using swapped-in adapter ${selected_adapter} for deterministic receipt validation"
  else
    log "Swap unavailable (HTTP ${HTTP_STATUS}); continuing with explicit adapter selection"
  fi
fi

runtime_fields="$(fetch_adapter_runtime_fields "$selected_adapter")"
selected_adapter_hash="$(printf '%s\n' "$runtime_fields" | awk -F= '/^hash_b3=/{print $2}')"
selected_base_model_id="$(printf '%s\n' "$runtime_fields" | awk -F= '/^base_model_id=/{print $2}')"
selected_adapter_tier="$(printf '%s\n' "$runtime_fields" | awk -F= '/^tier=/{print $2}')"
selected_adapter_rank="$(printf '%s\n' "$runtime_fields" | awk -F= '/^rank=/{print $2}')"
selected_adapter_alpha="$(printf '%s\n' "$runtime_fields" | awk -F= '/^alpha=/{print $2}')"
selected_adapter_lora_strength="$(printf '%s\n' "$runtime_fields" | awk -F= '/^lora_strength=/{print $2}')"
selected_adapter_aos_path="$(printf '%s\n' "$runtime_fields" | awk -F= '/^aos_file_path=/{print $2}')"
[ -n "$selected_adapter_hash" ] || die "Selected adapter is missing hash_b3/content_hash_b3" "adapter_id=${selected_adapter}"
[ -n "$selected_base_model_id" ] || die "Selected adapter is missing base_model_id" "adapter_id=${selected_adapter}"
[ -n "$selected_adapter_rank" ] || die "Selected adapter is missing rank" "adapter_id=${selected_adapter}"
[ -n "$selected_adapter_alpha" ] || die "Selected adapter is missing alpha" "adapter_id=${selected_adapter}"

selected_model_fields="$(fetch_model_runtime_fields "$selected_base_model_id")"
resolved_model_id="$(printf '%s\n' "$selected_model_fields" | awk -F= '/^id=/{print $2}')"
resolved_model_name="$(printf '%s\n' "$selected_model_fields" | awk -F= '/^name=/{print $2}')"
selected_model_path_raw="$(printf '%s\n' "$selected_model_fields" | awk -F= '/^model_path=/{print $2}')"
[ -n "$resolved_model_id" ] || die "Resolved model record is missing id" "base_model_id=${selected_base_model_id}"
[ -n "$selected_model_path_raw" ] || die "Resolved model record is missing model_path" "model_id=${resolved_model_id}"
selected_model_path="$(normalize_runtime_path "$selected_model_path_raw")" || die "Failed to normalize model path for selected adapter" "model_path=${selected_model_path_raw}"
[ -d "$selected_model_path" ] || die "Resolved model path does not exist for selected adapter base model" "model_path=${selected_model_path}"
selected_manifest_model_hint="$resolved_model_name"
[ -n "$selected_manifest_model_hint" ] || selected_manifest_model_hint="$resolved_model_id"
export AOS_BASE_MODEL_ID="$selected_manifest_model_hint"
export AOS_MODEL_PATH="$selected_model_path"
export AOS_MODEL_CACHE_DIR="$(dirname "$selected_model_path")"
log "Resolved adapter base model ${selected_base_model_id} to ${resolved_model_name} at ${selected_model_path}"

if [ "$STARTED_STACK" -eq 1 ]; then
  manifest_template="$(resolve_manifest_yaml)" || die "Unable to resolve base manifest template" "Set AOS_WORKER_MANIFEST or AOS_MANIFEST_PATH to an existing YAML manifest."
  ensure_worker_adapter_bundle "$selected_adapter" "$selected_adapter_hash" "$selected_adapter_aos_path" >/dev/null
  runtime_manifest="${ROOT_DIR}/var/tmp/functional-smoke-worker-${selected_adapter}.yaml"
  build_runtime_worker_manifest \
    "$manifest_template" \
    "$runtime_manifest" \
    "$selected_adapter" \
    "$selected_adapter_hash" \
    "$selected_adapter_tier" \
    "$selected_adapter_rank" \
    "$selected_adapter_alpha" \
    "$selected_adapter_lora_strength"
  restart_stack_with_worker_manifest "$runtime_manifest"
else
  log "Using externally managed stack; skipping runtime worker manifest rewrite"
fi

if [ -S "${AOS_WORKER_SOCKET:-${ROOT_DIR}/var/run/worker.sock}" ]; then
  log "Priming worker adapter stack for ${selected_adapter}"
  prime_worker_adapter "$selected_adapter" "$selected_adapter_hash"
else
  log "Worker socket unavailable; skipping direct worker priming"
fi

log "Ensuring model is loaded for inference: ${selected_base_model_id}"
ensure_model_loaded_for_inference "$selected_base_model_id"

deadline=$(( $(date +%s) + SMOKE_TIMEOUT_SECONDS ))

log "Running one inference with adapter ${selected_adapter}"
infer_body="$(python3 - <<'PY'
import json
print(json.dumps({
    "prompt": "Respond with OK.",
    "adapters": ["__ADAPTER__"],
    "max_tokens": 8,
    "backend": "mlx",
    "seed": 1337,
}))
PY
)"
infer_body="${infer_body/__ADAPTER__/${selected_adapter}}"
http_request POST "${AOS_SERVER_URL}/v1/infer" "$infer_body"
if [ "$HTTP_STATUS" != "200" ]; then
  die "Inference failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
fi

smoke_summary="$(python3 - "$HTTP_BODY_FILE" "$selected_adapter" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)

selected_adapter_id = sys.argv[2]
text = (data.get('text') or data.get('response') or '').strip()
run = data.get('run_receipt') or {}
det = data.get('deterministic_receipt') or {}
adapters_used = det.get('adapters_used') or data.get('adapters_used') or []
if not text:
    print('empty_text', file=sys.stderr)
    sys.exit(1)
if not run:
    print('missing_run_receipt', file=sys.stderr)
    sys.exit(1)
if not det:
    print('missing_deterministic_receipt', file=sys.stderr)
    sys.exit(1)
trace_id = run.get('trace_id') or ''
receipt_digest = run.get('receipt_digest') or ''
if not trace_id or not receipt_digest:
    print('missing_trace_or_digest', file=sys.stderr)
    sys.exit(1)
if selected_adapter_id and selected_adapter_id not in adapters_used:
    print('selected_adapter_not_in_receipt', file=sys.stderr)
    sys.exit(1)
print(f"response_text={text}")
print(f"trace_id={trace_id}")
print(f"run_receipt_digest={receipt_digest}")
print(f"adapters_used={adapters_used}")
PY
)" || die "Inference response missing required receipt fields"

printf '%s\n' "$smoke_summary"

trace_id="$(printf '%s\n' "$smoke_summary" | awk -F= '/^trace_id=/{print $2}')"
run_receipt_digest="$(printf '%s\n' "$smoke_summary" | awk -F= '/^run_receipt_digest=/{print $2}')"

if [ "$(date +%s)" -ge "$deadline" ]; then
  die "Smoke timeout budget exhausted before receipt verification" "Increase FUNCTIONAL_SMOKE_TIMEOUT_SECONDS if needed."
fi

log "Fetching run receipt by digest"
http_request GET "${AOS_SERVER_URL}/v1/adapteros/receipts/${run_receipt_digest}"
if [ "$HTTP_STATUS" != "200" ]; then
  die "Run receipt lookup failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
fi

RUN_RECEIPT_PATH="${ROOT_DIR}/var/tmp/functional-smoke.run-receipt.json"
cp "$HTTP_BODY_FILE" "$RUN_RECEIPT_PATH"

log "Verifying inference receipt"
verify_body="$(python3 - "$trace_id" <<'PY'
import json
import sys

trace_id = (sys.argv[1] or "").strip()
if not trace_id:
    print("missing_trace_id", file=sys.stderr)
    sys.exit(1)
print(json.dumps({"trace_id": trace_id}))
PY
)" || die "Failed to build trace verification payload" "trace_id=${trace_id}"

http_request POST "${AOS_SERVER_URL}/v1/replay/verify/trace" "$verify_body"
if [ "$HTTP_STATUS" != "200" ]; then
  die "Inference receipt verification failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
fi

TRACE_VERIFY_PATH="${ROOT_DIR}/var/tmp/functional-smoke.trace-verify.json"
cp "$HTTP_BODY_FILE" "$TRACE_VERIFY_PATH"

verify_summary="$(python3 - "$TRACE_VERIFY_PATH" <<'PY'
import json
import sys
with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)
print(f"trace_verify_pass={str(bool(data.get('pass'))).lower()}")
print(f"trace_verify_receipt_match={str(bool((data.get('receipt_digest') or {}).get('matches'))).lower()}")
PY
)"
printf '%s\n' "$verify_summary"
verify_pass="$(printf '%s\n' "$verify_summary" | awk -F= '/^trace_verify_pass=/{print $2}')"
verify_receipt_match="$(printf '%s\n' "$verify_summary" | awk -F= '/^trace_verify_receipt_match=/{print $2}')"
[ "$verify_pass" = "true" ] || die "Inference receipt verification reported pass=false" "Response: $(cat "$TRACE_VERIFY_PATH")"
[ "$verify_receipt_match" = "true" ] || die "Inference receipt verification reported receipt_digest mismatch" "Response: $(cat "$TRACE_VERIFY_PATH")"

if [ "$swap_performed" -eq 1 ]; then
  log "Verifying swap receipt via audit logs"
  http_request GET "${AOS_SERVER_URL}/v1/audit/logs?action=adapter.swap&limit=20"
  if [ "$HTTP_STATUS" != "200" ]; then
    die "Swap audit lookup failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
  fi

  swap_found="$(python3 - "$HTTP_BODY_FILE" "$adapter_one" "$adapter_two" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)
old_id = sys.argv[2]
new_id = sys.argv[3]
entries = data.get('logs', []) if isinstance(data, dict) else []
for entry in entries:
    if entry.get('action') != 'adapter.swap':
        continue
    resource_id = entry.get('resource_id') or ''
    if old_id in resource_id and new_id in resource_id:
        print('true')
        break
else:
    print('false')
PY
)"
  [ "$swap_found" = "true" ] || die "Swap receipt not found in audit logs" "Expected adapter.swap entry for ${adapter_one} -> ${adapter_two}."
fi

if [ "$HEADLESS" -eq 1 ]; then
  log "Headless flag enabled"
fi

log "PASS"
