#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${AOS_SERVER_URL:-http://localhost:${AOS_SERVER_PORT:-18080}}"
BASE_URL="${BASE_URL%/}"

CURL_MAX_TIME="${AOS_CURL_MAX_TIME:-60}"
MAX_TOKENS="${AOS_MAX_TOKENS:-16}"

PROMPT_DEFAULT="${AOS_PROMPT_DEFAULT:-contract: default routing}"
PROMPT_SELECTED="${AOS_PROMPT_SELECTED:-contract: explicit selection}"

AUTH_HEADER=""
CREATED_STACK_ID=""

die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

note() {
  printf '%s\n' "$*"
}

json_escape() {
  local s="$1"
  s=${s//\\/\\\\}
  s=${s//\"/\\\"}
  s=${s//$'\n'/\\n}
  s=${s//$'\r'/\\r}
  s=${s//$'\t'/\\t}
  printf '%s' "$s"
}

strip_ws() {
  local s="$1"
  s=${s//$' '/}
  s=${s//$'\n'/}
  s=${s//$'\r'/}
  s=${s//$'\t'/}
  printf '%s' "$s"
}

json_get_string() {
  local json="$1"
  local key="$2"

  local tmp="${json#*\"$key\"}"
  [[ "$tmp" == "$json" ]] && return 1
  tmp="${tmp#*:}"
  tmp="${tmp#*\"}"
  printf '%s' "${tmp%%\"*}"
}

json_get_array_content() {
  local json="$1"
  local key="$2"

  local tmp="${json#*\"$key\"}"
  [[ "$tmp" == "$json" ]] && return 1
  tmp="${tmp#*:}"
  tmp="${tmp#*[[]}"
  printf '%s' "${tmp%%]*}"
}

HTTP_STATUS=""
HTTP_BODY=""

http_json() {
  local method="$1"
  local path="$2"
  local body="${3:-}"

  local url="${BASE_URL}${path}"
  local marker="__HTTP_STATUS__:"
  local resp=""

  if [[ -n "$AUTH_HEADER" ]]; then
    if [[ "$method" == "GET" || "$method" == "DELETE" ]]; then
      if ! resp="$(curl -sS --max-time "$CURL_MAX_TIME" -X "$method" "$url" -H "$AUTH_HEADER" -w $'\n'"$marker"'%{http_code}')"; then
        die "curl failed: $method $url"
      fi
    else
      if ! resp="$(curl -sS --max-time "$CURL_MAX_TIME" -X "$method" "$url" -H "$AUTH_HEADER" -H "Content-Type: application/json" -d "$body" -w $'\n'"$marker"'%{http_code}')"; then
        die "curl failed: $method $url"
      fi
    fi
  else
    if [[ "$method" == "GET" || "$method" == "DELETE" ]]; then
      if ! resp="$(curl -sS --max-time "$CURL_MAX_TIME" -X "$method" "$url" -w $'\n'"$marker"'%{http_code}')"; then
        die "curl failed: $method $url"
      fi
    else
      if ! resp="$(curl -sS --max-time "$CURL_MAX_TIME" -X "$method" "$url" -H "Content-Type: application/json" -d "$body" -w $'\n'"$marker"'%{http_code}')"; then
        die "curl failed: $method $url"
      fi
    fi
  fi

  HTTP_STATUS="${resp##*$marker}"
  HTTP_BODY="${resp%$'\n'"$marker$HTTP_STATUS"}"
}

cleanup() {
  local rc=$?
  if [[ -n "${CREATED_STACK_ID:-}" ]]; then
    local url="${BASE_URL}/v1/adapter-stacks/${CREATED_STACK_ID}"
    if [[ -n "${AUTH_HEADER:-}" ]]; then
      curl -sS --max-time "$CURL_MAX_TIME" -X DELETE "$url" -H "$AUTH_HEADER" >/dev/null 2>&1 || true
    else
      curl -sS --max-time "$CURL_MAX_TIME" -X DELETE "$url" >/dev/null 2>&1 || true
    fi
  fi
  return "$rc"
}
trap cleanup EXIT

if ! command -v curl >/dev/null 2>&1; then
  die "curl is required"
fi

note "adapterOS inference contract: ${BASE_URL}"

if [[ -n "${AOS_TOKEN:-}" ]]; then
  AUTH_HEADER="Authorization: Bearer ${AOS_TOKEN}"
  http_json GET "/v1/auth/me"
  if [[ "$HTTP_STATUS" != "200" ]]; then
    die "AOS_TOKEN rejected (GET /v1/auth/me -> HTTP $HTTP_STATUS): $HTTP_BODY"
  fi
else
  http_json GET "/v1/auth/me"
  if [[ "$HTTP_STATUS" != "200" ]]; then
    if [[ -z "${AOS_EMAIL:-}" || -z "${AOS_PASSWORD:-}" ]]; then
      die "auth required (GET /v1/auth/me -> HTTP $HTTP_STATUS). Set AOS_TOKEN, or run server with AOS_DEV_NO_AUTH=1, or set AOS_EMAIL and AOS_PASSWORD to login."
    fi

    login_body="$(printf '{"email":"%s","password":"%s"}' "$(json_escape "$AOS_EMAIL")" "$(json_escape "$AOS_PASSWORD")")"
    http_json POST "/v1/auth/login" "$login_body"
    if [[ "$HTTP_STATUS" != "200" ]]; then
      die "login failed (HTTP $HTTP_STATUS): $HTTP_BODY"
    fi

    token="$(json_get_string "$HTTP_BODY" "token" || true)"
    [[ -z "${token:-}" ]] && die "login succeeded but token was not found in response: $HTTP_BODY"
    AUTH_HEADER="Authorization: Bearer $token"
  fi
fi

note ""
note "1) Inference without selection (default behavior)"
baseline_body="$(printf '{"prompt":"%s","max_tokens":%s}' "$(json_escape "$PROMPT_DEFAULT")" "$MAX_TOKENS")"
http_json POST "/v1/infer" "$baseline_body"
if [[ "$HTTP_STATUS" != "200" ]]; then
  case "$HTTP_STATUS" in
    501) die "inference failed (HTTP 501): worker not initialized. Start a worker and retry. Body: $HTTP_BODY" ;;
    503) die "inference failed (HTTP 503): no compatible worker available. Start a worker and retry. Body: $HTTP_BODY" ;;
    *) die "inference failed (HTTP $HTTP_STATUS): $HTTP_BODY" ;;
  esac
fi

if ! baseline_adapters_raw="$(json_get_array_content "$HTTP_BODY" "adapters_used")"; then
  die "could not find adapters_used in baseline response: $HTTP_BODY"
fi
baseline_adapters_norm="$(strip_ws "$baseline_adapters_raw")"
note "baseline.adapters_used=[${baseline_adapters_norm}]"

note ""
note "2) Inference with explicit stack_id (empty stack => adapters_used must be empty)"
stack_name="contract-empty-${RANDOM}${RANDOM}-$$"
create_stack_body="$(printf '{"name":"%s","description":"contract-inference.sh temporary stack","adapter_ids":[]}' "$(json_escape "$stack_name")")"
http_json POST "/v1/adapter-stacks" "$create_stack_body"
if [[ "$HTTP_STATUS" != "201" && "$HTTP_STATUS" != "200" ]]; then
  die "failed to create empty adapter stack (HTTP $HTTP_STATUS). Provide admin credentials/permissions or set AOS_TOKEN. Body: $HTTP_BODY"
fi

stack_id="$(json_get_string "$HTTP_BODY" "id" || true)"
[[ -z "${stack_id:-}" ]] && die "create stack response missing id: $HTTP_BODY"
CREATED_STACK_ID="$stack_id"
note "created empty stack_id=${stack_id}"

selected_body="$(printf '{"prompt":"%s","max_tokens":%s,"stack_id":"%s"}' "$(json_escape "$PROMPT_SELECTED")" "$MAX_TOKENS" "$(json_escape "$stack_id")")"
http_json POST "/v1/infer" "$selected_body"
if [[ "$HTTP_STATUS" != "200" ]]; then
  die "inference with stack_id failed (HTTP $HTTP_STATUS): $HTTP_BODY"
fi

if ! selected_adapters_raw="$(json_get_array_content "$HTTP_BODY" "adapters_used")"; then
  die "could not find adapters_used in selected response: $HTTP_BODY"
fi
selected_adapters_norm="$(strip_ws "$selected_adapters_raw")"
note "selected.adapters_used=[${selected_adapters_norm}]"

if [[ -n "$selected_adapters_norm" ]]; then
  die "stack selection appears ignored: expected adapters_used=[] for empty stack_id '${stack_id}', got adapters_used=[${selected_adapters_norm}]"
fi

if [[ "$baseline_adapters_norm" == "$selected_adapters_norm" ]]; then
  die "explicit selection did not change receipt fields: baseline.adapters_used is already empty. Configure a non-empty default stack/manifest, or also test with AOS_ADAPTER_ID set."
fi

note ""
note "3) Non-existent stack_id must be rejected (selection must not be ignored)"
missing_stack_id="contract-missing-${RANDOM}${RANDOM}-$$"
missing_body="$(printf '{"prompt":"%s","max_tokens":%s,"stack_id":"%s"}' "$(json_escape "$PROMPT_SELECTED")" "$MAX_TOKENS" "$(json_escape "$missing_stack_id")")"
http_json POST "/v1/infer" "$missing_body"
if [[ "$HTTP_STATUS" == 2* ]]; then
  die "stack_id selection appears ignored: non-existent stack_id '${missing_stack_id}' returned HTTP ${HTTP_STATUS}. Body: $HTTP_BODY"
fi
note "non-existent stack_id rejected (HTTP ${HTTP_STATUS})"

if [[ -n "${AOS_ADAPTER_ID:-}" ]]; then
  note ""
  note "4) Inference with explicit adapter_id (${AOS_ADAPTER_ID})"
  adapter_body="$(printf '{"prompt":"%s","max_tokens":%s,"adapters":["%s"]}' "$(json_escape "$PROMPT_SELECTED")" "$MAX_TOKENS" "$(json_escape "$AOS_ADAPTER_ID")")"
  http_json POST "/v1/infer" "$adapter_body"
  if [[ "$HTTP_STATUS" != "200" ]]; then
    die "inference with adapters[] failed (HTTP $HTTP_STATUS): $HTTP_BODY"
  fi

  if ! adapter_adapters_raw="$(json_get_array_content "$HTTP_BODY" "adapters_used")"; then
    die "could not find adapters_used in adapter-selected response: $HTTP_BODY"
  fi
  if [[ "$adapter_adapters_raw" != *"\"${AOS_ADAPTER_ID}\""* ]]; then
    die "adapter selection appears ignored: requested adapter_id '${AOS_ADAPTER_ID}' not present in adapters_used. adapters_used=[${adapter_adapters_raw}]"
  fi
  note "adapter_id present in adapters_used"

  note ""
  note "5) Non-existent adapter_id must be rejected (selection must not be ignored)"
  missing_adapter_id="contract-missing-adapter-${RANDOM}${RANDOM}-$$"
  missing_adapter_body="$(printf '{"prompt":"%s","max_tokens":%s,"adapters":["%s"]}' "$(json_escape "$PROMPT_SELECTED")" "$MAX_TOKENS" "$(json_escape "$missing_adapter_id")")"
  http_json POST "/v1/infer" "$missing_adapter_body"
  if [[ "$HTTP_STATUS" == 2* ]]; then
    die "adapter selection appears ignored: non-existent adapter_id '${missing_adapter_id}' returned HTTP ${HTTP_STATUS}. Body: $HTTP_BODY"
  fi
  note "non-existent adapter_id rejected (HTTP ${HTTP_STATUS})"
fi

note ""
note "OK: contract satisfied (adapter/stack selection affects adapters_used and invalid selections are rejected)"
