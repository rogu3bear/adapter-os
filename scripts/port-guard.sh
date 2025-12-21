#!/bin/bash
# AdapterOS Port Guard
# Shared helpers to keep dev ports stable across rebuilds and restarts.
#
# Safe to source from other scripts; does not modify shell options.

# Timings (override via env when needed)
: "${PORT_GUARD_GRACE_TIMEOUT:=15}"   # seconds to wait after SIGTERM
: "${PORT_GUARD_FORCE_TIMEOUT:=5}"    # seconds to wait after SIGKILL

# Lightweight logging helpers (colors optional for readability)
pg_status() { printf "\033[0;34m[port-guard]\033[0m %s\n" "$1"; }
pg_warn() { printf "\033[1;33m[port-guard]\033[0m %s\n" "$1"; }
pg_error() { printf "\033[0;31m[port-guard]\033[0m %s\n" "$1"; }

# Check whether a port is currently occupied
pg_port_in_use() {
    local port="$1"
    lsof -nP -i :"$port" -sTCP:LISTEN >/dev/null 2>&1
}

# List PIDs that look like AdapterOS-owned listeners for a port
pg_adapter_pids_for_port() {
    local port="$1"
    local pids
    pids=$(lsof -nP -i :"$port" -sTCP:LISTEN -t 2>/dev/null | tr '\n' ' ')

    for pid in $pids; do
        local cmd
        cmd=$(ps -p "$pid" -o command= 2>/dev/null | tr -d '\n')
        if echo "$cmd" | grep -qiE "(adapteros|aos|pnpm.*dev|vite)"; then
            printf "%s " "$pid"
        fi
    done
}

# Attempt to stop a PID gracefully, then forcefully if needed
pg_stop_pid() {
    local pid="$1"
    local service_name="$2"
    local port="$3"

    pg_status "Sending SIGTERM to $service_name (PID $pid) on port $port"
    kill -TERM "$pid" 2>/dev/null || return 0

    local start
    start=$(date +%s)
    while kill -0 "$pid" 2>/dev/null; do
        if [ $(($(date +%s) - start)) -ge "$PORT_GUARD_GRACE_TIMEOUT" ]; then
            pg_warn "$service_name (PID $pid) still running after ${PORT_GUARD_GRACE_TIMEOUT}s; forcing stop"
            kill -KILL "$pid" 2>/dev/null || true
            break
        fi
        sleep 1
    done

    # Wait briefly to confirm the force kill if it was needed
    start=$(date +%s)
    while kill -0 "$pid" 2>/dev/null; do
        if [ $(($(date +%s) - start)) -ge "$PORT_GUARD_FORCE_TIMEOUT" ]; then
            pg_error "$service_name (PID $pid) resisted shutdown"
            return 1
        fi
        sleep 1
    done

    pg_status "$service_name (PID $pid) stopped"
    return 0
}

# Ensure a port is free; attempts graceful cleanup for AdapterOS-looking processes
ensure_port_free() {
    local port="$1"
    local service_name="${2:-Service}"

    if ! pg_port_in_use "$port"; then
        return 0
    fi

    local adapter_pids
    adapter_pids=$(pg_adapter_pids_for_port "$port")

    if [ -z "$adapter_pids" ]; then
        pg_warn "Port $port is in use by non-AdapterOS process; leaving untouched"
        return 1
    fi

    pg_status "Port $port is in use; attempting graceful cleanup for $service_name"
    local pid
    for pid in $adapter_pids; do
        pg_stop_pid "$pid" "$service_name" "$port" || true
    done

    if pg_port_in_use "$port"; then
        pg_error "Port $port is still occupied after cleanup"
        return 1
    fi

    pg_status "Port $port is now free for $service_name"
    return 0
}
