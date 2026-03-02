#!/bin/bash
# adapterOS Freeze Guard
# Port conflict detection and resource management (never touches external processes).
# Prompts for adapterOS resources; purely diagnostic for external processes.
#
# Usage: source scripts/lib/freeze-guard.sh
#
# Environment Variables:
#   FG_AUTO_STOP - Set to "1" or "true" to auto-stop adapterOS processes without prompting
#                  (useful for automated/agent environments)
#   FG_AUTO_KILL - Deprecated alias for FG_AUTO_STOP (still works)

# Colors for output
FG_RED='\033[0;31m'
FG_YELLOW='\033[1;33m'
FG_GREEN='\033[0;32m'
FG_BLUE='\033[0;34m'
FG_CYAN='\033[0;36m'
FG_BOLD='\033[1m'
FG_RESET='\033[0m'

# Prefer FG_AUTO_STOP; FG_AUTO_KILL kept for backward compatibility
: "${FG_AUTO_STOP:=${FG_AUTO_KILL}}"

fg_status() { printf "${FG_BLUE}[freeze-guard]${FG_RESET} %s\n" "$1"; }
fg_warn() { printf "${FG_YELLOW}[freeze-guard]${FG_RESET} %s\n" "$1"; }
fg_error() { printf "${FG_RED}[freeze-guard]${FG_RESET} %s\n" "$1"; }
fg_success() { printf "${FG_GREEN}[freeze-guard]${FG_RESET} %s\n" "$1"; }

# Check if a port is in TIME_WAIT state
# Returns: 0 if TIME_WAIT found, 1 otherwise
fg_port_in_time_wait() {
    local port="$1"
    netstat -an 2>/dev/null | grep -q "[:.]${port}.*TIME_WAIT"
}

# Check if a PID is an adapterOS process
# Returns: 0 if adapterOS, 1 otherwise
fg_is_adapteros_process() {
    local pid="$1"
    local cmd
    cmd=$(ps -p "$pid" -o command= 2>/dev/null | tr -d '\n')
    echo "$cmd" | grep -qiE "(adapteros|aos|pnpm.*dev|vite)"
}

# Get process info for a PID
fg_get_process_info() {
    local pid="$1"
    ps -p "$pid" -o pid=,command= 2>/dev/null | head -c 80
}

# Check port and return status (never kills)
# Returns: 0 if port free, 1 if occupied (with diagnostic output)
freeze_check_port() {
    local port="$1"
    local service_name="${2:-Service}"

    # Check if port is listening
    local pids
    pids=$(lsof -nP -i :"$port" -sTCP:LISTEN -t 2>/dev/null | tr '\n' ' ')

    if [ -z "$pids" ]; then
        # Port not in use by listener, check TIME_WAIT
        if fg_port_in_time_wait "$port"; then
            # TIME_WAIT does not prevent binding a new listener on modern OSes;
            # treat this as non-blocking and continue.
            fg_warn "Port $port has TIME_WAIT connections (non-blocking)"
            echo ""
            echo "  Note: TIME_WAIT does not block a new server bind."
            echo "  If you hit AddrInUse, something is actively listening:"
            echo "    lsof -nP -i :$port -sTCP:LISTEN"
            echo ""
        fi
        return 0
    fi

    # Port is occupied - analyze who owns it
    for pid in $pids; do
        local cmd_info
        cmd_info=$(fg_get_process_info "$pid")

        if fg_is_adapteros_process "$pid"; then
            # adapterOS process - offer interactive prompt
            fg_warn "Port $port is in use by adapterOS process"
            echo ""
            echo "  PID: $pid"
            echo "  Command: $cmd_info"
            echo ""

            # Interactive prompt (default: No, unless FG_AUTO_STOP is set)
            if [[ "${FG_AUTO_STOP}" == "1" || "${FG_AUTO_STOP}" == "true" ]]; then
                fg_status "Auto-stop mode: stopping adapterOS process (PID $pid)..."
                REPLY="y"
            else
                read -p "  Stop this adapterOS process? [y/N] " -n 1 -r
                echo ""
            fi

            if [[ $REPLY =~ ^[Yy]$ ]]; then
                fg_status "Stopping adapterOS process (PID $pid)..."
                kill -TERM "$pid" 2>/dev/null

                # Wait up to 10 seconds for graceful shutdown
                local waited=0
                while kill -0 "$pid" 2>/dev/null && [ $waited -lt 10 ]; do
                    sleep 1
                    waited=$((waited + 1))
                done

                if kill -0 "$pid" 2>/dev/null; then
                    fg_warn "Process still running after 10s, sending SIGKILL..."
                    kill -KILL "$pid" 2>/dev/null
                    sleep 1
                fi

                if kill -0 "$pid" 2>/dev/null; then
                    fg_error "Failed to stop process"
                    return 1
                fi

                fg_success "Process stopped"
                return 0
            else
                fg_error "Port $port blocked by existing adapterOS process"
                echo ""
                echo "  To resolve manually:"
                echo "    kill $pid"
                echo ""
                return 1
            fi
        else
            # Non-adapterOS process - pure freeze (never touch)
            fg_error "Port $port is in use by external process (NOT adapterOS)"
            echo ""
            echo "  PID: $pid"
            echo "  Command: $cmd_info"
            echo ""
            echo "  adapterOS will NOT stop external processes."
            echo "  Required action:"
            echo "    Stop the external process and retry on the same port."
            echo ""
            return 1
        fi
    done

    return 0
}

# Check for stale PID file
# Returns: 0 if no stale file or user cleaned it, 1 if blocked
freeze_check_pid_file() {
    local pid_file="$1"
    local service_name="${2:-Service}"

    if [ ! -f "$pid_file" ]; then
        return 0
    fi

    local pid
    pid=$(cat "$pid_file" 2>/dev/null)

    if [ -z "$pid" ]; then
        # Empty PID file - offer to remove
        fg_warn "Empty PID file found: $pid_file"
        if [[ "${FG_AUTO_STOP}" == "1" || "${FG_AUTO_STOP}" == "true" ]]; then
            fg_status "Auto-stop mode: removing empty PID file..."
            REPLY="y"
        else
            read -p "  Remove empty PID file? [y/N] " -n 1 -r
            echo ""
        fi
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            rm -f "$pid_file"
            fg_success "Removed empty PID file"
            return 0
        fi
        return 1
    fi

    # Check if process is still running
    if kill -0 "$pid" 2>/dev/null; then
        # Process exists - verify it's adapterOS
        if fg_is_adapteros_process "$pid"; then
            fg_warn "$service_name appears to be running (PID $pid)"
            echo ""
            echo "  Command: $(fg_get_process_info "$pid")"
            echo ""
            if [[ "${FG_AUTO_STOP}" == "1" || "${FG_AUTO_STOP}" == "true" ]]; then
                fg_status "Auto-stop mode: stopping existing $service_name..."
                REPLY="y"
            else
                read -p "  Stop existing $service_name? [y/N] " -n 1 -r
                echo ""
            fi
            if [[ $REPLY =~ ^[Yy]$ ]]; then
                kill -TERM "$pid" 2>/dev/null
                sleep 2
                if kill -0 "$pid" 2>/dev/null; then
                    kill -KILL "$pid" 2>/dev/null
                    sleep 1
                fi
                rm -f "$pid_file"
                fg_success "Stopped $service_name"
                return 0
            fi
            return 1
        else
            fg_error "PID file points to non-adapterOS process"
            echo ""
            echo "  PID file: $pid_file"
            echo "  PID: $pid"
            echo "  Command: $(fg_get_process_info "$pid")"
            echo ""
            echo "  This is unexpected. The PID file may be stale."
            if [[ "${FG_AUTO_STOP}" == "1" || "${FG_AUTO_STOP}" == "true" ]]; then
                fg_status "Auto-stop mode: removing stale PID file..."
                REPLY="y"
            else
                read -p "  Remove stale PID file? [y/N] " -n 1 -r
                echo ""
            fi
            if [[ $REPLY =~ ^[Yy]$ ]]; then
                rm -f "$pid_file"
                fg_success "Removed stale PID file"
                return 0
            fi
            return 1
        fi
    else
        # Process not running - stale PID file
        fg_warn "Stale PID file found: $pid_file (process $pid not running)"
        if [[ "${FG_AUTO_STOP}" == "1" || "${FG_AUTO_STOP}" == "true" ]]; then
            fg_status "Auto-stop mode: removing stale PID file..."
            REPLY="y"
        else
            read -p "  Remove stale PID file? [y/N] " -n 1 -r
            echo ""
        fi
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            rm -f "$pid_file"
            fg_success "Removed stale PID file"
            return 0
        fi
        return 1
    fi
}

# Check for stale Unix socket
# Returns: 0 if no stale socket or user cleaned it, 1 if blocked
freeze_check_socket() {
    local socket_path="$1"
    local service_name="${2:-Service}"

    if [ ! -S "$socket_path" ]; then
        return 0
    fi

    # Check if anything is listening on it
    if lsof "$socket_path" >/dev/null 2>&1; then
        local listener_pid
        listener_pid=$(lsof -t "$socket_path" 2>/dev/null | head -1)

        if [ -n "$listener_pid" ]; then
            if fg_is_adapteros_process "$listener_pid"; then
                fg_warn "Socket in use by adapterOS (PID $listener_pid)"
                if [[ "${FG_AUTO_STOP}" == "1" || "${FG_AUTO_STOP}" == "true" ]]; then
                    fg_status "Auto-stop mode: stopping process using socket..."
                    REPLY="y"
                else
                    read -p "  Stop the process using this socket? [y/N] " -n 1 -r
                    echo ""
                fi
                if [[ $REPLY =~ ^[Yy]$ ]]; then
                    kill -TERM "$listener_pid" 2>/dev/null
                    sleep 2
                    rm -f "$socket_path" 2>/dev/null
                    fg_success "Stopped process and removed socket"
                    return 0
                fi
                return 1
            else
                fg_error "Socket in use by non-adapterOS process"
                echo "  PID: $listener_pid"
                echo "  Command: $(fg_get_process_info "$listener_pid")"
                return 1
            fi
        fi
    fi

    # Socket exists but nothing listening - stale
    fg_warn "Stale socket found: $socket_path"
    if [[ "${FG_AUTO_STOP}" == "1" || "${FG_AUTO_STOP}" == "true" ]]; then
        fg_status "Auto-stop mode: removing stale socket..."
        REPLY="y"
    else
        read -p "  Remove stale socket? [y/N] " -n 1 -r
        echo ""
    fi
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -f "$socket_path"
        fg_success "Removed stale socket"
        return 0
    fi
    return 1
}

# Check for database lock
# Returns: 0 if no lock, 1 if locked
freeze_check_db_lock() {
    local db_path="$1"

    if [ ! -f "$db_path" ]; then
        return 0
    fi

    # Check for SQLite lock files
    if [ -f "${db_path}-wal" ] || [ -f "${db_path}-shm" ]; then
        local holder_pid
        holder_pid=$(lsof "$db_path" 2>/dev/null | grep -v "^COMMAND" | awk '{print $2}' | head -1)

        if [ -n "$holder_pid" ]; then
            fg_error "Database is locked by another process"
            echo ""
            echo "  Database: $db_path"
            echo "  Holder PID: $holder_pid"
            echo "  Command: $(fg_get_process_info "$holder_pid")"
            echo ""
            echo "  Options:"
            echo "    1. Stop the process holding the lock"
            echo "    2. Wait for it to release"
            echo ""
            return 1
        fi
    fi

    return 0
}

# Run all preflight checks for startup
# Returns: 0 if all clear, 1 if any blocked
freeze_preflight() {
    local backend_port="${1:-${AOS_SERVER_PORT:-8080}}"
    local ui_port="${2:-${AOS_UI_PORT:-3200}}"
    local var_dir="${3:-var}"

    local failed=0

    fg_status "Running preflight checks..."
    echo ""

    # Check backend port
    if ! freeze_check_port "$backend_port" "Backend API"; then
        failed=1
    fi

    # Check UI port
    if ! freeze_check_port "$ui_port" "UI Server"; then
        failed=1
    fi

    # Check PID files (in var/, not var/run/)
    if ! freeze_check_pid_file "$var_dir/backend.pid" "Backend"; then
        failed=1
    fi

    if ! freeze_check_pid_file "$var_dir/ui.pid" "UI"; then
        failed=1
    fi

    # Check worker socket (in var/run/)
    if ! freeze_check_socket "$var_dir/run/worker.sock" "Worker"; then
        failed=1
    fi

    # Check database
    if ! freeze_check_db_lock "$var_dir/aos-cp.sqlite3"; then
        failed=1
    fi

    if [ $failed -eq 0 ]; then
        fg_success "All preflight checks passed"
    else
        fg_error "Preflight checks failed - resolve issues above before starting"
    fi

    return $failed
}
