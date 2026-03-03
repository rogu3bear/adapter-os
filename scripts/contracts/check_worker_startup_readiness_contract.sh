#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

SM_FILE="$ROOT_DIR/scripts/service-manager.sh"

fail() {
  echo "FAIL: $1"
  exit 1
}

require_match() {
  local pattern="$1"
  local file="$2"
  local msg="$3"
  rg -q -- "$pattern" "$file" || fail "$msg ($file)"
}

require_no_match() {
  local pattern="$1"
  local file="$2"
  local msg="$3"
  if rg -q -- "$pattern" "$file"; then
    fail "$msg ($file)"
  fi
}

[[ -f "$SM_FILE" ]] || fail "Missing required script: $SM_FILE"

worker_ready_block="$(awk '/worker_ready_status_by_pid\(\)/,/^}/' "$SM_FILE")"
[[ -n "$worker_ready_block" ]] || fail "Could not locate worker_ready_status_by_pid() block"

if ! echo "$worker_ready_block" | rg -q 'ready\|running\|active\|healthy\)'; then
  fail "worker_ready_status_by_pid must include post-registration ready states"
fi

if echo "$worker_ready_block" | rg -q 'registered\|ready\|running\|active\|healthy\)'; then
  fail "worker_ready_status_by_pid must not treat registered as ready"
fi

start_worker_block="$(awk '/start_worker\(\)/,/^}/' "$SM_FILE")"
[[ -n "$start_worker_block" ]] || fail "Could not locate start_worker() block"

if echo "$start_worker_block" | rg -q 'Worker started \(PID: \$pid, Control Plane:'; then
  fail "start_worker must not report success from control-plane status before socket readiness"
fi

if echo "$start_worker_block" | rg -q 'Worker started at timeout boundary \(PID: \$pid, Control Plane:'; then
  fail "timeout-boundary success must not be control-plane-only"
fi

if ! echo "$start_worker_block" | rg -q '\[ -S "\$uds_path" \]'; then
  fail "start_worker must gate readiness on worker socket existence"
fi

if ! echo "$start_worker_block" | rg -q 'Worker started \(PID: \$pid, Socket: \$uds_path\)'; then
  fail "start_worker must emit socket-based success message"
fi

if ! echo "$start_worker_block" | rg -q 'socket never created after \$\{timeout\}s'; then
  fail "start_worker must fail when process runs without socket readiness"
fi

require_match 'registered \(transitional\), socket not ready' "$SM_FILE" \
  "status output must explicitly distinguish transitional registered state from ready"
require_no_match '\[RUNNING\].*socket not ready' "$SM_FILE" \
  "status must not label socket-missing worker as RUNNING"

echo "=== Worker Startup Readiness Contract Check: PASSED ==="
