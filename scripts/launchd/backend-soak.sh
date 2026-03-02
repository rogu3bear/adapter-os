#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

DURATION_SECS="${1:-900}"
INTERVAL_SECS="${2:-15}"
LATENCY_BUDGET_MS="${3:-15000}"
INFER_EVERY_SECS="${AOS_SOAK_INFER_EVERY_SECS:-60}"
MAX_RESTARTS_ALLOWED="${AOS_SOAK_MAX_RESTARTS:-0}"
INFER_TIMEOUT_SECS="${AOS_SOAK_INFER_TIMEOUT_SECS:-0}"
BACKEND_PORT="${AOS_SERVER_PORT:-18080}"
BACKEND_LABEL="${AOS_BACKEND_LAUNCHD_LABEL:-com.adapteros.backend}"
BACKEND_TARGET="gui/$(id -u)/${BACKEND_LABEL}"
SUMMARY_FILE="$PROJECT_ROOT/var/logs/backend-soak-summary.json"

if ! [[ "$DURATION_SECS" =~ ^[0-9]+$ ]] || [ "$DURATION_SECS" -le 0 ]; then
    echo "Invalid duration seconds: $DURATION_SECS" >&2
    exit 2
fi
if ! [[ "$INTERVAL_SECS" =~ ^[0-9]+$ ]] || [ "$INTERVAL_SECS" -le 0 ]; then
    echo "Invalid interval seconds: $INTERVAL_SECS" >&2
    exit 2
fi
if ! [[ "$LATENCY_BUDGET_MS" =~ ^[0-9]+$ ]] || [ "$LATENCY_BUDGET_MS" -le 0 ]; then
    echo "Invalid latency budget ms: $LATENCY_BUDGET_MS" >&2
    exit 2
fi
if ! [[ "$INFER_EVERY_SECS" =~ ^[0-9]+$ ]] || [ "$INFER_EVERY_SECS" -le 0 ]; then
    echo "Invalid inference cadence seconds: $INFER_EVERY_SECS" >&2
    exit 2
fi
if ! [[ "$MAX_RESTARTS_ALLOWED" =~ ^[0-9]+$ ]] || [ "$MAX_RESTARTS_ALLOWED" -lt 0 ]; then
    echo "Invalid max restarts: $MAX_RESTARTS_ALLOWED" >&2
    exit 2
fi
if [ "$INFER_TIMEOUT_SECS" -eq 0 ]; then
    INFER_TIMEOUT_SECS=$(( (LATENCY_BUDGET_MS / 1000) + 15 ))
fi
if ! [[ "$INFER_TIMEOUT_SECS" =~ ^[0-9]+$ ]] || [ "$INFER_TIMEOUT_SECS" -le 0 ]; then
    echo "Invalid inference timeout seconds: $INFER_TIMEOUT_SECS" >&2
    exit 2
fi

mkdir -p "$PROJECT_ROOT/var/logs"

log() {
    printf '[%s] [backend-soak] %s\n' "$(date -Iseconds)" "$*"
}

get_launchd_field() {
    local field="$1"
    launchctl print "$BACKEND_TARGET" 2>/dev/null | awk -F'= ' -v f="$field" '
        $1 ~ ("^[[:space:]]*" f "[[:space:]]*$") {
            gsub(/ /, "", $2);
            print $2;
            exit;
        }
    '
}

current_pid() {
    get_launchd_field "pid"
}

current_runs() {
    get_launchd_field "runs"
}

fail_and_write_summary() {
    local reason="$1"
    local now_ts
    now_ts="$(date +%s)"
    local elapsed=$((now_ts - start_ts))
    {
        echo "{"
        echo "  \"status\": \"failed\","
        echo "  \"reason\": \"${reason}\","
        echo "  \"duration_seconds\": ${elapsed},"
        echo "  \"iterations\": ${iterations},"
        echo "  \"restarts_observed\": ${restarts_observed},"
        echo "  \"health_failures\": ${health_failures},"
        echo "  \"inference_probes\": ${inference_probes},"
        echo "  \"max_inference_latency_ms\": ${max_latency_ms},"
        echo "  \"backend_target\": \"${BACKEND_TARGET}\""
        echo "}"
    } >"$SUMMARY_FILE"
    log "FAILED: ${reason}"
    log "summary: $SUMMARY_FILE"
    exit 1
}

if ! command -v launchctl >/dev/null 2>&1; then
    echo "launchctl is required on macOS" >&2
    exit 1
fi

if ! launchctl print "$BACKEND_TARGET" >/dev/null 2>&1; then
    echo "launchd backend target not found: $BACKEND_TARGET" >&2
    exit 1
fi

start_ts="$(date +%s)"
end_ts=$((start_ts + DURATION_SECS))
iterations=0
restarts_observed=0
health_failures=0
inference_probes=0
max_latency_ms=0

baseline_pid="$(current_pid || true)"
if [ -z "${baseline_pid:-}" ]; then
    fail_and_write_summary "backend_pid_unavailable_at_start"
fi
baseline_runs="$(current_runs || echo 0)"
last_pid="$baseline_pid"
next_infer_at="$start_ts"

log "starting soak: duration=${DURATION_SECS}s interval=${INTERVAL_SECS}s latency_budget_ms=${LATENCY_BUDGET_MS} max_restarts=${MAX_RESTARTS_ALLOWED} infer_timeout_secs=${INFER_TIMEOUT_SECS}"
log "launchd target=${BACKEND_TARGET} baseline_pid=${baseline_pid} baseline_runs=${baseline_runs}"

while :; do
    now_ts="$(date +%s)"
    if [ "$now_ts" -ge "$end_ts" ]; then
        break
    fi
    iterations=$((iterations + 1))

    pid_now="$(current_pid || true)"
    if [ -z "${pid_now:-}" ]; then
        fail_and_write_summary "backend_pid_missing_during_soak"
    fi
    if [ "$pid_now" != "$last_pid" ]; then
        restarts_observed=$((restarts_observed + 1))
        log "backend pid changed: ${last_pid} -> ${pid_now} (restarts_observed=${restarts_observed})"
        last_pid="$pid_now"
        if [ "$restarts_observed" -gt "$MAX_RESTARTS_ALLOWED" ]; then
            fail_and_write_summary "restart_budget_exceeded"
        fi
    fi

    if ! curl -sS --max-time 4 "http://127.0.0.1:${BACKEND_PORT}/healthz" >/dev/null 2>&1; then
        health_failures=$((health_failures + 1))
        fail_and_write_summary "healthz_failed"
    fi
    if ! curl -sS --max-time 4 "http://127.0.0.1:${BACKEND_PORT}/readyz" >/dev/null 2>&1; then
        health_failures=$((health_failures + 1))
        fail_and_write_summary "readyz_failed"
    fi

    if [ "$now_ts" -ge "$next_infer_at" ]; then
        inference_probes=$((inference_probes + 1))
        infer_resp=""
        if ! infer_resp="$(curl -sS --max-time "$INFER_TIMEOUT_SECS" -X POST "http://127.0.0.1:${BACKEND_PORT}/v1/infer" \
            -H 'Content-Type: application/json' \
            -d '{"prompt":"Reply with ok.","max_tokens":4,"temperature":0.0}')"; then
            fail_and_write_summary "infer_probe_http_failure"
        fi
        latency_ms="$(printf '%s\n' "$infer_resp" | sed -n 's/.*"latency_ms":\([0-9][0-9]*\).*/\1/p' | head -1)"
        if [ -z "${latency_ms:-}" ]; then
            fail_and_write_summary "infer_probe_failed_or_missing_latency"
        fi
        if [ "$latency_ms" -gt "$max_latency_ms" ]; then
            max_latency_ms="$latency_ms"
        fi
        if [ "$latency_ms" -gt "$LATENCY_BUDGET_MS" ]; then
            fail_and_write_summary "infer_latency_budget_exceeded_${latency_ms}ms"
        fi
        log "probe ok: latency_ms=${latency_ms}"
        next_infer_at=$((now_ts + INFER_EVERY_SECS))
    fi

    sleep "$INTERVAL_SECS"
done

final_runs="$(current_runs || echo 0)"
total_elapsed=$(( $(date +%s) - start_ts ))
{
    echo "{"
    echo "  \"status\": \"passed\","
    echo "  \"duration_seconds\": ${total_elapsed},"
    echo "  \"iterations\": ${iterations},"
    echo "  \"restarts_observed\": ${restarts_observed},"
    echo "  \"health_failures\": ${health_failures},"
    echo "  \"inference_probes\": ${inference_probes},"
    echo "  \"max_inference_latency_ms\": ${max_latency_ms},"
    echo "  \"launchd_runs_baseline\": ${baseline_runs:-0},"
    echo "  \"launchd_runs_final\": ${final_runs:-0},"
    echo "  \"backend_target\": \"${BACKEND_TARGET}\""
    echo "}"
} >"$SUMMARY_FILE"

log "PASSED: soak complete (${total_elapsed}s)"
log "summary: $SUMMARY_FILE"
