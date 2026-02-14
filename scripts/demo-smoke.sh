#!/usr/bin/env bash
set -euo pipefail

# Demo smoke test: verifies the UI-critical path actually works:
# - backend responds
# - /readyz is 200 and ready
# - chat completions returns a response
# - worker UDS inference works
#
# Hygiene: uses ./var/tmp only; does not write to /tmp.

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

BASE_URL="${AOS_BASE_URL:-http://localhost:8080}"
SOCKET_PATH="${AOS_WORKER_SOCKET:-$PROJECT_ROOT/var/run/worker.sock}"

TMP_DIR="$PROJECT_ROOT/var/tmp"
mkdir -p "$TMP_DIR"

tmp_readyz="$TMP_DIR/readyz.$$.json"
tmp_chat="$TMP_DIR/chat.$$.json"

cleanup() {
  rm -f "$tmp_readyz" "$tmp_chat" 2>/dev/null || true
}
trap cleanup EXIT

fail() {
  echo "DEMO SMOKE: FAIL: $*" >&2
  exit 1
}

ok() {
  echo "DEMO SMOKE: OK: $*"
}

echo "DEMO SMOKE: base_url=$BASE_URL"
echo "DEMO SMOKE: socket=$SOCKET_PATH"

code="$(curl -sS -o /dev/null -w "%{http_code}" --max-time 3 "$BASE_URL/healthz" || echo 000)"
if [ "$code" != "200" ]; then
  fail "/healthz expected 200, got $code"
fi
ok "/healthz 200"

code="$(curl -sS -o "$tmp_readyz" -w "%{http_code}" --max-time 5 "$BASE_URL/readyz" || echo 000)"
if [ "$code" != "200" ]; then
  fail "/readyz expected 200, got $code (body: $(head -c 200 "$tmp_readyz" 2>/dev/null || true))"
fi

ready="$(python3 - <<'PY' "$tmp_readyz" 2>/dev/null || true
import json,sys
path=sys.argv[1]
try:
  j=json.load(open(path))
  print("true" if j.get("ready") else "false")
except Exception:
  print("false")
PY
)"
if [ "$ready" != "true" ]; then
  fail "/readyz ready!=true (body: $(head -c 200 "$tmp_readyz" 2>/dev/null || true))"
fi
ok "/readyz ready=true"

payload='{"messages":[{"role":"user","content":"Reply with exactly: ok"}],"max_tokens":4}'
code="$(curl -sS -o "$tmp_chat" -w "%{http_code}" --max-time 60 \
  -H 'Content-Type: application/json' \
  -d "$payload" \
  "$BASE_URL/v1/chat/completions" || echo 000)"
if [ "$code" != "200" ]; then
  fail "chat completions expected 200, got $code (body: $(head -c 200 "$tmp_chat" 2>/dev/null || true))"
fi

content="$(python3 - <<'PY' "$tmp_chat" 2>/dev/null || true
import json,sys
j=json.load(open(sys.argv[1]))
choices=j.get("choices") or []
if not choices:
  raise SystemExit(1)
m=choices[0].get("message") or {}
print((m.get("content") or "").strip())
PY
)"
if [ "$content" != "ok" ]; then
  fail "chat content expected 'ok', got: $(printf '%s' "$content" | head -c 120)"
fi
ok "chat completions content=ok"

if [ ! -S "$SOCKET_PATH" ]; then
  fail "worker socket missing: $SOCKET_PATH"
fi

infer_out="$(target/debug/aosctl infer --prompt "Reply with exactly: ok" --socket "$SOCKET_PATH" --max-tokens 4 --timeout 60000 2>/dev/null || true)"
if [ "$(printf '%s' "$infer_out" | tr -d '\r\n' | tr -s ' ')" != "ok" ]; then
  fail "aosctl infer expected 'ok', got: $(printf '%s' "$infer_out" | head -c 120)"
fi
ok "aosctl infer ok"

echo "DEMO SMOKE: PASS"

