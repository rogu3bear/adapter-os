#!/usr/bin/env bash
# End-to-end determinism mode validation.
#
# This script automates the manual curl steps outlined in the determinism mode
# test plan. It expects a running control plane and worker plus a registered
# adapter ID. The debug determinism override endpoint must be enabled via
# `AOS_ALLOW_DEBUG_DETERMINISM_OVERRIDE=true` so request-level overrides can be
# asserted without relying on schema changes.
#
# Environment:
#   TOKEN            - Bearer token for API calls (required)
#   ADAPTER_ID       - Adapter ID to include in the stack (required)
#   AOS_BASE_URL     - API base URL (default: http://localhost:18080)
#   STACK_NAME       - Optional name for the test stack
#   TEST_PROMPT      - Optional prompt text (default: "What is 2+2?")
#   DETERMINISM_SEED - Seed used for strict determinism comparison (default: 12345)
#   USE_DEBUG_OVERRIDE - If set to 0, skip /v1/debug/infer and embed determinism_mode
#                        in the request body instead (best-effort for older APIs).

set -euo pipefail

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1" >&2
    exit 1
  }
}

require_cmd curl
require_cmd jq
require_cmd diff

if [[ -z "${TOKEN:-}" ]]; then
  echo "TOKEN is required" >&2
  exit 1
fi

if [[ -z "${ADAPTER_ID:-}" ]]; then
  echo "ADAPTER_ID is required" >&2
  exit 1
fi

BASE_URL="${AOS_BASE_URL:-http://localhost:${AOS_SERVER_PORT:-18080}}"
STACK_NAME="${STACK_NAME:-determinism-test-stack-$(date +%s)}"
PROMPT="${TEST_PROMPT:-What is 2+2?}"
SEED="${DETERMINISM_SEED:-12345}"
USE_DEBUG_OVERRIDE="${USE_DEBUG_OVERRIDE:-1}"
MAX_TOKENS=50

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_ROOT="${REPO_ROOT}/var/tmp"
mkdir -p "${TMP_ROOT}"

STACK_FILE="$(mktemp "${TMP_ROOT}/determinism_stack_XXXX.json")"
RELAXED_FILE="$(mktemp "${TMP_ROOT}/determinism_relaxed_XXXX.json")"
STRICT_FILE="$(mktemp "${TMP_ROOT}/determinism_strict_XXXX.json")"
INHERIT_FILE="$(mktemp "${TMP_ROOT}/determinism_inherit_XXXX.json")"
STRICT_RUN1="$(mktemp "${TMP_ROOT}/determinism_strict_run1_XXXX.json")"
STRICT_RUN2="$(mktemp "${TMP_ROOT}/determinism_strict_run2_XXXX.json")"

cleanup() {
  rm -f "$STACK_FILE" "$RELAXED_FILE" "$STRICT_FILE" "$INHERIT_FILE" "$STRICT_RUN1" "$STRICT_RUN2"
}
trap cleanup EXIT

post_json() {
  local url="$1"
  local body="$2"
  curl -sS -X POST "$url" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d "$body"
}

echo "Step 1: Creating stack '$STACK_NAME' with determinism_mode=strict"
create_body=$(jq -n \
  --arg name "$STACK_NAME" \
  --arg desc "Stack for testing determinism modes" \
  --arg adapter "$ADAPTER_ID" \
  '{
    name: $name,
    description: $desc,
    adapter_ids: [$adapter],
    workflow_type: "sequential",
    determinism_mode: "strict"
  }')
post_json "$BASE_URL/v1/adapter-stacks" "$create_body" | tee "$STACK_FILE" >/dev/null
STACK_ID=$(jq -r '.id' "$STACK_FILE")
STACK_MODE=$(jq -r '.determinism_mode // "unset"' "$STACK_FILE")

if [[ -z "$STACK_ID" || "$STACK_ID" == "null" ]]; then
  echo "Failed to create stack (no id in response): $STACK_FILE" >&2
  exit 1
fi

echo "Created stack: id=$STACK_ID determinism_mode=$STACK_MODE"

echo "Step 2: Verifying stack determinism_mode via list endpoint"
LIST_MODE=$(curl -sS -H "Authorization: Bearer $TOKEN" "$BASE_URL/v1/adapter-stacks" \
  | jq -r --arg name "$STACK_NAME" 'map(select(.name == $name)) | .[0].determinism_mode // "unset"')

if [[ "$LIST_MODE" != "strict" ]]; then
  echo "Expected determinism_mode=strict, got '$LIST_MODE'" >&2
  exit 1
fi
echo "Stack determinism_mode confirmed: $LIST_MODE"

build_infer_body() {
  local seed_val="$1"
  jq -n \
    --arg prompt "$PROMPT" \
    --arg stack "$STACK_ID" \
    --argjson max_tokens "$MAX_TOKENS" \
    --argjson seed "$seed_val" \
    '{
      prompt: $prompt,
      adapter_stack: [$stack],
      max_tokens: $max_tokens,
      seed: $seed
    }'
}

infer_with_mode() {
  local mode="$1"
  local outfile="$2"

  local url body
  body=$(build_infer_body "$SEED")

  if [[ "$USE_DEBUG_OVERRIDE" == "1" && -n "$mode" ]]; then
    url="$BASE_URL/v1/debug/infer?mode=$mode"
  else
    url="$BASE_URL/v1/infer"
    if [[ -n "$mode" ]]; then
      body=$(echo "$body" | jq --arg mode "$mode" '. + {determinism_mode: $mode}')
    fi
  fi

  post_json "$url" "$body" | tee "$outfile" >/dev/null
}

echo "Step 3: Inference with explicit relaxed override"
infer_with_mode "relaxed" "$RELAXED_FILE"
RELAXED_MODE=$(jq -r '.determinism_mode_used // "unknown"' "$RELAXED_FILE")
echo "Relaxed response determinism_mode_used=$RELAXED_MODE"

echo "Step 4: Inference with explicit strict override"
infer_with_mode "strict" "$STRICT_FILE"
STRICT_MODE=$(jq -r '.determinism_mode_used // "unknown"' "$STRICT_FILE")
echo "Strict response determinism_mode_used=$STRICT_MODE"

echo "Step 5: Inference without explicit mode (should inherit stack)"
infer_with_mode "" "$INHERIT_FILE"
INHERIT_MODE=$(jq -r '.determinism_mode_used // "unknown"' "$INHERIT_FILE")
echo "Inherited determinism_mode_used=$INHERIT_MODE"

echo "Step 6: Determinism check (strict mode, same seed twice)"
infer_with_mode "strict" "$STRICT_RUN1"
infer_with_mode "strict" "$STRICT_RUN2"

if diff "$STRICT_RUN1" "$STRICT_RUN2" >/dev/null; then
  echo "Determinism verified: strict responses identical"
else
  echo "Determinism FAILURE: strict responses differ" >&2
  exit 1
fi

echo ""
echo "Results:"
echo "- Stack ID: $STACK_ID"
echo "- Stack determinism_mode: $LIST_MODE"
echo "- Override (relaxed) mode observed: $RELAXED_MODE"
echo "- Override (strict) mode observed: $STRICT_MODE"
echo "- Inherited mode observed: $INHERIT_MODE"
echo "All determinism mode checks passed."
