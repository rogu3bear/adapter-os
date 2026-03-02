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

DATASET_VERSION_ID="${1:-${DATASET_VERSION_ID:-}}"
if [ -z "$DATASET_VERSION_ID" ]; then
  die "dataset_version_id is required" "Run scripts/upload_minimal_dataset.sh and pass its dataset_version_id."
fi

MODEL_CACHE_ROOT="${AOS_MODEL_CACHE_DIR:-${ROOT_DIR}/var/model-cache/models}"
if [ ! -d "$MODEL_CACHE_ROOT" ]; then
  die "Model cache root not found: ${MODEL_CACHE_ROOT}" "Ensure model cache exists under var/model-cache/models."
fi

if [ -n "${AOS_BASE_MODEL_ID:-}" ] && [ -d "${MODEL_CACHE_ROOT}/${AOS_BASE_MODEL_ID}" ]; then
  MODEL_ID="$AOS_BASE_MODEL_ID"
  MODEL_DIR="${MODEL_CACHE_ROOT}/${MODEL_ID}"
else
  MODEL_DIR="$(find "$MODEL_CACHE_ROOT" -mindepth 1 -maxdepth 1 -type d -print0 | \
    while IFS= read -r -d '' dir; do
      if [ -f "$dir/config.json" ]; then
        size_kb=$(du -sk "$dir" | awk '{print $1}')
        printf '%s\t%s\n' "$size_kb" "$dir"
      fi
    done | sort -n | head -n 1 | cut -f2-)"
  MODEL_ID="$(basename "$MODEL_DIR")"
fi

if [ -z "$MODEL_DIR" ]; then
  die "No model directory with config.json found under ${MODEL_CACHE_ROOT}" "Populate var/model-cache/models with a model."
fi
MODEL_CONFIG="${MODEL_DIR}/config.json"
if [ ! -f "$MODEL_CONFIG" ]; then
  die "Model config missing: ${MODEL_CONFIG}" "Ensure config.json exists in ${MODEL_DIR}."
fi

export AOS_BASE_MODEL_ID="$MODEL_ID"
export MODEL_ID

AUTH_PATH="${ROOT_DIR}/var/tmp/dev-auth.json"
mkdir -p "$(dirname "$AUTH_PATH")"
BASE_URL="${AOS_SERVER_URL:-http://127.0.0.1:8080}"
TENANT_ID="${AOS_TENANT_ID:-default}"

cat > "$AUTH_PATH" <<EOF
{"base_url":"${BASE_URL}","tenant_id":"${TENANT_ID}","token":"dev-bypass","refresh_token":null,"expires_at":null}
EOF

export AOSCTL_AUTH_PATH="$AUTH_PATH"

if [ "${SKIP_MODEL_SEED:-0}" != "1" ]; then
  "$AOSCTL" models seed --model-path "$MODEL_DIR" >/dev/null
fi

REPO_NAME="${REPO_NAME:-minimal-golden-path}"
repo_list="$(curl -s "${BASE_URL}/v1/adapter-repositories?tenant_id=${TENANT_ID}")"

repo_id="$(printf '%s' "$repo_list" | python3 - <<'PY'
import json
import os
import sys

target = os.environ.get("REPO_NAME", "")
data = json.load(sys.stdin)
for repo in data:
    if repo.get("name") == target:
        print(repo.get("id", ""))
        break
PY
)"

if [ -z "$repo_id" ]; then
  create_body="$(python3 - <<'PY'
import json
import os

body = {
    "tenant_id": os.environ.get("AOS_TENANT_ID", "default"),
    "name": os.environ.get("REPO_NAME", "minimal-golden-path"),
    "base_model_id": os.environ.get("AOS_BASE_MODEL_ID") or os.environ.get("MODEL_ID"),
    "description": "Minimal golden path repository",
    "default_branch": "main",
}
print(json.dumps(body))
PY
)"
  create_resp="$(curl -s -w "\n%{http_code}" -H "Content-Type: application/json" \
    -d "$create_body" "${BASE_URL}/v1/adapter-repositories")"
  create_body_resp="$(printf '%s' "$create_resp" | head -n 1)"
  create_code="$(printf '%s' "$create_resp" | tail -n 1)"
  if [ "$create_code" != "201" ]; then
    die "Failed to create repository (HTTP ${create_code}): ${create_body_resp}" "Check server logs in var/logs/backend.log."
  fi
  repo_id="$(printf '%s' "$create_body_resp" | python3 - <<'PY'
import json
import sys

data = json.load(sys.stdin)
repo_id = data.get("repo_id")
if not repo_id:
    print("", end="")
else:
    print(repo_id)
PY
)"
fi

if [ -z "$repo_id" ]; then
  die "Failed to resolve repo_id" "Verify adapter repository creation via /v1/adapter-repositories."
fi

train_json="$("$AOSCTL" --json train start "$repo_id" \
  --base-model-id "$MODEL_ID" \
  --dataset-version-ids "$DATASET_VERSION_ID" \
  --backend mlx)"

parsed="$(printf '%s' "$train_json" | python3 - <<'PY'
import json
import sys

data = json.load(sys.stdin)
job_id = data.get("id")
if not job_id:
    print("missing job_id", file=sys.stderr)
    sys.exit(1)
print(f"repo_id={data.get('repo_id') or ''}")
print(f"job_id={job_id}")
PY
)" || die "Failed to parse training start response" "Check server logs in var/logs/backend.log."

echo "$parsed"
