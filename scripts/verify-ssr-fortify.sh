#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

PW_RUN_ID="${PW_RUN_ID:-ssr-fortify}"
PW_SERVER_PORT="${PW_SERVER_PORT:-8180}"

export PW_RUN_ID
export PW_SERVER_PORT
export PW_DEV_BYPASS=1
export PW_REUSE_EXISTING_SERVER=1
# Prevent stale shared sccache wrappers from stalling long test compiles in churn-heavy worktrees.
export RUSTC_WRAPPER=
export SCCACHE_DISABLE=1

RUN_ROOT="var/playwright/runs/${PW_RUN_ID}"
RUN_DIR="${RUN_ROOT}/run"
DEBUG_DIR="${RUN_ROOT}/debug"
PID_FILE="${RUN_DIR}/aos-server.pid"
SINGLE_WRITER_PID_FILE="${RUN_DIR}/aos-cp-single-writer.pid"

mkdir -p "$RUN_DIR" "$DEBUG_DIR" "var/tmp" "var/playwright/models/mistral-7b-instruct-v0.3-4bit"

log() {
  printf '[verify-ssr-fortify] %s\n' "$*"
}

kill_pid_file() {
  local file="$1"
  if [[ ! -f "$file" ]]; then
    return
  fi

  local pid
  pid="$(cat "$file" 2>/dev/null || true)"
  if [[ "$pid" =~ ^[0-9]+$ ]] && kill -0 "$pid" 2>/dev/null; then
    log "stopping pid ${pid} from ${file}"
    kill "$pid" 2>/dev/null || true
    sleep 1
    kill -9 "$pid" 2>/dev/null || true
  fi
  rm -f "$file"
}

kill_run_scoped_processes() {
  while IFS= read -r line; do
    local pid="${line%% *}"
    local cmd="${line#* }"
    if [[ "$cmd" == *"$RUN_ROOT"* || "$cmd" == *"PW_RUN_ID=${PW_RUN_ID}"* || "$cmd" == *"AOS_SERVER_PORT=${PW_SERVER_PORT}"* ]]; then
      if [[ "$cmd" == *"cargo "* || "$cmd" == *"aos-server"* || "$cmd" == *"playwright"* || "$cmd" == *"node "* || "$cmd" == *"bash -lc"* ]]; then
        log "stopping run-scoped process pid=${pid}"
        kill "$pid" 2>/dev/null || true
      fi
    fi
  done < <(ps -axo pid=,command=)
}

kill_port_listener() {
  if ! command -v lsof >/dev/null 2>&1; then
    return
  fi

  local pids
  pids="$(lsof -ti tcp:"${PW_SERVER_PORT}" -sTCP:LISTEN 2>/dev/null || true)"
  for pid in $pids; do
    local cmd
    cmd="$(ps -p "$pid" -o command= 2>/dev/null || true)"
    if [[ -z "$cmd" ]]; then
      continue
    fi

    if [[ "$cmd" == *"aos-server"* || "$cmd" == *"playwright"* || "$cmd" == *"cargo "* || "$cmd" == *"node "* || "$cmd" == *"bash -lc"* ]]; then
      log "stopping listener on :${PW_SERVER_PORT} (pid=${pid})"
      kill "$pid" 2>/dev/null || true
      sleep 1
      kill -9 "$pid" 2>/dev/null || true
    else
      log "port :${PW_SERVER_PORT} is occupied by non-target process:"
      log "$cmd"
      log "set PW_SERVER_PORT to a free port and retry"
      exit 1
    fi
  done
}

wait_for_ok() {
  local url="$1"
  local timeout_secs="${2:-180}"
  local elapsed=0

  while (( elapsed < timeout_secs )); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done

  log "timeout waiting for ${url}"
  return 1
}

wait_for_html() {
  local url="$1"
  local timeout_secs="${2:-180}"
  local elapsed=0

  while (( elapsed < timeout_secs )); do
    local headers
    headers="$(curl -fsSI "$url" 2>/dev/null || true)"
    if [[ "$headers" == *"text/html"* ]]; then
      return 0
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done

  log "timeout waiting for HTML at ${url}"
  return 1
}

cleanup() {
  set +e
  kill_pid_file "$PID_FILE"
  kill_pid_file "$SINGLE_WRITER_PID_FILE"
  kill_port_listener
}
trap cleanup EXIT INT TERM

log "PW_RUN_ID=${PW_RUN_ID}"
log "PW_SERVER_PORT=${PW_SERVER_PORT}"

kill_pid_file "$PID_FILE"
kill_pid_file "$SINGLE_WRITER_PID_FILE"
kill_run_scoped_processes
kill_port_listener

rm -f "${RUN_ROOT}/aos-cp.sqlite3" "${RUN_ROOT}/aos-kv.redb"
rm -rf "${RUN_ROOT}/aos-kv-index"

log "building adapteros-server once"
cargo build -p adapteros-server

log "compile surface checks"
cargo check -p adapteros-ui --features ssr
cargo check -p adapteros-ui --target wasm32-unknown-unknown --features hydrate
cargo check -p adapteros-server

log "server unit coverage (assets)"
cargo test -p adapteros-server assets::tests:: --bin aos-server -- --nocapture

log "hydration contract"
cargo test --test hydration_gating_test --features extended-tests

log "starting aos-server (manual, isolated state)"
TMPDIR="${ROOT_DIR}/var/tmp" \
AOS_SERVER_PORT="${PW_SERVER_PORT}" \
AOS_MANIFEST_PATH="${ROOT_DIR}/manifests/mistral7b-4bit-mlx.yaml" \
E2E_MODE=1 \
AOS_DEV_NO_AUTH=1 \
AOS_STORAGE_MODE=sql_only \
AOS_DATABASE_URL="sqlite://${RUN_ROOT}/aos-cp.sqlite3" \
AOS_KV_PATH="${RUN_ROOT}/aos-kv.redb" \
AOS_KV_TANTIVY_PATH="${RUN_ROOT}/aos-kv-index" \
AOS_MODEL_CACHE_DIR="var/playwright/models" \
AOS_BASE_MODEL_ID="mistral-7b-instruct-v0.3-4bit" \
AOS_DEV_JWT_SECRET="dev-secret" \
AOS_SKIP_PREFLIGHT=1 \
AOS_MIGRATION_TIMEOUT_SECS=600 \
AOS_RATE_LIMITS_REQUESTS_PER_MINUTE=10000 \
AOS_RATE_LIMITS_BURST_SIZE=2000 \
AOS_RATE_LIMITS_INFERENCE_PER_MINUTE=10000 \
target/debug/aos-server --config configs/cp.toml --pid-file "${SINGLE_WRITER_PID_FILE}" \
  > "${DEBUG_DIR}/aos-server.log" 2>&1 &
SERVER_PID=$!
echo "$SERVER_PID" > "$PID_FILE"

wait_for_ok "http://localhost:${PW_SERVER_PORT}/healthz" 180
wait_for_ok "http://localhost:${PW_SERVER_PORT}/healthz/db" 180
wait_for_html "http://localhost:${PW_SERVER_PORT}/" 180

log "playwright smoke (chromium+webkit)"
npx playwright test tests/playwright/ui/routes.core.smoke.spec.ts \
  --config tests/playwright/playwright.ui.config.ts \
  --project=chromium --project=webkit --grep @smoke

log "playwright no-js SSR (chromium+webkit)"
npx playwright test tests/playwright/ui/routes.core.nojs.ssr.spec.ts \
  --config tests/playwright/playwright.ui.config.ts \
  --project=chromium --project=webkit

log "SSR fortification verification complete"
