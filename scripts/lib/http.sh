#!/usr/bin/env bash
# adapterOS HTTP helpers (curl)
#
# Contract:
# - Never hang: every request has connect + total timeout.
# - Never silently treat non-2xx as success (caller decides best-effort vs fail-fast).
# - Capture response headers/body under ./var/tmp (never /tmp).
#
# Usage:
#   source scripts/lib/http.sh
#   code="$(aos_http_request GET "http://127.0.0.1:8080/healthz")"
#   body_path="$AOS_HTTP_BODY_PATH"
#   headers_path="$AOS_HTTP_HEADERS_PATH"
#
# Env overrides:
#   AOS_HTTP_CONNECT_TIMEOUT_S (default: 2)
#   AOS_HTTP_MAX_TIME_S        (default: 10)
#   AOS_HTTP_TMP_DIR           (default: var/tmp/http)

set -u

aos_http_trim_url() {
  local url="${1:-}"
  # Trim trailing slashes for consistency (keep scheme+host).
  printf "%s" "${url%/}"
}

aos_http__curl_supports_fail_with_body() {
  curl --help all 2>/dev/null | grep -q -- "--fail-with-body"
}

aos_http__snippet() {
  local path="$1"
  local n="${2:-300}"
  if [ ! -f "$path" ]; then
    return 0
  fi
  python3 - "$path" "$n" 2>/dev/null <<'PY' || true
import sys
path = sys.argv[1]
n = int(sys.argv[2])
try:
    with open(path, "rb") as f:
        data = f.read(n)
    # best-effort decode
    s = data.decode("utf-8", errors="replace").replace("\n", " ").replace("\r", " ")
    print(s)
except Exception:
    pass
PY
}

# Perform an HTTP request and capture headers/body under ./var/tmp.
#
# Outputs: HTTP status code to stdout (or "000" if curl failed before getting a response).
# Side effects: sets:
#   AOS_HTTP_BODY_PATH, AOS_HTTP_HEADERS_PATH
#
# Returns:
#   0 if curl itself succeeded (regardless of HTTP status)
#   non-zero if curl failed (DNS/refused/timeout/etc.)
aos_http_request() {
  local method="${1:-}"
  local url_raw="${2:-}"
  local body="${3:-}"

  if [ -z "$method" ] || [ -z "$url_raw" ]; then
    echo "000"
    echo "[http] ERROR: usage: aos_http_request METHOD URL [BODY]" >&2
    return 2
  fi

  local url
  url="$(aos_http_trim_url "$url_raw")"

  local connect_timeout="${AOS_HTTP_CONNECT_TIMEOUT_S:-2}"
  local max_time="${AOS_HTTP_MAX_TIME_S:-10}"
  local tmp_dir="${AOS_HTTP_TMP_DIR:-var/tmp/http}"

  mkdir -p "$tmp_dir"

  local ts pid suffix
  ts="$(date +%s)"
  pid="$$"
  suffix="${ts}.${pid}.$RANDOM"

  AOS_HTTP_BODY_PATH="${tmp_dir}/body.${suffix}.txt"
  AOS_HTTP_HEADERS_PATH="${tmp_dir}/headers.${suffix}.txt"
  export AOS_HTTP_BODY_PATH AOS_HTTP_HEADERS_PATH

  local curl_args=(
    -sS
    --connect-timeout "$connect_timeout"
    --max-time "$max_time"
    -D "$AOS_HTTP_HEADERS_PATH"
    -o "$AOS_HTTP_BODY_PATH"
    -w "%{http_code}"
    -X "$method"
  )
  if aos_http__curl_supports_fail_with_body; then
    curl_args+=(--fail-with-body)
  fi

  # Only set Content-Type when we have a body; avoid surprising GETs.
  if [ -n "$body" ]; then
    curl_args+=(-H "Content-Type: application/json" --data-binary "$body")
  fi

  local code="000"
  if ! code="$(curl "${curl_args[@]}" "$url" 2>/dev/null)"; then
    # curl failed: code is likely empty; keep 000.
    echo "000"
    return 1
  fi

  printf "%s" "$code"
  return 0
}

# Convenience: GET and require JSON (best-effort validation if python3 exists).
# Returns non-zero on non-2xx or invalid JSON.
aos_http_get_json() {
  local url="${1:-}"
  local code
  code="$(aos_http_request GET "$url")" || {
    echo "[http] ERROR: curl failed: GET $url" >&2
    return 1
  }

  case "$code" in
    2??) ;;
    *)
      echo "[http] ERROR: GET $url -> HTTP $code" >&2
      echo "[http] body: $(aos_http__snippet "$AOS_HTTP_BODY_PATH" 300)" >&2
      return 1
      ;;
  esac

  if command -v python3 >/dev/null 2>&1; then
    python3 - "$AOS_HTTP_BODY_PATH" 2>/dev/null <<'PY' || {
import json, sys
with open(sys.argv[1], "rb") as f:
    json.load(f)
PY
      echo "[http] ERROR: response was not valid JSON: GET $url" >&2
      echo "[http] body: $(aos_http__snippet "$AOS_HTTP_BODY_PATH" 300)" >&2
      return 1
    }
  fi

  return 0
}

