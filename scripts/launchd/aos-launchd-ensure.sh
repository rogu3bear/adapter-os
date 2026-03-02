#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LOG_FILE="$PROJECT_ROOT/var/logs/launchd-guardian.log"
TS="$(date -Iseconds)"
LOCK_DIR="$PROJECT_ROOT/var/run/launchd-guardian.lock"
SHARED_LOCK_DIR="$PROJECT_ROOT/var/run/service-control.lock"
SHARED_LOCK_ACQUIRED=0
BACKEND_PORT="${AOS_SERVER_PORT:-18080}"
BACKEND_PID_FILE="$PROJECT_ROOT/var/backend.pid"
BACKEND_LAUNCHD_LABEL="${AOS_BACKEND_LAUNCHD_LABEL:-com.adapteros.backend}"
BACKEND_LAUNCHD_DOMAIN="gui/$(id -u)"
BACKEND_LAUNCHD_TARGET="${BACKEND_LAUNCHD_DOMAIN}/${BACKEND_LAUNCHD_LABEL}"
BACKEND_METRICS_FILE="$PROJECT_ROOT/var/run/backend-supervision.state"
WORKER_SOCK="$PROJECT_ROOT/var/run/worker.sock"
WORKER_PID_FILE="$PROJECT_ROOT/var/worker.pid"

mkdir -p "$PROJECT_ROOT/var/logs"
mkdir -p "$PROJECT_ROOT/var/run"

record_backend_restart_event() {
    local cause="$1"
    local ts
    ts="$(date -Iseconds)"
    local count=0
    if [ -f "$BACKEND_METRICS_FILE" ]; then
        local existing
        existing="$(awk -F= '/^restart_count=/{print $2}' "$BACKEND_METRICS_FILE" 2>/dev/null | tail -1)"
        if [[ "$existing" =~ ^[0-9]+$ ]]; then
            count="$existing"
        fi
    fi
    count=$((count + 1))

    local tmp_file="${BACKEND_METRICS_FILE}.tmp"
    {
        echo "restart_count=$count"
        echo "last_restart_cause=$cause"
        echo "last_restart_ts=$ts"
    } >"$tmp_file"
    mv "$tmp_file" "$BACKEND_METRICS_FILE"
}

# Prevent overlapping ticks (launchd StartInterval can fire while startup is in-flight).
if ! mkdir "$LOCK_DIR" 2>/dev/null; then
    {
        echo "[$TS] guardian tick skipped (lock held)"
    } >>"$LOG_FILE" 2>&1
    exit 0
fi
trap 'rmdir "$LOCK_DIR" >/dev/null 2>&1 || true; if [ "$SHARED_LOCK_ACQUIRED" -eq 1 ]; then rmdir "$SHARED_LOCK_DIR" >/dev/null 2>&1 || true; fi' EXIT

{
    echo "[$TS] guardian tick start"

    if ! mkdir "$SHARED_LOCK_DIR" 2>/dev/null; then
        echo "[$TS] service-control lock held, skipping guardian actions"
        echo "[$TS] guardian tick ok"
        exit 0
    fi
    SHARED_LOCK_ACQUIRED=1
    export AOS_SERVICE_CONTROL_LOCK_HELD=1

    backend_pid=""
    backend_pid_alive=0
    if [ -f "$BACKEND_PID_FILE" ]; then
        backend_pid="$(cat "$BACKEND_PID_FILE" 2>/dev/null || true)"
        if [[ "$backend_pid" =~ ^[0-9]+$ ]] && kill -0 "$backend_pid" 2>/dev/null; then
            backend_pid_alive=1
        fi
    fi

    # Backend: prefer health probe, then listener/PID checks, then managed start.
    if curl -sf --max-time 2 "http://127.0.0.1:$BACKEND_PORT/healthz" >/dev/null 2>&1; then
        echo "[$TS] backend healthy on :$BACKEND_PORT"
    elif lsof -nP -i :"$BACKEND_PORT" -sTCP:LISTEN -t 2>/dev/null | head -1 >/dev/null; then
        echo "[$TS] backend listening on :$BACKEND_PORT (health pending)"
    elif [ "$backend_pid_alive" -eq 1 ]; then
        echo "[$TS] backend initializing (pid=$backend_pid port pending)"
    else
        backend_launchd_present=0
        if command -v launchctl >/dev/null 2>&1 &&
            launchctl print "$BACKEND_LAUNCHD_TARGET" >/dev/null 2>&1; then
            backend_launchd_present=1
            if launchctl kickstart "$BACKEND_LAUNCHD_TARGET" >/dev/null 2>&1; then
                record_backend_restart_event "launchd_kickstart_missing_backend"
                echo "[$TS] backend missing; requested native launchd start ($BACKEND_LAUNCHD_TARGET)"
            else
                record_backend_restart_event "launchd_kickstart_failed_fallback"
                echo "[$TS] native launchd start failed ($BACKEND_LAUNCHD_TARGET), falling back"
            fi
        fi

        # One short grace probe avoids unnecessary restarts during transient bind/health races.
        sleep 2
        if curl -sf --max-time 2 "http://127.0.0.1:$BACKEND_PORT/healthz" >/dev/null 2>&1; then
            echo "[$TS] backend recovered during grace probe on :$BACKEND_PORT"
        elif lsof -nP -i :"$BACKEND_PORT" -sTCP:LISTEN -t 2>/dev/null | head -1 >/dev/null; then
            echo "[$TS] backend listener appeared during grace probe on :$BACKEND_PORT"
        elif [ "$backend_pid_alive" -eq 1 ] && kill -0 "$backend_pid" 2>/dev/null; then
            echo "[$TS] backend pid still alive during grace probe (pid=$backend_pid)"
        else
            if [ "$backend_launchd_present" -eq 1 ]; then
                echo "[$TS] backend still unavailable after launchd start request"
            else
                if ! "$PROJECT_ROOT/scripts/service-manager.sh" start backend; then
                    record_backend_restart_event "service_manager_start_failed_missing_backend"
                    echo "[$TS] backend start failed"
                else
                    record_backend_restart_event "service_manager_start_missing_backend"
                fi
            fi
        fi
    fi

    # Worker supervision:
    # - If PID is alive and socket exists: healthy, do nothing.
    # - If PID is alive and socket missing: worker is still initializing, do nothing.
    # - If socket exists but PID is dead/missing: stale socket, trigger managed restart.
    # - If neither exists: trigger managed start.
    worker_pid=""
    worker_pid_alive=0
    if [ -f "$WORKER_PID_FILE" ]; then
        worker_pid="$(cat "$WORKER_PID_FILE" 2>/dev/null || true)"
        if [[ "$worker_pid" =~ ^[0-9]+$ ]] && kill -0 "$worker_pid" 2>/dev/null; then
            worker_pid_alive=1
        fi
    fi

    if [ "$worker_pid_alive" -eq 1 ] && [ -S "$WORKER_SOCK" ]; then
        echo "[$TS] worker healthy (pid=$worker_pid socket=$WORKER_SOCK)"
    elif [ "$worker_pid_alive" -eq 1 ]; then
        echo "[$TS] worker initializing (pid=$worker_pid socket pending)"
    else
        if [ -S "$WORKER_SOCK" ]; then
            echo "[$TS] worker socket stale (no live pid), restarting worker"
        else
            echo "[$TS] worker missing (no live pid/socket), starting worker"
        fi
        if ! "$PROJECT_ROOT/scripts/service-manager.sh" start worker; then
            echo "[$TS] worker start failed"
        fi
    fi

    echo "[$TS] guardian tick ok"
} >>"$LOG_FILE" 2>&1
