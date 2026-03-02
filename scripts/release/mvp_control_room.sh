#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
MVP Production Control Room Runner

Usage:
  scripts/release/mvp_control_room.sh [options]

Options:
  --base-url URL       Health/readiness base URL (default: http://localhost:18080)
  --log-dir DIR        Evidence root dir (default: var/release-control-room)
  --skip-deploy        Skip deploy step (for rehearsal/verification only)
  --dry-run            Print commands without executing
  -h, --help           Show this help

Behavior:
  - Executes control-room fast path in strict order.
  - Stops on first failure.
  - Writes timestamped evidence logs per step plus a summary log.
EOF
}

BASE_URL="${BASE_URL:-http://localhost:18080}"
LOG_ROOT="${LOG_ROOT:-var/release-control-room}"
SKIP_DEPLOY=0
DRY_RUN=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-url)
      [[ $# -ge 2 ]] || { echo "error: --base-url requires a value" >&2; exit 2; }
      BASE_URL="$2"
      shift 2
      ;;
    --log-dir)
      [[ $# -ge 2 ]] || { echo "error: --log-dir requires a value" >&2; exit 2; }
      LOG_ROOT="$2"
      shift 2
      ;;
    --skip-deploy)
      SKIP_DEPLOY=1
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage
      exit 2
      ;;
  esac
done

timestamp_utc() {
  date -u +"%Y-%m-%dT%H:%M:%SZ"
}

slug_ts() {
  date -u +"%Y%m%dT%H%M%SZ"
}

log() {
  printf "[mvp-control-room] %s\n" "$*"
}

fail() {
  printf "[mvp-control-room] ERROR: %s\n" "$*" >&2
  exit 1
}

is_local_base_url() {
  case "$BASE_URL" in
    http://localhost|http://localhost:*|https://localhost|https://localhost:*|http://127.0.0.1|http://127.0.0.1:*|https://127.0.0.1|https://127.0.0.1:*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

run_step() {
  local step="$1"
  local cmd="$2"
  local safe_step
  safe_step="$(echo "$step" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '_')"
  local step_log="${RUN_DIR}/${safe_step}.log"
  local start_ts end_ts
  start_ts="$(timestamp_utc)"

  {
    echo "[$start_ts] STEP: $step"
    echo "COMMAND: $cmd"
  } >> "$EVIDENCE_LOG"

  log "STEP: $step"
  log "CMD:  $cmd"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    {
      echo "[$(timestamp_utc)] RESULT: SKIPPED_DRY_RUN"
      echo
    } >> "$EVIDENCE_LOG"
    return 0
  fi

  if bash -lc "$cmd" >"$step_log" 2>&1; then
    end_ts="$(timestamp_utc)"
    {
      echo "[$end_ts] RESULT: PASS"
      echo "ARTIFACT: $step_log"
      echo
    } >> "$EVIDENCE_LOG"
    log "PASS: $step"
  else
    end_ts="$(timestamp_utc)"
    {
      echo "[$end_ts] RESULT: FAIL"
      echo "ARTIFACT: $step_log"
      echo
    } >> "$EVIDENCE_LOG"
    log "FAIL: $step (see $step_log)"
    fail "stopping at failed step: $step"
  fi
}

RUN_ID="$(slug_ts)"
RUN_DIR="${LOG_ROOT}/${RUN_ID}"
mkdir -p "$RUN_DIR"
EVIDENCE_LOG="${RUN_DIR}/evidence.log"
SUMMARY_LOG="${RUN_DIR}/summary.txt"

{
  echo "MVP Control Room Run"
  echo "run_id: $RUN_ID"
  echo "started_at_utc: $(timestamp_utc)"
  echo "base_url: $BASE_URL"
  echo "skip_deploy: $SKIP_DEPLOY"
  echo "dry_run: $DRY_RUN"
  echo
} > "$EVIDENCE_LOG"

log "run directory: $RUN_DIR"
log "evidence log: $EVIDENCE_LOG"

run_step "Preflight: start" "FG_AUTO_STOP=1 ./start preflight"

if [[ "$SKIP_DEPLOY" -eq 1 ]] && is_local_base_url; then
  run_step "Rehearsal: start local services" "AOS_MODEL_BACKEND=mlx AOS_DEV_NO_AUTH=0 AOS_DEV_SKIP_METALLIB_CHECK=0 AOS_DEV_SKIP_DRIFT_CHECK=0 FG_AUTO_STOP=1 bash scripts/service-manager.sh start backend && AOS_MODEL_BACKEND=mlx AOS_DEV_NO_AUTH=0 AOS_DEV_SKIP_METALLIB_CHECK=0 AOS_DEV_SKIP_DRIFT_CHECK=0 FG_AUTO_STOP=1 bash scripts/service-manager.sh start worker"
fi

run_step "Preflight: aosctl doctor" "AOS_SERVER_URL='${BASE_URL}' ./aosctl doctor"
run_step "Config check" "env AOS_DATABASE_URL=\"\${AOS_DATABASE_URL:-\${DATABASE_URL:-sqlite://var/aos-cp.sqlite3}}\" bash scripts/check-config.sh --allow-in-use"
run_step "Env path policy check" "bash scripts/check_env_paths.sh"
run_step "Migration conflict check" "bash scripts/check_migration_conflicts.sh"
PYTHON_BIN="${AOS_PYTHON_BIN:-python3}"
run_step "Migration signature verify" "\"${PYTHON_BIN}\" scripts/verify_migration_signatures.py"
if [[ "$SKIP_DEPLOY" -eq 1 ]] && is_local_base_url; then
  run_step "Backup verification" "AOS_BACKUP_ROOT=\"\${AOS_BACKUP_ROOT:-$PWD/var/backups/control-room}\" AOS_BACKUP_KEY_PATH=\"\${AOS_BACKUP_KEY_PATH:-$PWD/var/keys/backup.key}\" bash scripts/backup/verify-backups.sh"
else
  run_step "Backup verification" "bash scripts/backup/verify-backups.sh"
fi

if [[ "$SKIP_DEPLOY" -eq 0 ]]; then
  run_step "Deploy production" "bash scripts/deploy-production.sh"
else
  log "Skipping deploy step (--skip-deploy)"
  {
    echo "[$(timestamp_utc)] STEP: Deploy production"
    echo "COMMAND: bash scripts/deploy-production.sh"
    echo "[$(timestamp_utc)] RESULT: SKIPPED_FLAG"
    echo
  } >> "$EVIDENCE_LOG"
fi

run_step "Verify deployment" "bash scripts/verify-deployment.sh"
run_step "Healthz probe" "curl -fsS '${BASE_URL}/healthz'"
run_step "Readyz probe" "curl -fsS '${BASE_URL}/readyz'"
MVP_SMOKE_CMD="bash scripts/mvp_smoke.sh"
if [[ "$SKIP_DEPLOY" -eq 1 ]]; then
  MVP_SMOKE_CMD="MVP_SMOKE_SKIP_FMT=1 MVP_SMOKE_SKIP_SERVER_API_TESTS=1 MVP_SMOKE_SKIP_UI_BUILD=1 AOS_API_URL='${BASE_URL}' bash scripts/mvp_smoke.sh"
fi
run_step "MVP smoke" "$MVP_SMOKE_CMD"
run_step "Inference smoke" "bash scripts/smoke-inference.sh"

{
  echo "MVP control-room run completed successfully."
  echo "run_id: $RUN_ID"
  echo "completed_at_utc: $(timestamp_utc)"
  echo "evidence: $EVIDENCE_LOG"
} > "$SUMMARY_LOG"

log "SUCCESS: all steps completed"
log "summary: $SUMMARY_LOG"
