#!/usr/bin/env bash
set -euo pipefail

SOCKET="/var/run/aos/aos.sock"

echo "[1/3] Starting worker (background)"
./target/release/aosctl serve --tenant default --plan cp_abc123 --socket "$SOCKET" &
SERVE_PID=$!

cleanup() {
  echo "Cleaning up..."
  kill $SERVE_PID >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "[2/3] Waiting for socket: $SOCKET"
for i in {1..30}; do
  if [ -S "$SOCKET" ]; then
    break
  fi
  sleep 1
done

if [ ! -S "$SOCKET" ]; then
  echo "Socket not available: $SOCKET" >&2
  exit 1
}

echo "[3/3] Running inference via CLI"
./target/release/aosctl infer --prompt "Hello from smoke test" --socket "$SOCKET" --timeout 20000 || {
  echo "Inference failed" >&2
  exit 1
}

echo "Smoke test completed successfully"

