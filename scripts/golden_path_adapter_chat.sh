#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

die() {
  local msg="$1"
  local hint="${2:-}"
  echo "ERROR: ${msg}" >&2
  if [ -n "$hint" ]; then
    echo "HINT: ${hint}" >&2
  fi
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "Missing required command: $1" "Install or add $1 to PATH."
}

require_cmd python3
require_cmd curl

AOSCTL="${ROOT_DIR}/aosctl"
if [ ! -x "$AOSCTL" ]; then
  die "aosctl not found at ${AOSCTL}" "Build: cargo build -p adapteros-cli && ln -sf target/debug/aosctl ./aosctl"
fi

MODEL_CACHE_ROOT="${AOS_MODEL_CACHE_DIR:-${ROOT_DIR}/var/model-cache/models}"
if [ ! -d "$MODEL_CACHE_ROOT" ]; then
  die "Model cache root not found: ${MODEL_CACHE_ROOT}" "Ensure models exist under var/model-cache/models."
fi

MODEL_DIR="$(find "$MODEL_CACHE_ROOT" -mindepth 1 -maxdepth 1 -type d -print0 | \
  while IFS= read -r -d '' dir; do
    if [ -f "$dir/config.json" ]; then
      size_kb=$(du -sk "$dir" | awk '{print $1}')
      printf '%s\t%s\n' "$size_kb" "$dir"
    fi
  done | sort -n | head -n 1 | cut -f2-)"

if [ -z "$MODEL_DIR" ]; then
  die "No model directory with config.json found under ${MODEL_CACHE_ROOT}" "Populate var/model-cache/models with a model."
fi

MODEL_ID="$(basename "$MODEL_DIR")"
MODEL_CONFIG="${MODEL_DIR}/config.json"
TOKENIZER_PATH="${MODEL_DIR}/tokenizer.json"
if [ ! -f "$TOKENIZER_PATH" ]; then
  die "Tokenizer not found: ${TOKENIZER_PATH}" "Ensure tokenizer.json exists in ${MODEL_DIR}."
fi

export AOS_SERVER_URL="${AOS_SERVER_URL:-http://127.0.0.1:8080}"
export AOS_TENANT_ID="${AOS_TENANT_ID:-default}"
export AOS_DEV_NO_AUTH=1
export AOS_MODEL_CACHE_DIR="$MODEL_CACHE_ROOT"
export AOS_BASE_MODEL_ID="$MODEL_ID"
export AOS_MODEL_PATH="$MODEL_DIR"
export AOS_TOKENIZER_PATH="$TOKENIZER_PATH"
export AOS_MANIFEST_PATH="$MODEL_CONFIG"
export AOS_WORKER_MANIFEST="$MODEL_CONFIG"
export AOS_MODEL_BACKEND="${AOS_MODEL_BACKEND:-mlx}"
export DATABASE_URL="${DATABASE_URL:-sqlite://${ROOT_DIR}/var/aos-cp.sqlite3}"

AUTH_PATH="${ROOT_DIR}/var/tmp/dev-auth.json"
mkdir -p "$(dirname "$AUTH_PATH")"
cat > "$AUTH_PATH" <<EOF
{"base_url":"${AOS_SERVER_URL}","tenant_id":"${AOS_TENANT_ID}","token":"dev-bypass","refresh_token":null,"expires_at":null}
EOF
export AOSCTL_AUTH_PATH="$AUTH_PATH"

echo "==> Starting control plane + worker"
if ! "${ROOT_DIR}/start" up --quick; then
  die "Failed to start services" "Check var/logs/start.log and var/logs/backend.log."
fi

echo "==> Waiting for /readyz"
ready_ok=0
for _ in $(seq 1 60); do
  if curl -fsS "${AOS_SERVER_URL}/readyz" >/dev/null; then
    ready_ok=1
    break
  fi
  sleep 2
done
if [ "$ready_ok" -ne 1 ]; then
  die "Control plane not ready" "Check var/logs/backend.log for readiness errors."
fi

echo "==> Seeding model in DB: ${MODEL_ID}"
"$AOSCTL" models seed --model-path "$MODEL_DIR" >/dev/null

DATASET_PATH="${ROOT_DIR}/var/datasets/minimal.jsonl"
echo "==> Generating minimal dataset: ${DATASET_PATH}"
"${ROOT_DIR}/scripts/make_minimal_dataset.py" --output "$DATASET_PATH" --count 32 --seed 1337 >/dev/null

echo "==> Uploading dataset"
upload_out="$("${ROOT_DIR}/scripts/upload_minimal_dataset.sh" "$DATASET_PATH")"
dataset_id="$(printf '%s\n' "$upload_out" | awk -F= '/^dataset_id=/{print $2}')"
dataset_version_id="$(printf '%s\n' "$upload_out" | awk -F= '/^dataset_version_id=/{print $2}')"
if [ -z "$dataset_id" ] || [ -z "$dataset_version_id" ]; then
  die "Dataset upload did not return ids" "Check var/logs/backend.log for dataset upload errors."
fi
echo "dataset_id=${dataset_id}"
echo "dataset_version_id=${dataset_version_id}"

echo "==> Starting training job"
train_out="$("${ROOT_DIR}/scripts/start_minimal_training.sh" "$dataset_version_id")"
repo_id="$(printf '%s\n' "$train_out" | awk -F= '/^repo_id=/{print $2}')"
job_id="$(printf '%s\n' "$train_out" | awk -F= '/^job_id=/{print $2}')"
if [ -z "$job_id" ]; then
  die "Training start did not return job_id" "Check var/logs/backend.log for training start errors."
fi
echo "repo_id=${repo_id}"
echo "job_id=${job_id}"

echo "==> Waiting for training completion"
training_timeout="${TRAINING_TIMEOUT_SECS:-1800}"
start_ts=$(date +%s)
adapter_id=""
aos_path=""
package_hash_b3=""
weights_hash_b3=""
status=""
while true; do
  status_json="$("$AOSCTL" --json train status "$job_id")"
  status="$(printf '%s' "$status_json" | python3 - <<'PY'
import json
import sys
data = json.load(sys.stdin)
print(data.get("status", ""))
PY
)"

  if [ "$status" = "completed" ]; then
    parsed="$(printf '%s' "$status_json" | python3 - <<'PY'
import json
import sys
data = json.load(sys.stdin)
print(f"adapter_id={data.get('adapter_id') or ''}")
print(f"aos_path={data.get('aos_path') or ''}")
print(f"package_hash_b3={data.get('package_hash_b3') or data.get('artifact_hash_b3') or ''}")
print(f"weights_hash_b3={data.get('weights_hash_b3') or ''}")
print(f"base_model_id={data.get('base_model_id') or ''}")
print(f"determinism_mode={data.get('determinism_mode') or ''}")
PY
)"
    adapter_id="$(printf '%s\n' "$parsed" | awk -F= '/^adapter_id=/{print $2}')"
    aos_path="$(printf '%s\n' "$parsed" | awk -F= '/^aos_path=/{print $2}')"
    package_hash_b3="$(printf '%s\n' "$parsed" | awk -F= '/^package_hash_b3=/{print $2}')"
    weights_hash_b3="$(printf '%s\n' "$parsed" | awk -F= '/^weights_hash_b3=/{print $2}')"
    base_model_id="$(printf '%s\n' "$parsed" | awk -F= '/^base_model_id=/{print $2}')"
    determinism_mode="$(printf '%s\n' "$parsed" | awk -F= '/^determinism_mode=/{print $2}')"
    break
  fi

  if [ "$status" = "failed" ]; then
    err_msg="$(printf '%s' "$status_json" | python3 - <<'PY'
import json
import sys
data = json.load(sys.stdin)
print(data.get("error_message") or data.get("error_code") or "training failed")
PY
)"
    die "Training failed: ${err_msg}" "Check var/logs/backend.log for job_id=${job_id}."
  fi

  now_ts=$(date +%s)
  elapsed=$((now_ts - start_ts))
  if [ "$elapsed" -gt "$training_timeout" ]; then
    die "Training timed out after ${training_timeout}s" "Check var/logs/backend.log for job_id=${job_id}."
  fi
  sleep 5
done

if [ -z "$adapter_id" ]; then
  die "Training completed but adapter_id missing" "Check var/logs/backend.log for packaging errors."
fi

echo "adapter_id=${adapter_id}"
echo "aos_path=${aos_path}"
echo "package_hash_b3=${package_hash_b3}"
echo "weights_hash_b3=${weights_hash_b3}"
echo "base_model_id=${base_model_id}"
echo "determinism_mode=${determinism_mode}"

echo "==> Verifying adapter discoverability"
adapters_json="$(curl -s "${AOS_SERVER_URL}/v1/adapters")"
adapter_match="$(printf '%s' "$adapters_json" | python3 - <<'PY'
import json
import sys
data = json.load(sys.stdin)
target = sys.argv[1]
for item in data.get("adapters", data):
    if item.get("adapter_id") == target:
        print(item.get("hash_b3", ""))
        break
PY
"$adapter_id"
)"
if [ -z "$adapter_match" ]; then
  die "Adapter not found in /v1/adapters list" "Check adapter registration in training logs."
fi
echo "adapter_hash_b3=${adapter_match}"

echo "==> Running inference with adapter"
export ADAPTER_ID="$adapter_id"
infer_payload="$(python3 - <<'PY'
import json
import os
payload = {
    "prompt": "Summarize the minimal adapter training path in one sentence.",
    "adapters": [os.environ.get("ADAPTER_ID")],
    "max_tokens": 64,
    "backend": "mlx",
}
print(json.dumps(payload))
PY
)"
infer_resp_path="${ROOT_DIR}/var/tmp/minimal_infer_response.json"
mkdir -p "$(dirname "$infer_resp_path")"
curl -s -H "Content-Type: application/json" -d "$infer_payload" \
  "${AOS_SERVER_URL}/v1/infer" > "$infer_resp_path"

infer_summary="$(python3 - <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    data = json.load(handle)

text = data.get("text") or data.get("response") or ""
det = data.get("deterministic_receipt") or {}
run = data.get("run_receipt") or {}
trace = data.get("trace") or {}
chain = trace.get("router_decision_chain") or []

if not det or not run:
    print("missing_receipt_fields", file=sys.stderr)
    sys.exit(1)

summary = {
    "response_text": text.strip(),
    "determinism_mode_applied": data.get("determinism_mode_applied"),
    "replay_guarantee": data.get("replay_guarantee"),
    "receipt_adapter_ids": det.get("adapters_used"),
    "receipt_backend_used": det.get("backend_used"),
    "run_receipt_digest": run.get("receipt_digest"),
    "router_chain_len": len(chain),
    "router_chain_sample": chain[0].get("adapter_ids") if chain else None,
}

for key, value in summary.items():
    print(f"{key}={value}")
PY
"$infer_resp_path"
)" || die "Inference response missing receipts" "Check var/logs/worker.log and var/logs/backend.log."

echo "$infer_summary"

echo "==> Training log excerpt"
backend_log="${ROOT_DIR}/var/logs/backend.log"
if [ -f "$backend_log" ]; then
  if command -v rg >/dev/null 2>&1; then
    rg "$job_id" "$backend_log" | tail -n 5 || true
  else
    grep -n "$job_id" "$backend_log" | tail -n 5 || true
  fi
else
  echo "No backend log found at ${backend_log}"
fi

echo "==> Golden path complete"
