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

AOSCTL="${ROOT_DIR}/aosctl"
if [ ! -x "$AOSCTL" ]; then
  die "aosctl not found at ${AOSCTL}" "Build: cargo build -p adapteros-cli && ln -sf target/debug/aosctl ./aosctl"
fi

DATASET_PATH="${1:-${ROOT_DIR}/var/datasets/minimal.jsonl}"
DATASET_NAME="${DATASET_NAME:-minimal-golden-path}"
DATASET_DESC="${DATASET_DESC:-Deterministic minimal dataset for golden path}"

if [ ! -f "$DATASET_PATH" ]; then
  "${ROOT_DIR}/scripts/make_minimal_dataset.py" --output "$DATASET_PATH"
fi

AUTH_PATH="${ROOT_DIR}/var/tmp/dev-auth.json"
mkdir -p "$(dirname "$AUTH_PATH")"
BASE_URL="${AOS_SERVER_URL:-http://127.0.0.1:8080}"
TENANT_ID="${AOS_TENANT_ID:-default}"

cat > "$AUTH_PATH" <<EOF
{"base_url":"${BASE_URL}","tenant_id":"${TENANT_ID}","token":"dev-bypass","refresh_token":null,"expires_at":null}
EOF

export AOSCTL_AUTH_PATH="$AUTH_PATH"

upload_json="$("$AOSCTL" --json dataset ingest "$DATASET_PATH" --format jsonl --name "$DATASET_NAME" --description "$DATASET_DESC")"

parsed="$(printf '%s' "$upload_json" | python3 - <<'PY'
import json
import sys

data = json.load(sys.stdin)
dataset_id = data.get("dataset_id")
version_id = data.get("dataset_version_id")
if not dataset_id or not version_id:
    print("missing dataset_id or dataset_version_id", file=sys.stderr)
    sys.exit(1)
print(f"dataset_id={dataset_id}")
print(f"dataset_version_id={version_id}")
PY
)" || die "Failed to parse dataset upload response" "Check server logs in var/logs/server.log."

echo "$parsed"
