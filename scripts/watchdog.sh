#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
  cat <<'EOF'
Usage:
  scripts/watchdog.sh <timeout_seconds>

Environment:
  AOS_SERVER_PORT              Backend port (default: 18080)
  AOS_UI_PORT                  UI port (default: 18081)
  AOS_VAR_DIR                  Runtime directory (default: <repo>/var)
  AOS_WORKER_SOCKET            Worker UDS path (default: <var>/run/worker.sock)
  AOS_WATCHDOG_HOST            Host for curl checks (default: 127.0.0.1)
  AOS_WATCHDOG_INTERVAL        Poll interval seconds (default: 1)
  AOS_WATCHDOG_REQUIRE_UI      If "1", require UI to be up for readiness (default: 0)
  AOS_WATCHDOG_REQUIRE_WORKER  If "1", require worker to be up for readiness (default: 0)
EOF
}

die() {
  echo "watchdog: $*" >&2
  exit 2
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

is_int() {
  [[ "${1:-}" =~ ^[0-9]+$ ]]
}

read_pid_file() {
  local pid_file="$1"
  local pid=""
  [ -f "$pid_file" ] || return 1
  pid="$(cat "$pid_file" 2>/dev/null || true)"
  [[ "$pid" =~ ^[0-9]+$ ]] || return 1
  if kill -0 "$pid" 2>/dev/null; then
    echo "$pid"
    return 0
  fi
  return 1
}

pid_listening_on_port() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null | head -1 || true
    return 0
  fi
  return 0
}

pid_holding_socket() {
  local sock_path="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -t "$sock_path" 2>/dev/null | head -1 || true
    return 0
  fi
  return 0
}

curl_status_code() {
  local url="$1"
  local code="000"
  code="$(curl -sS -o /dev/null -w "%{http_code}" --connect-timeout 1 --max-time 2 "$url" 2>/dev/null || true)"
  if [[ "$code" =~ ^[0-9]{3}$ ]]; then
    echo "$code"
  else
    echo "000"
  fi
}

http_is_okish() {
  local code="${1:-000}"
  [[ "$code" =~ ^[0-9]{3}$ ]] || return 1
  [ "$code" != "000" ] || return 1
  local n=$((10#$code))
  [ "$n" -ge 200 ] && [ "$n" -lt 400 ]
}

resolve_backend_log() {
  local log_dir="$1"
  local repo_root="$2"

  local candidates=(
    "$log_dir/backend.log"
    "$repo_root/server-dev.log"
    "$repo_root/cp.log"
  )
  local f
  for f in "${candidates[@]}"; do
    if [ -f "$f" ]; then
      echo "$f"
      return 0
    fi
  done

  local latest=""
  latest="$(ls -t "$log_dir"/aos-cp.* 2>/dev/null | head -1 || true)"
  if [ -n "$latest" ] && [ -f "$latest" ]; then
    echo "$latest"
    return 0
  fi

  echo "$log_dir/backend.log"
}

tail_logs() {
  local service="$1"
  local log_path="$2"
  echo ""
  echo "===== ${service} logs (last 50): ${log_path} ====="
  if [ -f "$log_path" ]; then
    tail -n 50 "$log_path" || true
  else
    echo "(missing)"
  fi
}

main() {
  if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
    usage
    exit 0
  fi

  local timeout_seconds="${1:-${AOS_WATCHDOG_TIMEOUT:-60}}"
  local interval_seconds="${AOS_WATCHDOG_INTERVAL:-1}"

  is_int "$timeout_seconds" || die "timeout_seconds must be an integer"
  is_int "$interval_seconds" || die "AOS_WATCHDOG_INTERVAL must be an integer"
  [ "$interval_seconds" -gt 0 ] || die "AOS_WATCHDOG_INTERVAL must be > 0"

  require_cmd curl
  require_cmd tail

  local host="${AOS_WATCHDOG_HOST:-127.0.0.1}"
  local backend_port="${AOS_SERVER_PORT:-18080}"
  local ui_port="${AOS_UI_PORT:-18081}"
  local var_dir="${AOS_VAR_DIR:-$REPO_ROOT/var}"
  if [[ "$var_dir" != /* ]]; then
    var_dir="$REPO_ROOT/$var_dir"
  fi
  local log_dir="$var_dir/logs"
  local worker_sock="${AOS_WORKER_SOCKET:-$var_dir/run/worker.sock}"
  if [[ "$worker_sock" != /* ]]; then
    worker_sock="$REPO_ROOT/$worker_sock"
  fi

  local backend_pid_file="$var_dir/backend.pid"
  local ui_pid_file="$var_dir/ui.pid"
  local worker_pid_file="$var_dir/worker.pid"

  local backend_log
  backend_log="$(resolve_backend_log "$log_dir" "$REPO_ROOT")"
  local ui_log="$log_dir/ui.log"
  local worker_log="$log_dir/worker.log"

  local require_ui="${AOS_WATCHDOG_REQUIRE_UI:-0}"
  local require_worker="${AOS_WATCHDOG_REQUIRE_WORKER:-0}"

  local start_ts
  start_ts="$(date +%s)"
  local deadline_ts=$((start_ts + timeout_seconds))

  local last_health="000"
  local last_ready="000"
  local last_ui_code="000"

  echo "watchdog: waiting up to ${timeout_seconds}s (interval ${interval_seconds}s)"
  echo "watchdog: backend=http://${host}:${backend_port} ui=http://${host}:${ui_port} var_dir=${var_dir}"

  while true; do
    local now_ts elapsed remaining
    now_ts="$(date +%s)"
    elapsed=$((now_ts - start_ts))
    remaining=$((deadline_ts - now_ts))
    if [ "$remaining" -lt 0 ]; then
      remaining=0
    fi

    local backend_pid ui_pid worker_pid
    backend_pid="$(read_pid_file "$backend_pid_file" || true)"
    if [ -z "$backend_pid" ]; then
      backend_pid="$(pid_listening_on_port "$backend_port")"
    fi

    ui_pid="$(read_pid_file "$ui_pid_file" || true)"
    if [ -z "$ui_pid" ]; then
      ui_pid="$(pid_listening_on_port "$ui_port")"
    fi

    worker_pid=""
    if [ -S "$worker_sock" ]; then
      worker_pid="$(pid_holding_socket "$worker_sock")"
      if [ -n "$worker_pid" ] && ! kill -0 "$worker_pid" 2>/dev/null; then
        worker_pid=""
      fi
    fi
    if [ -z "$worker_pid" ]; then
      worker_pid="$(read_pid_file "$worker_pid_file" || true)"
    fi


    local api_base="http://${host}:${backend_port}/api"
    last_health="$(curl_status_code "${api_base}/healthz")"
    last_ready="$(curl_status_code "${api_base}/readyz")"
    last_ui_code="000"
    if [ "$require_ui" = "1" ]; then
      last_ui_code="$(curl_status_code "http://${host}:${ui_port}/")"
    fi

    local backend_up="down"
    [ -n "$backend_pid" ] && backend_up="up"
    local ui_up="down"
    [ -n "$ui_pid" ] && ui_up="up"
    local worker_up="down"
    if [ -n "$worker_pid" ]; then
      worker_up="up"
    elif [ -S "$worker_sock" ]; then
      worker_up="stale"
    fi

    local curl_suffix="healthz=${last_health} readyz=${last_ready}"
    if [ "$require_ui" = "1" ]; then
      curl_suffix="${curl_suffix} ui=${last_ui_code}"
    fi
    echo "[${elapsed}s/${timeout_seconds}s] services: backend=${backend_up} ui=${ui_up} worker=${worker_up} | curl: ${curl_suffix}"

    local ready=1
    if [ "$last_ready" != "200" ]; then
      ready=0
    fi
    if [ "$last_health" != "200" ]; then
      ready=0
    fi
    if [ "$require_ui" = "1" ]; then
      if ! http_is_okish "$last_ui_code"; then
        ready=0
      fi
    fi
    if [ "$require_worker" = "1" ]; then
      if [ "$worker_up" != "up" ]; then
        ready=0
      fi
    fi

    if [ "$ready" -eq 1 ]; then
      echo "watchdog: READY (elapsed ${elapsed}s) healthz=${last_health} readyz=${last_ready}"
      echo "watchdog: services up: backend=${backend_up} ui=${ui_up} worker=${worker_up}"
      exit 0
    fi

    if [ "$now_ts" -ge "$deadline_ts" ]; then
      echo "watchdog: TIMEOUT (elapsed ${elapsed}s) healthz=${last_health} readyz=${last_ready}"
      echo "watchdog: services up: backend=${backend_up} ui=${ui_up} worker=${worker_up}"

      tail_logs "backend" "$backend_log"
      tail_logs "ui" "$ui_log"
      tail_logs "worker" "$worker_log"

      exit 1
    fi

    local sleep_seconds="$interval_seconds"
    now_ts="$(date +%s)"
    local until_deadline=$((deadline_ts - now_ts))
    if [ "$until_deadline" -le 0 ]; then
      continue
    fi
    if [ "$until_deadline" -lt "$sleep_seconds" ]; then
      sleep_seconds="$until_deadline"
    fi
    sleep "$sleep_seconds"
  done
}

main "$@"
