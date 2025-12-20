#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_DIR="$ROOT/var/run"

API_PID_FILE="$RUN_DIR/adapteros-api-e2e.pid"
UI_PID_FILE="$RUN_DIR/adapteros-ui-e2e.pid"
WORKER_PID_FILE="$RUN_DIR/adapteros-worker-e2e.pid"

stop_proc() {
  local pid_file=$1
  local name=$2
  if [[ ! -f "$pid_file" ]]; then
    echo "$name not running (no pid file)"
    return 0
  fi

  local pid
  pid="$(cat "$pid_file")"
  if ! kill -0 "$pid" 2>/dev/null; then
    echo "$name pid $pid not active; cleaning up"
    rm -f "$pid_file"
    return 0
  fi

  echo "Stopping $name (pid $pid)..."
  kill "$pid" 2>/dev/null || true
  for _ in {1..30}; do
    if kill -0 "$pid" 2>/dev/null; then
      sleep 1
    else
      break
    fi
  done
  if kill -0 "$pid" 2>/dev/null; then
    echo "$name did not exit gracefully; sending SIGKILL"
    kill -9 "$pid" 2>/dev/null || true
  fi
  rm -f "$pid_file"
  echo "$name stopped."
}

stop_proc "$WORKER_PID_FILE" "Worker"
stop_proc "$UI_PID_FILE" "UI"
stop_proc "$API_PID_FILE" "API"

# Clean up worker socket
rm -f "$RUN_DIR/worker.sock"

echo "Stack is down."
