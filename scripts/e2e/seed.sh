#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
API_URL="${CYPRESS_API_URL:-http://127.0.0.1:${AOS_SERVER_PORT:-8080}}"
E2E_USER="${CYPRESS_E2E_USER:-dev@local}"
E2E_PASS="${CYPRESS_E2E_PASS:-dev123}"
AUTH_TOKEN_FILE="$ROOT/var/run/adapteros-e2e-token"
TMP_ROOT="${AOS_VAR_DIR:-$ROOT/var}/tmp"

if [[ "$TMP_ROOT" == /tmp* || "$TMP_ROOT" == /private/tmp* ]]; then
  echo "error: refusing temporary directory under /tmp: $TMP_ROOT" >&2
  exit 1
fi

mkdir -p "$TMP_ROOT"

payload=$(cat <<EOF
{ "email": "${E2E_USER}", "password": "${E2E_PASS}" }
EOF
)

mkdir -p "$(dirname "$AUTH_TOKEN_FILE")"

echo "Seeding E2E user via ${API_URL}/v1/dev/bootstrap..."
response_file="$(mktemp "${TMP_ROOT}/adapteros-e2e-seed.XXXXXX")"
status_code=$(curl -sS -o "$response_file" -w "%{http_code}" \
  -X POST "${API_URL}/v1/dev/bootstrap" \
  -H "Content-Type: application/json" \
  -d "$payload" || true)

if [[ "$status_code" == "200" || "$status_code" == "201" ]]; then
  token=$(python3 - <<'PY'
import json,sys
with open(sys.argv[1], 'r') as f:
    data = json.load(f)
print(data.get("token",""))
PY
  "$response_file")
  if [[ -n "$token" ]]; then
    echo "$token" > "$AUTH_TOKEN_FILE"
    export CYPRESS_AUTH_TOKEN="${CYPRESS_AUTH_TOKEN:-$token}"
    echo "✓ E2E user seeded; token written to $AUTH_TOKEN_FILE"
  else
    echo "⚠️  Bootstrap succeeded but token missing in response"
  fi
elif [[ "$status_code" == "409" || "$status_code" == "400" ]]; then
  echo "User already exists or bootstrap skipped (status ${status_code}); continuing"
else
  echo "✗ Failed to seed E2E user (status ${status_code})"
  cat "$response_file" >&2 || true
  rm -f "$response_file"
  exit 1
fi

rm -f "$response_file"
echo "Seed complete."
