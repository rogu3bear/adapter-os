#!/bin/bash
#
# Free Ports (safe by default)
# - Shows which process is listening on the given ports
# - Prints kill commands (does NOT kill unless --force is provided)
#
# Usage:
#   scripts/free-ports.sh [--force] [PORT...]
#   scripts/free-ports.sh --help
#
# Defaults:
#   PORTS: 8080 3200
#
# Environment:
#   FREE_PORTS_GRACE_TIMEOUT  Seconds to wait after SIGTERM (default: 10)
#   FREE_PORTS_FORCE_TIMEOUT  Seconds to wait after SIGKILL (default: 3)

set -euo pipefail

DEFAULT_PORTS=(8080 3200)
: "${FREE_PORTS_GRACE_TIMEOUT:=10}"
: "${FREE_PORTS_FORCE_TIMEOUT:=3}"

usage() {
    cat <<'EOF'
Usage:
  scripts/free-ports.sh [--force] [PORT...]

Options:
  -f, --force   Actually send signals to stop listeners (SIGTERM, then SIGKILL)
  -h, --help    Show this help

Notes:
  - With no PORT args, defaults to: 8080 3200
  - PORT args may be comma-separated (e.g. "8080,3200")
  - Without --force, this script only prints suggested kill commands.

Examples:
  scripts/free-ports.sh
  scripts/free-ports.sh 8080 3200
  scripts/free-ports.sh --force 8080,3200
EOF
}

die() {
    printf "error: %s\n" "$1" >&2
    exit 2
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

trim() {
    # shellcheck disable=SC2001
    printf "%s" "$1" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//'
}

is_valid_port() {
    local port="$1"
    [[ "$port" =~ ^[0-9]+$ ]] || return 1
    ((port >= 1 && port <= 65535))
}

pids_for_port() {
    local port="$1"
    lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null | sort -u || true
}

show_lsof_for_port() {
    local port="$1"
    lsof -nP -iTCP:"$port" -sTCP:LISTEN 2>/dev/null || true
}

pid_user() {
    local pid="$1"
    ps -p "$pid" -o user= 2>/dev/null | awk '{print $1}' || true
}

pid_command() {
    local pid="$1"
    trim "$(ps -p "$pid" -o command= 2>/dev/null || true)"
}

kill_prefix_for_pid() {
    local pid="$1"
    local current_user="$2"
    local owner
    owner="$(pid_user "$pid")"
    if [[ -n "$owner" && "$owner" != "$current_user" ]]; then
        printf "sudo "
    else
        printf ""
    fi
}

stop_pid() {
    local pid="$1"
    local grace_timeout="$2"
    local force_timeout="$3"

    if ! kill -0 "$pid" 2>/dev/null; then
        return 0
    fi

    if ! kill -TERM "$pid" 2>/dev/null; then
        return 1
    fi

    local start now
    start="$(date +%s)"
    while kill -0 "$pid" 2>/dev/null; do
        now="$(date +%s)"
        if ((now - start >= grace_timeout)); then
            kill -KILL "$pid" 2>/dev/null || true
            break
        fi
        sleep 1
    done

    start="$(date +%s)"
    while kill -0 "$pid" 2>/dev/null; do
        now="$(date +%s)"
        if ((now - start >= force_timeout)); then
            return 1
        fi
        sleep 1
    done

    return 0
}

main() {
    local force=0
    local -a ports=()

    while (($# > 0)); do
        case "$1" in
            -f|--force) force=1; shift ;;
            -h|--help) usage; exit 0 ;;
            --) shift; ports+=("$@"); break ;;
            -*)
                die "unknown option: $1"
                ;;
            *)
                ports+=("$1")
                shift
                ;;
        esac
    done

    if ((${#ports[@]} == 0)); then
        ports=("${DEFAULT_PORTS[@]}")
    fi

    # Expand comma-separated args.
    local -a expanded_ports=()
    local arg
    for arg in "${ports[@]}"; do
        local -a parts=()
        IFS=',' read -r -a parts <<< "$arg"
        local part
        for part in "${parts[@]}"; do
            part="$(trim "$part")"
            [[ -n "$part" ]] && expanded_ports+=("$part")
        done
    done
    ports=("${expanded_ports[@]}")

    if ((${#ports[@]} == 0)); then
        die "no ports provided"
    fi

    local port
    for port in "${ports[@]}"; do
        is_valid_port "$port" || die "invalid port: $port"
    done

    # De-duplicate while preserving order.
    local -a unique_ports=()
    local seen=" "
    for port in "${ports[@]}"; do
        if [[ "$seen" != *" $port "* ]]; then
            unique_ports+=("$port")
            seen+=" $port "
        fi
    done
    ports=("${unique_ports[@]}")

    need_cmd lsof
    need_cmd ps

    local current_user
    current_user="$(id -un)"

    local any_in_use=0
    local kill_failures=0

    for port in "${ports[@]}"; do
        printf "Port %s:\n" "$port"

        local pids
        pids="$(pids_for_port "$port")"

        if [[ -z "$pids" ]]; then
            printf "  free\n\n"
            continue
        fi

        any_in_use=1

        show_lsof_for_port "$port" | sed 's/^/  /' || true

        printf "  Suggested kill commands:\n"
        local pid
        while IFS= read -r pid; do
            [[ -n "$pid" ]] || continue
            local cmd prefix
            cmd="$(pid_command "$pid")"
            prefix="$(kill_prefix_for_pid "$pid" "$current_user")"
            if [[ -n "$cmd" ]]; then
                printf "    %skill -TERM %s  # %s\n" "$prefix" "$pid" "$cmd"
            else
                printf "    %skill -TERM %s\n" "$prefix" "$pid"
            fi
        done <<< "$pids"

        if ((force)); then
            printf "  --force: stopping listeners on port %s\n" "$port"
            while IFS= read -r pid; do
                [[ -n "$pid" ]] || continue
                if stop_pid "$pid" "$FREE_PORTS_GRACE_TIMEOUT" "$FREE_PORTS_FORCE_TIMEOUT"; then
                    printf "    stopped PID %s\n" "$pid"
                else
                    printf "    failed to stop PID %s (try the suggested command, possibly with sudo)\n" "$pid" >&2
                    kill_failures=1
                fi
            done <<< "$pids"

            if [[ -n "$(pids_for_port "$port")" ]]; then
                printf "  still in use\n\n"
                kill_failures=1
            else
                printf "  now free\n\n"
            fi
        else
            printf "  (re-run with --force to stop listeners)\n\n"
        fi
    done

    if ((any_in_use)) && ((force == 0)); then
        exit 1
    fi

    exit "$kill_failures"
}

main "$@"
