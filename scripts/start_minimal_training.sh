#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$ROOT_DIR/scripts/lib/http.sh"

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
  die "aosctl launcher not found at ${AOSCTL}" "Ensure repo launcher exists and is executable: ./aosctl --rebuild --help"
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
  MODEL_DIR="$(find -L "$MODEL_CACHE_ROOT" -mindepth 1 -maxdepth 1 -type d -print0 2>/dev/null | \
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
BASE_URL="${AOS_SERVER_URL:-http://127.0.0.1:18080}"
TENANT_ID="${AOS_TENANT_ID:-default}"
AUTH_HEADER=("-H" "Authorization: Bearer dev-bypass" "-H" "X-Tenant-ID: ${TENANT_ID}")
HTTP_STATUS=""
HTTP_BODY_FILE=""

mkdir -p "$ROOT_DIR/var/tmp"
export AOS_HTTP_CONNECT_TIMEOUT_S="${AOS_HTTP_CONNECT_TIMEOUT_S:-2}"
export AOS_HTTP_MAX_TIME_S="${AOS_HTTP_MAX_TIME_S:-15}"
export AOS_HTTP_TMP_DIR="${AOS_HTTP_TMP_DIR:-var/tmp/http/start-minimal-training}"

http_request() {
  local method="$1"
  local url="$2"
  local body="${3:-}"
  local code="000"

  if [ -n "$body" ]; then
    if ! aos_http_request "$method" "$url" "$body" "${AUTH_HEADER[@]}" >/dev/null; then
      code="${AOS_HTTP_STATUS:-000}"
    else
      code="${AOS_HTTP_STATUS:-000}"
    fi
  else
    if ! aos_http_request "$method" "$url" "" "${AUTH_HEADER[@]}" >/dev/null; then
      code="${AOS_HTTP_STATUS:-000}"
    else
      code="${AOS_HTTP_STATUS:-000}"
    fi
  fi

  HTTP_STATUS="$code"
  HTTP_BODY_FILE="${AOS_HTTP_BODY_PATH:-}"
  if [ -z "$HTTP_BODY_FILE" ] || [ ! -f "$HTTP_BODY_FILE" ]; then
    die "HTTP response capture missing for ${method} ${url}" "Check scripts/lib/http.sh and ensure var/tmp is writable."
  fi
}

cat > "$AUTH_PATH" <<EOF
{"base_url":"${BASE_URL}","tenant_id":"${TENANT_ID}","token":"dev-bypass","refresh_token":null,"expires_at":null}
EOF

export AOSCTL_AUTH_PATH="$AUTH_PATH"

if [ "${SKIP_MODEL_SEED:-0}" != "1" ]; then
  "$AOSCTL" models seed --model-path "$MODEL_DIR" >/dev/null
fi

export REPO_NAME="${REPO_NAME:-minimal-golden-path}"
http_request GET "${BASE_URL}/v1/adapter-repositories?tenant_id=${TENANT_ID}"
[ "$HTTP_STATUS" = "200" ] || die "Failed to query adapter repositories (HTTP ${HTTP_STATUS})" "Ensure backend is reachable at ${BASE_URL}. Response: $(cat "$HTTP_BODY_FILE")"
repo_list="$(cat "$HTTP_BODY_FILE")"

if ! models_list_json="$("$AOSCTL" models list --json 2>/dev/null)"; then
  die "Failed to list seeded models via aosctl" "Run ./aosctl models list --json manually for details."
fi
resolved_model_id="$(printf '%s' "$models_list_json" | MODEL_ID="$MODEL_ID" MODEL_DIR="$MODEL_DIR" AOS_BASE_MODEL_ID="$AOS_BASE_MODEL_ID" python3 -c '
import json
import os
import sys

data = json.load(sys.stdin)
target_id = os.environ.get("MODEL_ID", "")
target_name = os.environ.get("AOS_BASE_MODEL_ID", "") or target_id
target_dir = os.environ.get("MODEL_DIR", "")
target_dir_name = os.path.basename(os.path.normpath(target_dir)) if target_dir else ""
models = data.get("models", data if isinstance(data, list) else [])
for m in models:
    mid = m.get("id", "")
    mname = m.get("name", "")
    mpath = m.get("model_path", "")
    mpath_name = os.path.basename(os.path.normpath(mpath)) if mpath else ""
    if mid in (target_id, target_name):
        print(mid)
        break
    if mname in (target_id, target_name):
        print(mid or mname)
        break
    if target_dir_name and mpath_name == target_dir_name:
        print(mid or mname)
        break
')"
[ -n "$resolved_model_id" ] || die "Failed to resolve seeded model ID for ${MODEL_ID}" "aosctl models list --json output: ${models_list_json}"

repo_id="$(printf '%s' "$repo_list" | REPO_NAME="$REPO_NAME" python3 -c '
import json
import os
import sys

target = os.environ.get("REPO_NAME", "")
data = json.load(sys.stdin)
repos = data if isinstance(data, list) else data.get("repositories", data.get("repos", []))
for repo in (repos or []):
    if isinstance(repo, dict) and repo.get("name") == target:
        print(repo.get("id", ""))
        break
')"

if [ -z "$repo_id" ]; then
  create_body="$(BASE_MODEL_ID_FOR_REPO="$resolved_model_id" python3 - <<'PY'
import json
import os
bid = os.environ.get("BASE_MODEL_ID_FOR_REPO", "")
body = {
    "tenant_id": os.environ.get("AOS_TENANT_ID", "default"),
    "name": os.environ.get("REPO_NAME", "minimal-golden-path"),
    "description": "Minimal golden path repository",
    "default_branch": "main",
}
if bid:
    body["base_model_id"] = bid
print(json.dumps(body))
PY
)"
  http_request POST "${BASE_URL}/v1/adapter-repositories" "$create_body"
  create_body_resp="$(cat "$HTTP_BODY_FILE")"
  if [ "$HTTP_STATUS" != "201" ]; then
    die "Failed to create repository (HTTP ${HTTP_STATUS}): ${create_body_resp}" "Check server logs in var/logs/backend.log."
  fi
  repo_id="$(printf '%s' "$create_body_resp" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
repo_id = data.get("repo_id")
if not repo_id:
    print("", end="")
else:
    print(repo_id)
')"
fi

if [ -z "$repo_id" ]; then
  die "Failed to resolve repo_id" "Verify adapter repository creation via /v1/adapter-repositories."
fi

if ! train_json="$("$AOSCTL" --json train start "$repo_id" \
  --base-model-id "$resolved_model_id" \
  --dataset-version-ids "$DATASET_VERSION_ID" \
  --backend mlx 2>&1)"; then
  die "aosctl train start failed" "$train_json"
fi

parsed="$(printf '%s' "$train_json" | python3 -c '
import json
import sys

data = json.load(sys.stdin)
job_id = data.get("id")
if not job_id:
    print("missing job_id", file=sys.stderr)
    sys.exit(1)
repo_id = data.get("repo_id") or ""
print("repo_id=" + repo_id)
print("job_id=" + str(job_id))
')" || die "Failed to parse training start response" "Check server logs in var/logs/backend.log."

echo "$parsed"
