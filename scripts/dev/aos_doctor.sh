#!/usr/bin/env bash
set -euo pipefail

# adapterOS "doctor": stop services and clean stale runtime artifacts under var/.
#
# Safe by default:
# - Only touches $AOS_VAR_DIR (default: var)
# - Never writes to /tmp (path_security contract)
# - Does not kill non-adapterOS processes holding ports

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

: "${AOS_VAR_DIR:=var}"
: "${AOS_SERVER_PORT:=8080}"
: "${AOS_UI_PORT:=3200}"

if [[ "$AOS_VAR_DIR" == /tmp* || "$AOS_VAR_DIR" == /private/tmp* || "$AOS_VAR_DIR" == /var/tmp* ]]; then
  echo "ERROR: Refusing to operate on forbidden AOS_VAR_DIR under /tmp: $AOS_VAR_DIR" >&2
  exit 1
fi

echo "[doctor] root=$ROOT_DIR var=$AOS_VAR_DIR"

if [ -x scripts/service-manager.sh ]; then
  echo "[doctor] stopping services (fast)..."
  scripts/service-manager.sh stop all fast >/dev/null 2>&1 || true
else
  echo "[doctor] scripts/service-manager.sh not found; skipping coordinated stop" >&2
fi

mkdir -p "$AOS_VAR_DIR/run" "$AOS_VAR_DIR/tmp" "$AOS_VAR_DIR/quarantine" "$AOS_VAR_DIR/logs" >/dev/null 2>&1 || true

quarantine() {
  local path="$1"
  local reason="$2"
  if [ ! -e "$path" ]; then
    return 0
  fi
  local ts
  ts="$(date +%s)"
  local dest="$AOS_VAR_DIR/quarantine/$(basename "$path").${reason}.${ts}"
  mv "$path" "$dest" 2>/dev/null || rm -f "$path" 2>/dev/null || true
  echo "[doctor] quarantined: $path ($reason) -> $dest"
}

maybe_quarantine_pidfile() {
  local pidfile="$1"
  local label="$2"
  if [ ! -f "$pidfile" ]; then
    return 0
  fi
  local pid=""
  pid="$(cat "$pidfile" 2>/dev/null || true)"
  if [ -z "$pid" ]; then
    quarantine "$pidfile" "empty-pid"
    return 0
  fi
  if kill -0 "$pid" 2>/dev/null; then
    echo "[doctor] $label pidfile exists and process is alive (pid=$pid); leaving pidfile"
    return 0
  fi
  quarantine "$pidfile" "stale-pid"
}

maybe_quarantine_socket() {
  local sock="$1"
  if [ ! -S "$sock" ]; then
    return 0
  fi
  if command -v lsof >/dev/null 2>&1 && lsof "$sock" >/dev/null 2>&1; then
    echo "[doctor] socket is in use; leaving: $sock"
    return 0
  fi
  quarantine "$sock" "stale-socket"
}

echo "[doctor] cleaning stale pidfiles..."
maybe_quarantine_pidfile "$AOS_VAR_DIR/backend.pid" "backend"
maybe_quarantine_pidfile "$AOS_VAR_DIR/worker.pid" "worker"
maybe_quarantine_pidfile "$AOS_VAR_DIR/ui.pid" "ui"
maybe_quarantine_pidfile "$AOS_VAR_DIR/secd.pid" "secd"
maybe_quarantine_pidfile "$AOS_VAR_DIR/node.pid" "node"

echo "[doctor] cleaning stale sockets..."
maybe_quarantine_socket "$AOS_VAR_DIR/run/backend.sock"
maybe_quarantine_socket "$AOS_VAR_DIR/run/worker.sock"
maybe_quarantine_socket "$AOS_VAR_DIR/run/aos-secd.sock"

echo "[doctor] removing test sqlite artifacts under $AOS_VAR_DIR/..."
rm -f "$AOS_VAR_DIR"/*-test.sqlite3* "$AOS_VAR_DIR"/*_test.sqlite3* 2>/dev/null || true

echo "[doctor] cleaning tmp files under $AOS_VAR_DIR/tmp/..."
if command -v find >/dev/null 2>&1; then
  find "$AOS_VAR_DIR/tmp" -mindepth 1 -maxdepth 1 -type f -print -delete 2>/dev/null || true
else
  rm -f "$AOS_VAR_DIR/tmp/"* 2>/dev/null || true
fi

echo "[doctor] port status:"
if command -v lsof >/dev/null 2>&1; then
  for port in "$AOS_SERVER_PORT" "$AOS_UI_PORT"; do
    if lsof -nP -i :"$port" -sTCP:LISTEN >/dev/null 2>&1; then
      echo "  port $port: IN USE"
      lsof -nP -i :"$port" -sTCP:LISTEN | head -5 || true
      echo "  remediation: lsof -nP -i :$port -sTCP:LISTEN"
    else
      echo "  port $port: free"
    fi
  done
else
  echo "  lsof not available; skipping port inspection" >&2
fi

echo "[doctor] done"

