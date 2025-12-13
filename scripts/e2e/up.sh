#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_DIR="$ROOT/var/run"
LOG_DIR="$ROOT/var/log"
mkdir -p "$RUN_DIR" "$LOG_DIR"

# Detect frontend package manager (prefer pnpm).
if [[ -f "$ROOT/ui/pnpm-lock.yaml" ]]; then
  PM_CMD=(pnpm --dir "$ROOT/ui")
elif [[ -f "$ROOT/ui/yarn.lock" ]]; then
  PM_CMD=(yarn --cwd "$ROOT/ui")
elif [[ -f "$ROOT/ui/package-lock.json" ]]; then
  PM_CMD=(npm --prefix "$ROOT/ui")
else
  PM_CMD=(pnpm --dir "$ROOT/ui")
fi

UI_PORT="${UI_PORT:-3200}"
API_PORT="${API_PORT:-8080}"
DB_PATH="${DB_PATH:-$ROOT/var/aos-cp.sqlite3}"
BASE_URL_DEFAULT="http://127.0.0.1:${UI_PORT}"
API_URL_DEFAULT="http://127.0.0.1:${API_PORT}"

export CYPRESS_baseUrl="${CYPRESS_baseUrl:-$BASE_URL_DEFAULT}"
export CYPRESS_API_URL="${CYPRESS_API_URL:-$API_URL_DEFAULT}"
export CYPRESS_E2E_USER="${CYPRESS_E2E_USER:-dev@local}"
export CYPRESS_E2E_PASS="${CYPRESS_E2E_PASS:-dev123}"

API_HEALTH="${API_HEALTH:-${CYPRESS_API_URL}/api/readyz}"
UI_HEALTH="${UI_HEALTH:-${CYPRESS_baseUrl}}"
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-180}"
HEALTH_INTERVAL="${HEALTH_INTERVAL:-5}"

API_PID_FILE="$RUN_DIR/adapteros-api-e2e.pid"
UI_PID_FILE="$RUN_DIR/adapteros-ui-e2e.pid"
API_LOG="$LOG_DIR/adapteros-api-e2e.log"
UI_LOG="$LOG_DIR/adapteros-ui-e2e.log"

fail_if_running() {
  local pid_file=$1
  local name=$2
  if [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then
    echo "$name already running (pid $(cat "$pid_file")). Run scripts/e2e/down.sh first." >&2
    exit 1
  fi
}

fail_if_running "$API_PID_FILE" "API"
fail_if_running "$UI_PID_FILE" "UI"

wait_for_url() {
  local url="$1"
  local label="$2"
  local end=$((SECONDS + HEALTH_TIMEOUT))
  while (( SECONDS < end )); do
    if curl -fsS --max-time 5 "$url" >/dev/null; then
      echo "✓ $label ready at $url"
      return 0
    fi
    sleep "$HEALTH_INTERVAL"
  done
  echo "✗ $label not ready before timeout (${HEALTH_TIMEOUT}s): $url" >&2
  return 1
}

wait_for_db() {
  local end=$((SECONDS + HEALTH_TIMEOUT))
  while (( SECONDS < end )); do
    if [[ -s "$DB_PATH" ]]; then
      echo "✓ DB file present at $DB_PATH"
      return 0
    fi
    sleep "$HEALTH_INTERVAL"
  done
  echo "✗ DB file not present before timeout at $DB_PATH" >&2
  return 1
}

echo "Ensuring database schema..."
DATABASE_URL="sqlite://${DB_PATH}" \
  AOS_SKIP_MIGRATION_SIGNATURES="${AOS_SKIP_MIGRATION_SIGNATURES:-1}" \
  cargo sqlx migrate run

echo "Starting API on port ${API_PORT}..."
(
  cd "$ROOT"
  AOS_DEV_NO_AUTH="${AOS_DEV_NO_AUTH:-1}" \
    AOS_SERVER_PORT="${AOS_SERVER_PORT:-$API_PORT}" \
    AOS_SERVER__PORT="${AOS_SERVER__PORT:-$API_PORT}" \
    AOS_DATABASE_URL="sqlite://${DB_PATH}" \
    DATABASE_URL="sqlite://${DB_PATH}" \
    SQLX_DISABLE_STATEMENT_CHECKS="${SQLX_DISABLE_STATEMENT_CHECKS:-1}" \
    cargo run -p adapteros-server -- --config configs/cp.toml >"$API_LOG" 2>&1 &
  echo $! >"$API_PID_FILE"
)

echo "Starting UI preview on port ${UI_PORT}..."
if [[ ! -d "$ROOT/ui/dist" ]]; then
  "${PM_CMD[@]}" install --frozen-lockfile >/dev/null 2>&1 || "${PM_CMD[@]}" install >/dev/null 2>&1
  "${PM_CMD[@]}" build >/dev/null 2>&1
fi
"${PM_CMD[@]}" preview --host 0.0.0.0 --port "$UI_PORT" --strictPort >"$UI_LOG" 2>&1 &
echo $! >"$UI_PID_FILE"

wait_for_db
wait_for_url "$API_HEALTH" "API"
wait_for_url "$UI_HEALTH" "UI"

echo "Stack is up."
