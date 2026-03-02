#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$ROOT_DIR/scripts/lib/http.sh"

AOSCTL="${ROOT_DIR}/aosctl"
AOS_SERVER_URL="${AOS_SERVER_URL:-http://127.0.0.1:18080}"
AOS_TENANT_ID="${AOS_TENANT_ID:-default}"
AOS_SERVER_URL="${AOS_SERVER_URL%/}"

AUTH_HEADER=("-H" "Authorization: Bearer dev-bypass" "-H" "X-Tenant-ID: ${AOS_TENANT_ID}")
STARTED_STACK=0
NO_DB_RESET=0
ALLOW_SINGLE_ADAPTER=0

HTTP_STATUS=""
HTTP_BODY_FILE=""

mkdir -p "${ROOT_DIR}/var/tmp"
export AOS_HTTP_CONNECT_TIMEOUT_S="${AOS_HTTP_CONNECT_TIMEOUT_S:-2}"
export AOS_HTTP_MAX_TIME_S="${AOS_HTTP_MAX_TIME_S:-20}"
export AOS_HTTP_TMP_DIR="${AOS_HTTP_TMP_DIR:-var/tmp/http/golden-path}"

die() {
  local msg="$1"
  local hint="${2:-}"
  echo "ERROR: ${msg}" >&2
  if [ -n "$hint" ]; then
    echo "HINT: ${hint}" >&2
  fi
  exit 1
}

note() {
  echo "==> $*"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "Missing required command: $1" "Install or add $1 to PATH."
}

resolve_training_worker_binary() {
  local target_dir="${CARGO_TARGET_DIR:-target}"
  local candidates=(
    "${target_dir%/}/debug/aos-training-worker"
    "${target_dir%/}/release/aos-training-worker"
    "target/debug/aos-training-worker"
    "target/release/aos-training-worker"
  )

  local candidate
  for candidate in "${candidates[@]}"; do
    if [ -x "$candidate" ]; then
      echo "$candidate"
      return 0
    fi
  done

  return 1
}

ensure_training_worker_binary() {
  local training_worker_bin
  training_worker_bin=""
  if training_worker_bin="$(resolve_training_worker_binary)"; then
    :
  else
    training_worker_bin=""
  fi

  if [ -z "$training_worker_bin" ]; then
    note "Ensuring managed training worker binary exists"
    cargo build -p adapteros-training-worker >/dev/null
    if training_worker_bin="$(resolve_training_worker_binary)"; then
      :
    else
      training_worker_bin=""
    fi
  fi

  [ -n "$training_worker_bin" ] || die "Managed training worker binary not found after build" "Expected target/*/aos-training-worker from crate adapteros-training-worker."

  local training_worker_dir
  training_worker_dir="$(cd "$(dirname "$training_worker_bin")" && pwd -P)"
  case ":$PATH:" in
    *":${training_worker_dir}:"*) ;;
    *) export PATH="${training_worker_dir}:$PATH" ;;
  esac

  note "Using managed training worker binary: ${training_worker_bin}"
}

cleanup() {
  local rc=$?
  if [ "$STARTED_STACK" -eq 1 ]; then
    echo "==> Stopping control plane + worker"
    if ! "${ROOT_DIR}/start" down; then
      echo "WARNING: start down failed; services may still be running." >&2
    fi
  fi

  if [ -n "$HTTP_BODY_FILE" ] && [ -f "$HTTP_BODY_FILE" ]; then
    rm -f "$HTTP_BODY_FILE"
  fi

  exit "$rc"
}
trap cleanup EXIT INT TERM

usage() {
  cat <<'USAGE'
Usage: scripts/golden_path_adapter_chat.sh [--no-db-reset] [--allow-single-adapter] [--help]

Options:
  --no-db-reset           Keep existing DB state; do not remove var/aos-cp.sqlite3.
  --allow-single-adapter  Permit success when no second adapter can be prepared for swap.
  --help                  Show this help.

Default behavior is strict: requires a real adapter swap and verifies swap + inference receipts.
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --no-db-reset)
      NO_DB_RESET=1
      shift
      ;;
    --allow-single-adapter)
      ALLOW_SINGLE_ADAPTER=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      die "Unknown option: $1" "Run scripts/golden_path_adapter_chat.sh --help for valid flags."
      ;;
  esac
done

http_request() {
  local method="$1"
  local url="$2"
  local body="${3:-}"
  local status="000"

  if [ -n "$body" ]; then
    if ! aos_http_request "$method" "$url" "$body" "${AUTH_HEADER[@]}" >/dev/null; then
      status="${AOS_HTTP_STATUS:-000}"
    else
      status="${AOS_HTTP_STATUS:-000}"
    fi
  else
    if ! aos_http_request "$method" "$url" "" "${AUTH_HEADER[@]}" >/dev/null; then
      status="${AOS_HTTP_STATUS:-000}"
    else
      status="${AOS_HTTP_STATUS:-000}"
    fi
  fi

  HTTP_STATUS="$status"
  HTTP_BODY_FILE="${AOS_HTTP_BODY_PATH:-}"
  if [ -z "$HTTP_BODY_FILE" ] || [ ! -f "$HTTP_BODY_FILE" ]; then
    die "HTTP response capture missing for ${method} ${url}" "Check scripts/lib/http.sh and ensure var/tmp is writable."
  fi
}

wait_ready() {
  local ready_ok=0
  local i

  for i in $(seq 1 60); do
    http_request GET "${AOS_SERVER_URL}/readyz"
    if [ "$HTTP_STATUS" = "200" ]; then
      ready_ok=1
      break
    fi
    sleep 2
  done

  if [ "$ready_ok" -ne 1 ]; then
    die "Control plane not ready" "Check var/logs/backend.log for readiness errors."
  fi
}

training_wait() {
  local job_id="$1"
  local timeout="$2"
  local visibility_timeout="${TRAINING_JOB_VISIBILITY_TIMEOUT_SECS:-45}"
  local status_json
  local status_cmd_output
  local status
  local started
  local first_not_found_at=0

  started=$(date +%s)

  while true; do
    if ! status_cmd_output="$("$AOSCTL" --json train status "$job_id" 2>&1)"; then
      if printf '%s' "$status_cmd_output" | grep -qi "Training job not found"; then
        if [ "$first_not_found_at" -eq 0 ]; then
          first_not_found_at="$(date +%s)"
        fi
        if [ $(( $(date +%s) - first_not_found_at )) -gt "$visibility_timeout" ]; then
          die "Training job ${job_id} remained not found for ${visibility_timeout}s" "$status_cmd_output"
        fi
        sleep 2
        continue
      fi
      die "Training status command failed for job_id=${job_id}" "$status_cmd_output"
    fi
    status_json="$status_cmd_output"
    first_not_found_at=0

    if ! status="$(printf '%s' "$status_json" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("status", ""))')"; then
      die "Training status response was not valid JSON for job_id=${job_id}" "$status_json"
    fi

    if [ "$status" = "completed" ]; then
      printf '%s' "$status_json"
      return 0
    fi

    if [ "$status" = "failed" ]; then
      local err_msg
      err_msg="$(printf '%s' "$status_json" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("error_message") or d.get("error_code") or "training failed")')"
      die "Training failed: ${err_msg}" "Check var/logs/backend.log for job_id=${job_id}."
    fi

    if [ $(( $(date +%s) - started )) -gt "$timeout" ]; then
      die "Training timed out after ${timeout}s" "Check var/logs/backend.log for job_id=${job_id}."
    fi

    sleep 5
  done
}

start_training_job() {
  local dataset_version_id="$1"
  local repo_name="$2"
  local train_out

  if ! train_out="$(REPO_NAME="$repo_name" "${ROOT_DIR}/scripts/start_minimal_training.sh" "$dataset_version_id" 2>&1)"; then
    die "Training start command failed for repo=${repo_name}" "$train_out"
  fi

  local repo_id
  local job_id
  repo_id="$(printf '%s\n' "$train_out" | awk -F= '/^repo_id=/{print $2}')"
  job_id="$(printf '%s\n' "$train_out" | awk -F= '/^job_id=/{print $2}')"

  if [ -z "$repo_id" ]; then
    note "Training start output omitted repo_id; proceeding with job_id contract"
  fi
  [ -n "$job_id" ] || die "Training start did not return job_id" "Check backend logs for training start failures."

  echo "repo_id=${repo_id}"
  echo "job_id=${job_id}"
}

apply_dataset_trust_override() {
  local dataset_id="$1"
  local dataset_version_id="$2"
  local trust_override_payload
  local dataset_trust_state

  trust_override_payload="$(python3 - <<'PY'
import json
print(json.dumps({
    "override_state": "allowed",
    "reason": "functional proof explicit trust override"
}))
PY
)"
  http_request POST "${AOS_SERVER_URL}/v1/datasets/${dataset_id}/versions/${dataset_version_id}/trust-override" "$trust_override_payload"
  if [ "$HTTP_STATUS" != "200" ]; then
    die "Dataset trust override failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
  fi

  dataset_trust_state="$(python3 - "$HTTP_BODY_FILE" <<'PY'
import json
import sys
with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)
print(data.get('effective_trust_state', data.get('trust_state', '')))
PY
)"
  case "$dataset_trust_state" in
    allowed|allowed_with_warning) ;;
    *)
      die "Dataset trust override did not produce a trainable state: ${dataset_trust_state}" "Expected allowed or allowed_with_warning."
      ;;
  esac

  printf '%s\n' "$dataset_trust_state"
}

import_fallback_swap_adapter() {
  local exclude_adapter_id="$1"
  local duplicate_payload
  local imported_adapter_id

  duplicate_payload="$(python3 - <<'PY'
import json

print(json.dumps({
    "name": "functional-proof-swap-candidate"
}))
PY
)"

  http_request POST "${AOS_SERVER_URL}/v1/adapters/${exclude_adapter_id}/duplicate" "$duplicate_payload"
  if [ "$HTTP_STATUS" != "201" ] && [ "$HTTP_STATUS" != "200" ]; then
    return 1
  fi

  imported_adapter_id="$(python3 - "$HTTP_BODY_FILE" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    payload = json.load(fh)

print(payload.get("adapter_id") or payload.get("id") or "")
PY
)"
  if [ -z "$imported_adapter_id" ] || [ "$imported_adapter_id" = "$exclude_adapter_id" ]; then
    return 1
  fi

  printf '%s\n' "$imported_adapter_id"
}

parse_training_completion() {
  local status_json="$1"
  python3 - "$status_json" <<'PY'
import json
import sys

data = json.loads(sys.argv[1])

def pick(*keys):
    for key in keys:
        value = data.get(key)
        if value:
            return str(value)
    return ""

print("adapter_id=" + pick("adapter_id"))
print("aos_path=" + pick("aos_path"))
print("package_hash_b3=" + pick("package_hash_b3", "artifact_hash_b3"))
print("weights_hash_b3=" + pick("weights_hash_b3"))
print("base_model_id=" + pick("base_model_id"))
print("determinism_mode=" + pick("determinism_mode"))
PY
}

ensure_model_loaded_for_inference() {
  local model_id="$1"
  local model_status
  local started
  local timeout

  [ -n "$model_id" ] || die "Missing model_id for load readiness check" "Training status must include base_model_id."

  note "Ensuring base model is loaded for inference: ${model_id}"
  http_request POST "${AOS_SERVER_URL}/v1/models/${model_id}/load"
  case "$HTTP_STATUS" in
    200|202|409) ;;
    *)
      die "Model load request failed for ${model_id} (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
      ;;
  esac

  started="$(date +%s)"
  timeout="${MODEL_LOAD_TIMEOUT_SECS:-180}"
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

status = (
    data.get("status")
    or data.get("model_load_status")
    or data.get("state")
    or ""
)
print(status)
PY
)"

    case "$model_status" in
      ready)
        echo "model_status=ready"
        return 0
        ;;
      error|failed|unloaded|no-model)
        die "Model ${model_id} not ready for inference (status=${model_status})" "Response: $(cat "$HTTP_BODY_FILE")"
        ;;
    esac

    if [ $(( $(date +%s) - started )) -gt "$timeout" ]; then
      die "Model ${model_id} did not reach ready state within ${timeout}s (last_status=${model_status})" "Response: $(cat "$HTTP_BODY_FILE")"
    fi
    sleep 2
  done
}

find_snapshot_adapter_excluding() {
  local excluded="$1"
  local adapters_json
  local adapter_ids
  local candidate

  http_request GET "${AOS_SERVER_URL}/v1/adapters"
  [ "$HTTP_STATUS" = "200" ] || return 1
  adapters_json="$(cat "$HTTP_BODY_FILE")"

  adapter_ids="$(printf '%s' "$adapters_json" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
items = data.get("adapters", []) if isinstance(data, dict) else (data if isinstance(data, list) else [])
for item in items:
    adapter_id = item.get("adapter_id")
    if adapter_id:
        print(adapter_id)
')"

  for candidate in $adapter_ids; do
    if [ "$candidate" = "$excluded" ]; then
      continue
    fi

    http_request GET "${AOS_SERVER_URL}/v1/adapters/${candidate}/training-snapshot"
    if [ "$HTTP_STATUS" = "200" ]; then
      echo "$candidate"
      return 0
    fi
  done

  echo ""
}

ensure_adapter_swap_ready() {
  local adapter_id="$1"
  local repair_out
  local transition_out
  local detail_summary
  local lifecycle_state
  local content_hash_b3

  note "Ensuring adapter swap readiness: ${adapter_id}"

  if ! repair_out="$("$AOSCTL" adapter repair-hashes --adapter-id "$adapter_id" 2>&1)"; then
    die "Adapter hash repair failed for ${adapter_id}" "$repair_out"
  fi

  http_request GET "${AOS_SERVER_URL}/v1/adapters/${adapter_id}/detail"
  if [ "$HTTP_STATUS" != "200" ]; then
    die "Adapter detail lookup failed for ${adapter_id} (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
  fi

  detail_summary="$(python3 - "$HTTP_BODY_FILE" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)

print("lifecycle_state=" + (data.get("lifecycle_state") or ""))
print("content_hash_b3=" + (data.get("content_hash_b3") or ""))
PY
)"

  lifecycle_state="$(printf '%s\n' "$detail_summary" | awk -F= '/^lifecycle_state=/{print $2}')"
  content_hash_b3="$(printf '%s\n' "$detail_summary" | awk -F= '/^content_hash_b3=/{print $2}')"

  case "$lifecycle_state" in
    ready|active)
      ;;
    training)
      note "Promoting adapter lifecycle to ready: ${adapter_id}"
      if ! transition_out="$("$AOSCTL" adapter lifecycle-transition "$adapter_id" ready --reason "functional-proof training completion" 2>&1)"; then
        die "Lifecycle transition to ready failed for ${adapter_id}" "$transition_out"
      fi
      ;;
    draft)
      note "Promoting adapter lifecycle draft -> training -> ready: ${adapter_id}"
      if ! transition_out="$("$AOSCTL" adapter lifecycle-transition "$adapter_id" training --reason "functional-proof training lifecycle" 2>&1)"; then
        die "Lifecycle transition to training failed for ${adapter_id}" "$transition_out"
      fi
      if ! transition_out="$("$AOSCTL" adapter lifecycle-transition "$adapter_id" ready --reason "functional-proof training completion" 2>&1)"; then
        die "Lifecycle transition to ready failed for ${adapter_id}" "$transition_out"
      fi
      ;;
    *)
      die "Adapter ${adapter_id} lifecycle state is unsupported for swap: ${lifecycle_state}" "Expected draft, training, ready, or active."
      ;;
  esac

  http_request GET "${AOS_SERVER_URL}/v1/adapters/${adapter_id}/detail"
  if [ "$HTTP_STATUS" != "200" ]; then
    die "Post-transition adapter detail lookup failed for ${adapter_id} (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
  fi

  detail_summary="$(python3 - "$HTTP_BODY_FILE" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)

print("lifecycle_state=" + (data.get("lifecycle_state") or ""))
print("content_hash_b3=" + (data.get("content_hash_b3") or ""))
PY
)"

  lifecycle_state="$(printf '%s\n' "$detail_summary" | awk -F= '/^lifecycle_state=/{print $2}')"
  content_hash_b3="$(printf '%s\n' "$detail_summary" | awk -F= '/^content_hash_b3=/{print $2}')"

  case "$lifecycle_state" in
    ready|active) ;;
    *)
      die "Adapter ${adapter_id} is not swap-eligible after transitions (state=${lifecycle_state})" "Expected lifecycle_state=ready or active."
      ;;
  esac

  [ -n "$content_hash_b3" ] || die "Adapter ${adapter_id} is missing content_hash_b3 after repair" "Run ./aosctl adapter repair-hashes --adapter-id ${adapter_id} and inspect backend logs."
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

hash_b3 = data.get("hash_b3") or data.get("content_hash_b3") or ""
tier = data.get("tier") or "warm"
rank = data.get("rank")
alpha = data.get("alpha")
lora_strength = data.get("lora_strength")

print("hash_b3=" + str(hash_b3))
print("tier=" + str(tier))
print("rank=" + ("" if rank is None else str(rank)))
print("alpha=" + ("" if alpha is None else str(alpha)))
print("lora_strength=" + ("" if lora_strength is None else str(lora_strength)))
print("aos_file_path=" + str(data.get("aos_file_path") or ""))
PY
)" || die "Failed to parse adapter detail for ${adapter_id}" "Response: $(cat "$HTTP_BODY_FILE")"

  printf '%s\n' "$detail_summary"
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
    source_path="$(resolve_adapter_object_bundle "$adapter_hash")" || die "Unable to locate adapter bundle for ${adapter_id}" "Checked object stores for hash=${adapter_hash}."
  fi

  cp "$source_path" "$dest_path"
  printf '%s\n' "$dest_path"
}

build_runtime_worker_manifest() {
  local output_path="$1"
  local adapter_id="$2"
  local adapter_hash="$3"
  local adapter_tier="$4"
  local adapter_rank="$5"
  local adapter_alpha="$6"
  local adapter_lora_strength="$7"

  python3 - "$MANIFEST_YAML" "$output_path" "$adapter_id" "$adapter_hash" "$adapter_tier" "$adapter_rank" "$adapter_alpha" "$adapter_lora_strength" <<'PY'
import sys
import yaml

template_path = sys.argv[1]
output_path = sys.argv[2]
adapter_id = sys.argv[3]
adapter_hash = sys.argv[4]
adapter_tier_raw = (sys.argv[5] or "").strip().lower()
adapter_rank_raw = sys.argv[6] or "0"
adapter_alpha_raw = sys.argv[7] or "0"
adapter_lora_strength_raw = sys.argv[8]

if adapter_tier_raw in {"persistent", "ephemeral"}:
    adapter_tier = adapter_tier_raw
elif adapter_tier_raw in {"warm", "hot", "ready", "active", "resident"}:
    adapter_tier = "persistent"
else:
    adapter_tier = "persistent"

try:
    adapter_rank = int(float(adapter_rank_raw))
except Exception:
    adapter_rank = 0

try:
    adapter_alpha = float(adapter_alpha_raw)
except Exception:
    adapter_alpha = 0.0

if adapter_rank <= 0:
    raise SystemExit(f"invalid adapter rank for runtime manifest: {adapter_rank_raw}")
if adapter_alpha <= 0.0:
    raise SystemExit(f"invalid adapter alpha for runtime manifest: {adapter_alpha_raw}")

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

  manifest_seed="$(python3 - "$worker_manifest" <<'PY'
import hashlib
import pathlib
import sys

payload = pathlib.Path(sys.argv[1]).read_bytes()
print(hashlib.sha256(payload).hexdigest()[:16])
PY
)"
  runtime_plan_id="functional-golden-runtime-${manifest_seed}"

  note "Restarting stack with runtime worker manifest: ${worker_manifest}"
  if ! "${ROOT_DIR}/start" down; then
    die "Failed to stop stack before runtime manifest restart" "Check current process state and var/logs."
  fi

  if ! DEFAULT_MANIFEST_HASH= AOS_MANIFEST_HASH= AOS_MANIFEST_PATH="$worker_manifest" AOS_WORKER_MANIFEST="$worker_manifest" PLAN_ID="$runtime_plan_id" SKIP_SECD=1 SKIP_NODE=1 "${ROOT_DIR}/start" up --quick; then
    die "Failed to restart stack with runtime worker manifest" "Check var/logs/backend.log and var/logs/worker.log."
  fi

  export PLAN_ID="$runtime_plan_id"
  export AOS_WORKER_MANIFEST="$worker_manifest"
  wait_ready
}

worker_uds_request() {
  local method="$1"
  local path="$2"
  local body="${3:-}"
  local socket_path="${AOS_WORKER_SOCKET:-${ROOT_DIR}/var/run/worker.sock}"
  local out_file
  local status

  [ -S "$socket_path" ] || die "Worker socket not available: ${socket_path}" "Ensure worker is running before UDS adapter commands."

  out_file="$(mktemp "${ROOT_DIR}/var/tmp/golden-path.worker-http.XXXXXX")"
  if [ -n "$body" ]; then
    if ! status="$(curl -sS --unix-socket "$socket_path" -o "$out_file" -w '%{http_code}' -X "$method" -H 'Content-Type: application/json' -d "$body" "http://localhost${path}")"; then
      status="000"
    fi
  else
    if ! status="$(curl -sS --unix-socket "$socket_path" -o "$out_file" -w '%{http_code}' -X "$method" "http://localhost${path}")"; then
      status="000"
    fi
  fi

  if [ -n "$HTTP_BODY_FILE" ] && [ -f "$HTTP_BODY_FILE" ]; then
    rm -f "$HTTP_BODY_FILE"
  fi

  HTTP_STATUS="$status"
  HTTP_BODY_FILE="$out_file"
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
)" || die "Worker command response parsing failed for ${action}" "Response: $(cat "$HTTP_BODY_FILE")"

  ok="$(printf '%s\n' "$parsed" | awk -F= '/^success=/{print $2}')"
  message="$(printf '%s\n' "$parsed" | awk -F= '/^message=/{print $2}')"
  [ "$ok" = "true" ] || die "Worker command reported failure for ${action}" "Message: ${message}"
}

prime_worker_active_adapter() {
  local adapter_id="$1"
  local adapter_hash="$2"
  local preload_payload
  local swap_payload

  note "Priming worker adapter stack for inference: ${adapter_id}"

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

require_cmd python3
require_cmd curl
require_cmd awk
require_cmd cargo

model_has_train_pad_token() {
  local model_dir="$1"
  local tokenizer_config="${model_dir}/tokenizer_config.json"
  local tokenizer_json="${model_dir}/tokenizer.json"

  python3 - "$tokenizer_config" "$tokenizer_json" <<'PY'
import json
import os
import sys

cfg_path = sys.argv[1]
tok_path = sys.argv[2]

def truthy_pad_token(value):
    if value is None:
        return False
    if isinstance(value, dict):
        content = value.get("content")
        if isinstance(content, str) and content.strip():
            return True
        token = value.get("id")
        if isinstance(token, str) and token.strip():
            return True
        return False
    if isinstance(value, str):
        return bool(value.strip())
    return False

if os.path.isfile(cfg_path):
    try:
        with open(cfg_path, "r", encoding="utf-8") as fh:
            cfg = json.load(fh)
    except Exception:
        cfg = {}
    pad_id = cfg.get("pad_token_id")
    if pad_id is not None and str(pad_id).strip() != "":
        print("1")
        sys.exit(0)
    if truthy_pad_token(cfg.get("pad_token")):
        print("1")
        sys.exit(0)

if os.path.isfile(tok_path):
    try:
        with open(tok_path, "r", encoding="utf-8") as fh:
            tok = json.load(fh)
    except Exception:
        tok = {}
    for token in tok.get("added_tokens", []):
        if not isinstance(token, dict):
            continue
        content = token.get("content")
        if isinstance(content, str) and "pad" in content.lower():
            print("1")
            sys.exit(0)

print("0")
PY
}

if [ ! -x "$AOSCTL" ]; then
  die "aosctl launcher not found at ${AOSCTL}" "Ensure repo launcher exists and is executable: ./aosctl --rebuild --help"
fi

MODEL_CACHE_ROOT="${AOS_MODEL_CACHE_DIR:-${ROOT_DIR}/var/model-cache/models}"
if [ ! -d "$MODEL_CACHE_ROOT" ]; then
  die "Model cache root not found: ${MODEL_CACHE_ROOT}" "Ensure models exist under var/model-cache/models."
fi

PREFERRED_MANIFESTS="llama3.2-3b-instruct-4bit.yaml mistral7b-instruct-v0.3-mlx-4bit.yaml"
for m in $PREFERRED_MANIFESTS; do
  manifest_path="${ROOT_DIR}/manifests/$m"
  if [ -f "$manifest_path" ]; then
    case "$m" in
      llama3.2-3b*) model_hint="Llama-3.2-3B-Instruct-4bit" ;;
      mistral7b*)   model_hint="mistral-7b-instruct-v0.3-4bit" ;;
      *) model_hint="" ;;
    esac
    if [ -n "$model_hint" ] && [ -d "$MODEL_CACHE_ROOT/$model_hint" ] && [ -f "$MODEL_CACHE_ROOT/$model_hint/config.json" ]; then
      MODEL_DIR="$MODEL_CACHE_ROOT/$model_hint"
      break
    fi
  fi
done

if [ -z "${MODEL_DIR:-}" ]; then
  MODEL_DIR="$(find -L "$MODEL_CACHE_ROOT" -mindepth 1 -maxdepth 1 -type d -print0 2>/dev/null | \
    while IFS= read -r -d '' dir; do
      if [ -f "$dir/config.json" ]; then
        size_kb=$(du -sk "$dir" | awk '{print $1}')
        printf '%s\t%s\n' "$size_kb" "$dir"
      fi
    done | sort -n | head -n 1 | cut -f2-)"
fi

[ -n "$MODEL_DIR" ] || die "No model directory with config.json found under ${MODEL_CACHE_ROOT}" "Populate var/model-cache/models with a model."
MODEL_DIR="$(cd "$MODEL_DIR" && pwd -P)"
MODEL_ID="$(basename "$MODEL_DIR")"
TOKENIZER_PATH="${MODEL_DIR}/tokenizer.json"
[ -f "$TOKENIZER_PATH" ] || die "Tokenizer not found: ${TOKENIZER_PATH}" "Ensure tokenizer.json exists in ${MODEL_DIR}."

export AOS_SERVER_URL
export AOS_TENANT_ID
export AOS_DEV_NO_AUTH=1
export AOS_MODEL_CACHE_DIR="$MODEL_CACHE_ROOT"
export AOS_BASE_MODEL_ID="$MODEL_ID"
export AOS_MODEL_PATH="$MODEL_DIR"
export AOS_TOKENIZER_PATH="$TOKENIZER_PATH"
export AOS_MODEL_BACKEND="${AOS_MODEL_BACKEND:-mlx}"
export DATABASE_URL="${DATABASE_URL:-sqlite://${ROOT_DIR}/var/aos-cp.sqlite3}"
TRAINING_EXECUTION_MODE="${AOS_TRAINING_EXECUTION_MODE:-in_process}"
export AOS_TRAINING_EXECUTION_MODE="$TRAINING_EXECUTION_MODE"

if [ "${AOS_MODEL_BACKEND}" = "mlx" ]; then
  note "Ensuring backend binary is current"
  cargo build -p adapteros-server >/dev/null
  note "Ensuring worker binary supports MLX backend"
  cargo build -p adapteros-lora-worker --features mlx >/dev/null
fi

if [ "$TRAINING_EXECUTION_MODE" = "worker" ]; then
  ensure_training_worker_binary
else
  note "Training execution mode: ${TRAINING_EXECUTION_MODE} (managed training worker not required)"
fi

MANIFEST_YAML=""
case "$MODEL_ID" in
  Llama-3.2-3B-Instruct-4bit)
    manifest_candidate="${ROOT_DIR}/manifests/llama3.2-3b-instruct-4bit.yaml"
    [ -f "$manifest_candidate" ] && MANIFEST_YAML="$manifest_candidate"
    ;;
  mistral-7b-instruct-v0.3-4bit)
    manifest_candidate="${ROOT_DIR}/manifests/mistral7b-instruct-v0.3-mlx-4bit.yaml"
    [ -f "$manifest_candidate" ] && MANIFEST_YAML="$manifest_candidate"
    ;;
esac

if [ -z "$MANIFEST_YAML" ]; then
  fallback_manifests=(
    "${ROOT_DIR}/manifests/llama3.2-3b-instruct-4bit.yaml"
    "${ROOT_DIR}/manifests/mistral7b-instruct-v0.3-mlx-4bit.yaml"
  )
  for manifest_candidate in "${fallback_manifests[@]}"; do
    if [ -f "$manifest_candidate" ]; then
      MANIFEST_YAML="$manifest_candidate"
      break
    fi
  done
fi

if [ -f "$MANIFEST_YAML" ]; then
  export AOS_MANIFEST_PATH="$MANIFEST_YAML"
  export AOS_WORKER_MANIFEST="$MANIFEST_YAML"
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
export PLAN_ID="${PLAN_ID:-functional-golden-${plan_suffix}}"
echo "plan_id=${PLAN_ID}"

AUTH_PATH="${ROOT_DIR}/var/tmp/dev-auth.json"
mkdir -p "${ROOT_DIR}/var/tmp"
cat > "$AUTH_PATH" <<AUTH_JSON
{"base_url":"${AOS_SERVER_URL}","tenant_id":"${AOS_TENANT_ID}","token":"dev-bypass","refresh_token":null,"expires_at":null}
AUTH_JSON
export AOSCTL_AUTH_PATH="$AUTH_PATH"

if [ "$NO_DB_RESET" -eq 0 ]; then
  note "Resetting control-plane stores for clean run"
  rm -f \
    "${ROOT_DIR}/var/aos-cp.sqlite3" \
    "${ROOT_DIR}/var/aos-cp.sqlite3-shm" \
    "${ROOT_DIR}/var/aos-cp.sqlite3-wal" \
    "${ROOT_DIR}/var/aos-kv.redb"
  rm -rf "${ROOT_DIR}/var/aos-kv-index"
fi

note "Starting control plane + worker"
SKIP_SECD=1 SKIP_NODE=1 "${ROOT_DIR}/start" up --quick
STARTED_STACK=1

note "Waiting for /readyz"
wait_ready

note "Seeding model in DB: ${MODEL_ID}"
"$AOSCTL" models seed --model-path "$MODEL_DIR" >/dev/null

DOC_FIXTURE="${ROOT_DIR}/tests/fixtures/docs/adapter_notes.md"
[ -f "$DOC_FIXTURE" ] || die "Document fixture not found: ${DOC_FIXTURE}" "Expected existing fixture under tests/fixtures/docs."

note "Uploading document fixture: ${DOC_FIXTURE}"
DOC_UPLOAD_BODY="$(mktemp "${ROOT_DIR}/var/tmp/golden-doc-upload.XXXXXX.json")"
if ! DOC_UPLOAD_STATUS="$(curl -sS -o "$DOC_UPLOAD_BODY" -w '%{http_code}' "${AUTH_HEADER[@]}" \
  -F "name=functional-proof-doc" \
  -F "file=@${DOC_FIXTURE};type=text/markdown" \
  "${AOS_SERVER_URL}/v1/documents/upload")"; then
  DOC_UPLOAD_STATUS="000"
fi
if [ "$DOC_UPLOAD_STATUS" != "200" ]; then
  die "Document upload failed (HTTP ${DOC_UPLOAD_STATUS})" "Response: $(cat "$DOC_UPLOAD_BODY")"
fi

document_id="$(python3 - "$DOC_UPLOAD_BODY" <<'PY'
import json
import sys
with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)
print(data.get('document_id', ''))
PY
)"
initial_doc_status="$(python3 - "$DOC_UPLOAD_BODY" <<'PY'
import json
import sys
with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)
print(data.get('status', ''))
PY
)"
rm -f "$DOC_UPLOAD_BODY"

[ -n "$document_id" ] || die "Upload response missing document_id" "Check /v1/documents/upload handler output."
echo "document_id=${document_id}"
echo "document_status=${initial_doc_status}"

if [ "$initial_doc_status" != "indexed" ]; then
  note "Triggering explicit document processing for ${document_id}"
  http_request POST "${AOS_SERVER_URL}/v1/documents/${document_id}/process" "{}"
  if [ "$HTTP_STATUS" = "400" ] && grep -qi "processing lock" "$HTTP_BODY_FILE"; then
    note "Document processing already in progress; continuing with status polling"
  elif [ "$HTTP_STATUS" != "200" ] && [ "$HTTP_STATUS" != "202" ]; then
    die "Document process request failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
  fi
fi

note "Polling document status until indexed"
doc_timeout="${DOCUMENT_TIMEOUT_SECS:-300}"
doc_reprocess_interval="${DOCUMENT_REPROCESS_INTERVAL_SECS:-15}"
doc_reprocess_limit="${DOCUMENT_REPROCESS_ATTEMPTS:-8}"
doc_reprocess_attempts=0
doc_last_reprocess=0
doc_started="$(date +%s)"
final_doc_status=""
while true; do
  http_request GET "${AOS_SERVER_URL}/v1/documents/${document_id}"
  if [ "$HTTP_STATUS" != "200" ]; then
    die "Document status query failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
  fi

  final_doc_status="$(python3 - "$HTTP_BODY_FILE" <<'PY'
import json
import sys
with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)
print(data.get('status', ''))
PY
)"

  if [ "$final_doc_status" = "pending" ] || [ "$final_doc_status" = "processing" ]; then
    http_request GET "${AOS_SERVER_URL}/v1/documents/${document_id}/chunks"
    if [ "$HTTP_STATUS" = "200" ]; then
      chunk_count="$(python3 - "$HTTP_BODY_FILE" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)

if isinstance(data, list):
    print(len(data))
else:
    print(0)
PY
)"
      if [[ "$chunk_count" =~ ^[0-9]+$ ]] && [ "$chunk_count" -gt 0 ]; then
        note "Document chunks available (${chunk_count}); treating status=${final_doc_status} as indexed"
        final_doc_status="indexed"
      fi
    fi
  fi

  if [ "$final_doc_status" = "indexed" ]; then
    break
  fi

  if [ "$final_doc_status" = "failed" ]; then
    die "Document processing failed" "Check var/logs/backend.log for document_id=${document_id}."
  fi

  if [ "$final_doc_status" = "pending" ]; then
    now_ts="$(date +%s)"
    if [ "$doc_reprocess_attempts" -lt "$doc_reprocess_limit" ] && [ $(( now_ts - doc_last_reprocess )) -ge "$doc_reprocess_interval" ]; then
      note "Document still pending; re-triggering processing (${doc_reprocess_attempts}/${doc_reprocess_limit})"
      http_request POST "${AOS_SERVER_URL}/v1/documents/${document_id}/process" "{}"
      if [ "$HTTP_STATUS" = "400" ] && grep -qi "processing lock" "$HTTP_BODY_FILE"; then
        note "Document processing lock is active; continuing status polling"
      elif [ "$HTTP_STATUS" != "200" ] && [ "$HTTP_STATUS" != "202" ]; then
        die "Document reprocess request failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
      fi

      doc_reprocess_attempts=$((doc_reprocess_attempts + 1))
      doc_last_reprocess="$now_ts"
    fi
  fi

  if [ $(( $(date +%s) - doc_started )) -gt "$doc_timeout" ]; then
    die "Document indexing timed out after ${doc_timeout}s" "Check var/logs/backend.log for document_id=${document_id}."
  fi

  sleep 2
done

echo "document_status=${final_doc_status}"

note "Creating dataset from indexed document"
dataset_payload="$(python3 - <<'PY'
import json
print(json.dumps({
    "document_id": "__DOC_ID__",
    "name": "functional-proof-dataset",
    "description": "Deterministic dataset from fixture document"
}))
PY
)"
dataset_payload="${dataset_payload/__DOC_ID__/${document_id}}"
http_request POST "${AOS_SERVER_URL}/v1/datasets/from-documents" "$dataset_payload"
if [ "$HTTP_STATUS" != "200" ]; then
  die "Dataset creation from document failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
fi

dataset_id="$(python3 - "$HTTP_BODY_FILE" <<'PY'
import json
import sys
with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)
print(data.get('dataset_id', ''))
PY
)"
dataset_version_id="$(python3 - "$HTTP_BODY_FILE" <<'PY'
import json
import sys
with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)
print(data.get('dataset_version_id', ''))
PY
)"

[ -n "$dataset_id" ] || die "Dataset creation response missing dataset_id" "Check /v1/datasets/from-documents response."
[ -n "$dataset_version_id" ] || die "Dataset creation response missing dataset_version_id" "Check /v1/datasets/from-documents response."
echo "dataset_id=${dataset_id}"
echo "dataset_version_id=${dataset_version_id}"

note "Applying dataset trust override for training eligibility"
dataset_trust_state="$(apply_dataset_trust_override "$dataset_id" "$dataset_version_id")"
echo "dataset_trust_state=${dataset_trust_state}"

note "Starting training job (adapter #1)"
train_one="$(start_training_job "$dataset_version_id" "minimal-golden-path-a")"
repo_id_1="$(printf '%s\n' "$train_one" | awk -F= '/^repo_id=/{print $2}')"
job_id_1="$(printf '%s\n' "$train_one" | awk -F= '/^job_id=/{print $2}')"
echo "repo_id_1=${repo_id_1}"
echo "job_id_1=${job_id_1}"

note "Waiting for training completion (adapter #1)"
status_json_1="$(training_wait "$job_id_1" "${TRAINING_TIMEOUT_SECS:-1800}")"
parsed_1="$(parse_training_completion "$status_json_1")"
adapter_id_1="$(printf '%s\n' "$parsed_1" | awk -F= '/^adapter_id=/{print $2}')"
aos_path_1="$(printf '%s\n' "$parsed_1" | awk -F= '/^aos_path=/{print $2}')"
package_hash_1="$(printf '%s\n' "$parsed_1" | awk -F= '/^package_hash_b3=/{print $2}')"
base_model_id_1="$(printf '%s\n' "$parsed_1" | awk -F= '/^base_model_id=/{print $2}')"

[ -n "$adapter_id_1" ] || die "Training completed but adapter_id missing (adapter #1)" "Check backend logs for job_id=${job_id_1}."
[ -n "$base_model_id_1" ] || die "Training completed but base_model_id missing (adapter #1)" "Check backend logs for job_id=${job_id_1}."

echo "$parsed_1"

note "Verifying adapter #1 discoverability"
http_request GET "${AOS_SERVER_URL}/v1/adapters"
if [ "$HTTP_STATUS" != "200" ]; then
  die "Failed to list adapters (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
fi
adapter_hash_1="$(python3 - "$HTTP_BODY_FILE" "$adapter_id_1" <<'PY'
import json
import sys
with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)
needle = sys.argv[2]
items = data.get('adapters', []) if isinstance(data, dict) else (data if isinstance(data, list) else [])
for item in items:
    if item.get('adapter_id') == needle:
        print(item.get('hash_b3', ''))
        break
PY
)"
[ -n "$adapter_hash_1" ] || die "Adapter #1 not found in /v1/adapters" "Check registration in training logs."
echo "adapter_id_1=${adapter_id_1}"
echo "adapter_hash_b3_1=${adapter_hash_1}"
ensure_adapter_swap_ready "$adapter_id_1"

note "Preparing second adapter for real swap"
aos_path_2=""
package_hash_2=""
adapter_id_2="$(find_snapshot_adapter_excluding "$adapter_id_1")"
if [ -z "$adapter_id_2" ]; then
  note "No existing second adapter with training snapshot found; importing fallback adapter for swap verification"
  if ! adapter_id_2="$(import_fallback_swap_adapter "$adapter_id_1")"; then
    adapter_id_2=""
  fi
  if [ -n "$adapter_id_2" ]; then
    # Duplicate fallback reuses the same package bytes as adapter #1.
    aos_path_2="$aos_path_1"
    echo "adapter_id_2=${adapter_id_2}"
  fi
fi

if [ -z "$adapter_id_2" ]; then
  if [ "$ALLOW_SINGLE_ADAPTER" -eq 1 ]; then
    note "No second adapter available; --allow-single-adapter enabled. Continuing without swap."
    swap_performed=0
    active_adapter_id="$adapter_id_1"
  else
    die "Unable to prepare a second adapter for swap" "Re-run with --allow-single-adapter to bypass swap requirement explicitly."
  fi
else
  ensure_adapter_swap_ready "$adapter_id_2"
  if [ "$adapter_id_2" = "$adapter_id_1" ]; then
    if [ "$ALLOW_SINGLE_ADAPTER" -eq 1 ]; then
      note "Second adapter equals first adapter; --allow-single-adapter enabled. Continuing without swap."
      swap_performed=0
      active_adapter_id="$adapter_id_1"
    else
      die "Prepared adapter for swap is identical to adapter #1" "Need two distinct adapters for swap receipt verification."
    fi
  else
    swap_performed=1
    active_adapter_id="$adapter_id_1"
  fi
fi

if [ "$swap_performed" -eq 1 ]; then
  note "Swapping adapters: ${adapter_id_1} -> ${adapter_id_2}"
  swap_payload="$(python3 - <<'PY'
import json
print(json.dumps({
  "old_adapter_id": "__OLD__",
  "new_adapter_id": "__NEW__",
  "dry_run": False
}))
PY
)"
  swap_payload="${swap_payload/__OLD__/${adapter_id_1}}"
  swap_payload="${swap_payload/__NEW__/${adapter_id_2}}"

  http_request POST "${AOS_SERVER_URL}/v1/adapters/swap" "$swap_payload"
  if [ "$HTTP_STATUS" != "200" ]; then
    die "Adapter swap failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
  fi

  echo "swap_response=$(cat "$HTTP_BODY_FILE")"

  note "Verifying swap receipt via /v1/audit/logs"
  http_request GET "${AOS_SERVER_URL}/v1/audit/logs?action=adapter.swap&limit=20"
  if [ "$HTTP_STATUS" != "200" ]; then
    die "Audit log query failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
  fi

  swap_receipt_summary="$(python3 - "$HTTP_BODY_FILE" "$adapter_id_1" "$adapter_id_2" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)

old_id = sys.argv[2]
new_id = sys.argv[3]
entries = data.get('logs', []) if isinstance(data, dict) else []

for entry in entries:
    action = entry.get('action')
    resource_id = entry.get('resource_id') or ''
    if action != 'adapter.swap':
        continue
    if old_id in resource_id and new_id in resource_id:
        print(f"swap_receipt_id={entry.get('id', '')}")
        print(f"swap_receipt_action={action}")
        print(f"swap_receipt_resource={resource_id}")
        print(f"swap_receipt_status={entry.get('status', '')}")
        metadata = entry.get('metadata_json')
        if metadata is None:
            print('swap_receipt_metadata=')
        else:
            print(f"swap_receipt_metadata={metadata}")
        break
else:
    sys.exit(1)
PY
)" || die "Swap receipt not found in audit logs" "Expected action=adapter.swap with both adapter IDs in resource_id."

  echo "$swap_receipt_summary"

  note "Verifying audit chain integrity"
  http_request GET "${AOS_SERVER_URL}/v1/audit/chain/verify"
  if [ "$HTTP_STATUS" != "200" ]; then
    die "Audit chain verification request failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
  fi
  chain_valid="$(python3 - "$HTTP_BODY_FILE" <<'PY'
import json
import sys
with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)
print(str(bool(data.get('chain_valid'))).lower())
PY
)"
  if [ "$chain_valid" != "true" ]; then
    die "Audit chain verification reported invalid chain" "Response: $(cat "$HTTP_BODY_FILE")"
  fi
  echo "audit_chain_valid=true"
fi

active_adapter_aos_path=""
if [ "$active_adapter_id" = "$adapter_id_1" ]; then
  active_adapter_aos_path="$aos_path_1"
elif [ -n "${adapter_id_2:-}" ] && [ "$active_adapter_id" = "$adapter_id_2" ]; then
  active_adapter_aos_path="$aos_path_2"
fi

active_adapter_fields="$(fetch_adapter_runtime_fields "$active_adapter_id")"
active_adapter_hash="$(printf '%s\n' "$active_adapter_fields" | awk -F= '/^hash_b3=/{print $2}')"
active_adapter_tier="$(printf '%s\n' "$active_adapter_fields" | awk -F= '/^tier=/{print $2}')"
active_adapter_rank="$(printf '%s\n' "$active_adapter_fields" | awk -F= '/^rank=/{print $2}')"
active_adapter_alpha="$(printf '%s\n' "$active_adapter_fields" | awk -F= '/^alpha=/{print $2}')"
active_adapter_lora_strength="$(printf '%s\n' "$active_adapter_fields" | awk -F= '/^lora_strength=/{print $2}')"
active_adapter_aos_path_detail="$(printf '%s\n' "$active_adapter_fields" | awk -F= '/^aos_file_path=/{print $2}')"

[ -n "$active_adapter_hash" ] || die "Active adapter hash missing for ${active_adapter_id}" "Adapter detail must include hash_b3."
[ -n "$active_adapter_rank" ] || die "Active adapter rank missing for ${active_adapter_id}" "Adapter detail must include rank."
[ -n "$active_adapter_alpha" ] || die "Active adapter alpha missing for ${active_adapter_id}" "Adapter detail must include alpha."
if [ -z "$active_adapter_aos_path" ] && [ -n "$active_adapter_aos_path_detail" ]; then
  active_adapter_aos_path="$active_adapter_aos_path_detail"
fi

note "Ensuring worker adapter bundle path for ${active_adapter_id}"
ensure_worker_adapter_bundle "$active_adapter_id" "$active_adapter_hash" "$active_adapter_aos_path" >/dev/null

[ -n "${MANIFEST_YAML:-}" ] || die "Base manifest YAML not resolved" "Set AOS_MANIFEST_PATH to an existing manifest file."
runtime_worker_manifest="${ROOT_DIR}/var/tmp/golden-worker-manifest-${active_adapter_id}.yaml"
build_runtime_worker_manifest \
  "$runtime_worker_manifest" \
  "$active_adapter_id" \
  "$active_adapter_hash" \
  "$active_adapter_tier" \
  "$active_adapter_rank" \
  "$active_adapter_alpha" \
  "$active_adapter_lora_strength"
restart_stack_with_worker_manifest "$runtime_worker_manifest"

note "Priming worker adapter stack for ${active_adapter_id}"
prime_worker_active_adapter "$active_adapter_id" "$active_adapter_hash"

ensure_model_loaded_for_inference "$base_model_id_1"

note "Running deterministic inference through selected adapter: ${active_adapter_id}"
infer_payload="$(python3 - <<'PY'
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
infer_payload="${infer_payload/__ADAPTER__/${active_adapter_id}}"

http_request POST "${AOS_SERVER_URL}/v1/infer" "$infer_payload"
if [ "$HTTP_STATUS" != "200" ]; then
  die "Inference request failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
fi

INFER_RESPONSE_PATH="${ROOT_DIR}/var/tmp/golden_path_infer_response.json"
cp "$HTTP_BODY_FILE" "$INFER_RESPONSE_PATH"

after_infer_summary="$(python3 - "$INFER_RESPONSE_PATH" "$active_adapter_id" <<'PY'
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
    print('empty_response_text', file=sys.stderr)
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
    print('missing_receipt_ids', file=sys.stderr)
    sys.exit(1)
if selected_adapter_id and selected_adapter_id not in adapters_used:
    print('selected_adapter_not_in_receipt', file=sys.stderr)
    sys.exit(1)

print(f"response_text={text}")
print(f"trace_id={trace_id}")
print(f"run_receipt_digest={receipt_digest}")
print(f"deterministic_receipt_backend={det.get('backend_used')}")
print(f"deterministic_receipt_adapters={det.get('adapters_used')}")
print(f"adapters_used={adapters_used}")
PY
)" || die "Inference response missing required receipt fields" "Check var/logs/worker.log and var/logs/backend.log."

echo "$after_infer_summary"

trace_id="$(printf '%s\n' "$after_infer_summary" | awk -F= '/^trace_id=/{print $2}')"
run_receipt_digest="$(printf '%s\n' "$after_infer_summary" | awk -F= '/^run_receipt_digest=/{print $2}')"

note "Fetching run receipt by digest: ${run_receipt_digest}"
http_request GET "${AOS_SERVER_URL}/v1/adapteros/receipts/${run_receipt_digest}"
if [ "$HTTP_STATUS" != "200" ]; then
  die "Run receipt lookup failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
fi

RUN_RECEIPT_PATH="${ROOT_DIR}/var/tmp/golden_path_run_receipt.json"
cp "$HTTP_BODY_FILE" "$RUN_RECEIPT_PATH"

note "Verifying inference receipt with /v1/replay/verify/trace"
trace_verify_payload="$(python3 - "$trace_id" <<'PY'
import json
import sys

trace_id = (sys.argv[1] or "").strip()
if not trace_id:
    print("missing_trace_id", file=sys.stderr)
    sys.exit(1)
print(json.dumps({"trace_id": trace_id}))
PY
)" || die "Failed to build trace verification payload" "trace_id=${trace_id}"

http_request POST "${AOS_SERVER_URL}/v1/replay/verify/trace" "$trace_verify_payload"
if [ "$HTTP_STATUS" != "200" ]; then
  die "Trace receipt verification failed (HTTP ${HTTP_STATUS})" "Response: $(cat "$HTTP_BODY_FILE")"
fi

TRACE_VERIFY_PATH="${ROOT_DIR}/var/tmp/golden_path_trace_verify.json"
cp "$HTTP_BODY_FILE" "$TRACE_VERIFY_PATH"

trace_verify_summary="$(python3 - "$TRACE_VERIFY_PATH" <<'PY'
import json
import sys
with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    data = json.load(fh)
print(f"trace_verify_pass={str(bool(data.get('pass'))).lower()}")
print(f"trace_verify_receipt_match={str(bool((data.get('receipt_digest') or {}).get('matches'))).lower()}")
print(f"trace_verify_run_head_match={str(bool((data.get('run_head_hash') or {}).get('matches'))).lower()}")
print(f"trace_verify_output_match={str(bool((data.get('output_digest') or {}).get('matches'))).lower()}")
PY
)"
echo "$trace_verify_summary"
trace_verify_pass="$(printf '%s\n' "$trace_verify_summary" | awk -F= '/^trace_verify_pass=/{print $2}')"
trace_verify_receipt_match="$(printf '%s\n' "$trace_verify_summary" | awk -F= '/^trace_verify_receipt_match=/{print $2}')"
[ "$trace_verify_pass" = "true" ] || die "Trace receipt verification reported pass=false" "Response: $(cat "$TRACE_VERIFY_PATH")"
[ "$trace_verify_receipt_match" = "true" ] || die "Trace receipt verification reported receipt_digest mismatch" "Response: $(cat "$TRACE_VERIFY_PATH")"

echo "inference_response_path=${INFER_RESPONSE_PATH}"
echo "trace_verify_path=${TRACE_VERIFY_PATH}"
echo "run_receipt_path=${RUN_RECEIPT_PATH}"

backend_log="${ROOT_DIR}/var/logs/backend.log"
if [ -f "$backend_log" ]; then
  note "Training log excerpt"
  if command -v rg >/dev/null 2>&1; then
    rg "${job_id_1}" "$backend_log" | tail -n 5
  else
    grep -n "${job_id_1}" "$backend_log" | tail -n 5
  fi
fi

note "Golden functional path complete"
