#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$ROOT_DIR/var/tmp/startup-negative"
mkdir -p "$TMP_DIR"
TEST_DB_URL="sqlite://var/startup-negative.sqlite3"

fail() {
  echo "FAIL: $1"
  exit 1
}

LISTENER_PID=""
cleanup() {
  if [[ -n "$LISTENER_PID" ]]; then
    kill "$LISTENER_PID" >/dev/null 2>&1 || true
    wait "$LISTENER_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

# Negative case 1: malformed server port is rejected by config preflight.
set +e
AOS_DATABASE_URL="$TEST_DB_URL" AOS_SERVER_PORT="not-a-port" bash scripts/check-config.sh --no-dotenv >/dev/null 2>&1
invalid_port_rc=$?
set -e
if [[ "$invalid_port_rc" -eq 0 ]]; then
  fail "check-config unexpectedly accepted malformed AOS_SERVER_PORT"
fi

# Negative case 2: occupied port is rejected by startup preflight + check-config.
TEST_PORT="${STARTUP_NEGATIVE_PORT:-18190}"
python3 -m http.server "$TEST_PORT" --bind 127.0.0.1 --directory "$TMP_DIR" >/dev/null 2>&1 &
LISTENER_PID="$!"
sleep 1

if ! kill -0 "$LISTENER_PID" >/dev/null 2>&1; then
  fail "unable to reserve test port $TEST_PORT for startup negative-path check"
fi

set +e
AOS_SERVER_PORT="$TEST_PORT" ./start preflight >/dev/null 2>&1
preflight_rc=$?
set -e
if [[ "$preflight_rc" -eq 0 ]]; then
  fail "./start preflight unexpectedly succeeded with occupied server port $TEST_PORT"
fi

set +e
AOS_DATABASE_URL="$TEST_DB_URL" AOS_SERVER_PORT="$TEST_PORT" bash scripts/check-config.sh --no-dotenv >/dev/null 2>&1
check_config_rc=$?
set -e
if [[ "$check_config_rc" -eq 0 ]]; then
  fail "check-config unexpectedly succeeded with occupied server port $TEST_PORT"
fi

set +e
AOS_DATABASE_URL="$TEST_DB_URL" AOS_SERVER_PORT="$TEST_PORT" bash scripts/check-config.sh --no-dotenv --allow-in-use >/dev/null 2>&1
allow_in_use_rc=$?
set -e
if [[ "$allow_in_use_rc" -ne 0 ]]; then
  fail "check-config --allow-in-use should succeed with occupied server port"
fi

echo "=== Startup Negative-Path Contract Check: PASSED ==="
