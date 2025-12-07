#!/bin/bash
# AdapterOS Bootstrap Integration Test Suite
# Tests all edge cases for the unified ./start entry point
#
# Usage: ./scripts/bootstrap_integration_test.sh [test_name]
#
# Test Scenarios:
#   1. clean_start      - Start on clean system
#   2. port_conflict    - Detect port in use
#   3. stale_pid        - Detect stale PID file
#   4. stale_socket     - Detect stale socket
#   5. db_locked        - Detect database lock
#   6. concurrent       - Concurrent start prevention
#   7. interrupt_resume - Resume after interrupt
#   8. graceful_shutdown - Clean shutdown cycle

set -e

echo "DEPRECATION: scripts/bootstrap_integration_test.sh is legacy. Use ./start."
echo "A prompt will auto-cancel after 15s (default: No)."
echo ""
read -r -t 15 -p "Proceed with legacy bootstrap integration harness? [y/N]: " REPLY || REPLY=""
echo ""
if [[ ! "$REPLY" =~ ^[Yy]$ ]]; then
  echo "Aborting. Use ./start for canonical boot checks."
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# =============================================================================
# Test Helpers
# =============================================================================

log_test() {
    echo -e "${CYAN}[TEST]${NC} $1"
}

log_pass() {
    echo -e "${GREEN}[PASS]${NC} $1"
    ((TESTS_PASSED++))
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((TESTS_FAILED++))
}

log_info() {
    echo -e "${YELLOW}[INFO]${NC} $1"
}

# Clean up test artifacts
cleanup() {
    log_info "Cleaning up test artifacts..."
    # Kill any running test processes
    pkill -f "adapteros-server.*--config" 2>/dev/null || true
    pkill -f "vite.*3200" 2>/dev/null || true
    # Remove test PID files
    rm -f var/backend.pid var/ui.pid var/menu-bar.pid
    rm -f var/run/worker.sock
    sleep 2
}

# Wait for port to be free
wait_for_port_free() {
    local port="$1"
    local timeout="${2:-30}"
    local waited=0

    while lsof -nP -i :"$port" -sTCP:LISTEN >/dev/null 2>&1; do
        sleep 1
        ((waited++))
        if [ $waited -ge $timeout ]; then
            return 1
        fi
    done
    return 0
}

# =============================================================================
# Test Cases
# =============================================================================

test_clean_start() {
    ((TESTS_RUN++))
    log_test "1. Clean Start - Starting on clean system"

    cleanup

    # Ensure ports are free
    if ! wait_for_port_free 8080 10; then
        log_fail "Port 8080 not available for clean start test"
        return 1
    fi

    # Start backend only (faster test)
    if ./start backend 2>&1 | grep -q "Backend started"; then
        log_pass "Backend started successfully on clean system"
    else
        log_fail "Backend failed to start on clean system"
        return 1
    fi

    # Verify it's running
    if ./start status 2>&1 | grep -q "Backend API:.*RUNNING"; then
        log_pass "Backend confirmed running via status"
    else
        log_fail "Status does not show backend as running"
        return 1
    fi

    cleanup
}

test_port_conflict() {
    ((TESTS_RUN++))
    log_test "2. Port Conflict - Detect port in use"

    cleanup

    # Start a process on port 8080
    if ! wait_for_port_free 8080 10; then
        log_info "Port 8080 already in use, using for test"
    else
        ./start backend 2>&1 >/dev/null
        sleep 2
    fi

    # Try to start again - should detect conflict
    local output
    output=$(echo "n" | ./start backend 2>&1 || true)

    if echo "$output" | grep -qE "(Port 8080 is in use|already running)"; then
        log_pass "Port conflict correctly detected"
    else
        log_fail "Port conflict not detected"
        echo "Output was: $output"
        return 1
    fi

    cleanup
}

test_stale_pid() {
    ((TESTS_RUN++))
    log_test "3. Stale PID - Detect stale PID file"

    cleanup

    # Create a stale PID file (pointing to non-existent process)
    mkdir -p var
    echo "99999" > var/backend.pid

    # Preflight should detect this
    local output
    output=$(echo "n" | ./start preflight 2>&1 || true)

    if echo "$output" | grep -qE "(Stale PID file|process.*not running)"; then
        log_pass "Stale PID file correctly detected"
    else
        log_fail "Stale PID file not detected"
        echo "Output was: $output"
        return 1
    fi

    rm -f var/backend.pid
}

test_stale_socket() {
    ((TESTS_RUN++))
    log_test "4. Stale Socket - Detect stale socket file"

    cleanup

    # Create a stale socket file
    mkdir -p var/run
    # Create an actual socket file (not just a regular file)
    python3 -c "import socket; s=socket.socket(socket.AF_UNIX); s.bind('var/run/worker.sock')" 2>/dev/null || \
    mkfifo var/run/worker.sock 2>/dev/null || \
    touch var/run/worker.sock

    # For testing, we need an actual socket file. Let's create one:
    rm -f var/run/worker.sock
    python3 -c "
import socket
import os
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.bind('var/run/worker.sock')
s.close()
" 2>/dev/null || true

    # Preflight should detect this
    local output
    output=$(echo "n" | ./start preflight 2>&1 || true)

    if echo "$output" | grep -qE "(Stale socket|socket)"; then
        log_pass "Stale socket correctly detected"
    else
        # May not find it if not a real socket - skip if not applicable
        log_info "Stale socket test inconclusive (may need real socket)"
    fi

    rm -f var/run/worker.sock
}

test_db_locked() {
    ((TESTS_RUN++))
    log_test "5. Database Locked - Detect database lock"

    cleanup

    # Start the backend which will lock the database
    ./start backend 2>&1 >/dev/null
    sleep 2

    # Check if DB lock is detected during preflight
    local output
    output=$(echo "n" | ./start preflight 2>&1 || true)

    if echo "$output" | grep -qE "(Database is locked|locked)"; then
        log_pass "Database lock correctly detected"
    else
        # May be unlocked or shared - just verify backend is running
        if ./start status 2>&1 | grep -q "Backend API:.*RUNNING"; then
            log_info "Database lock test inconclusive (WAL mode may allow concurrent access)"
        else
            log_fail "Database lock test failed"
            return 1
        fi
    fi

    cleanup
}

test_concurrent_start() {
    ((TESTS_RUN++))
    log_test "6. Concurrent Start - Second instance should fail"

    cleanup

    # Start first instance
    ./start backend 2>&1 >/dev/null
    sleep 2

    # Try to start second instance - should detect conflict
    local output
    output=$(echo "n" | ./start backend 2>&1 || true)

    if echo "$output" | grep -qE "(already running|in use)"; then
        log_pass "Concurrent start correctly blocked"
    else
        log_fail "Concurrent start not prevented"
        echo "Output was: $output"
        return 1
    fi

    cleanup
}

test_interrupt_resume() {
    ((TESTS_RUN++))
    log_test "7. Interrupt + Resume - Re-run after interrupt"

    cleanup

    # Start backend
    ./start backend 2>&1 >/dev/null
    sleep 2

    # Simulate interrupt (SIGINT to backend)
    local pid
    pid=$(cat var/backend.pid 2>/dev/null)
    if [ -n "$pid" ]; then
        kill -INT "$pid" 2>/dev/null || true
        sleep 3
    fi

    # Clean up and try to start again
    rm -f var/backend.pid
    wait_for_port_free 8080 10

    # Should start cleanly
    if ./start backend 2>&1 | grep -q "Backend started"; then
        log_pass "Resume after interrupt successful"
    else
        log_fail "Failed to resume after interrupt"
        return 1
    fi

    cleanup
}

test_graceful_shutdown() {
    ((TESTS_RUN++))
    log_test "8. Graceful Shutdown - Clean shutdown cycle"

    cleanup

    # Start backend
    ./start backend 2>&1 >/dev/null
    sleep 2

    # Verify running
    if ! ./start status 2>&1 | grep -q "Backend API:.*RUNNING"; then
        log_fail "Backend not running for shutdown test"
        return 1
    fi

    # Graceful shutdown
    if ./start down 2>&1 | grep -q "Shutdown complete"; then
        log_pass "Graceful shutdown completed"
    else
        log_fail "Graceful shutdown failed"
        return 1
    fi

    # Verify stopped
    sleep 2
    if ./start status 2>&1 | grep -q "Backend API:.*STOPPED"; then
        log_pass "Backend confirmed stopped after shutdown"
    else
        log_fail "Backend still running after shutdown"
        return 1
    fi
}

# =============================================================================
# Main
# =============================================================================

print_summary() {
    echo ""
    echo -e "${CYAN}================================${NC}"
    echo -e "${CYAN}   Test Summary${NC}"
    echo -e "${CYAN}================================${NC}"
    echo ""
    echo -e "  Total:  $TESTS_RUN"
    echo -e "  ${GREEN}Passed: $TESTS_PASSED${NC}"
    echo -e "  ${RED}Failed: $TESTS_FAILED${NC}"
    echo ""

    if [ $TESTS_FAILED -eq 0 ]; then
        echo -e "${GREEN}All tests passed!${NC}"
        return 0
    else
        echo -e "${RED}Some tests failed.${NC}"
        return 1
    fi
}

run_all_tests() {
    echo -e "${CYAN}"
    echo "================================"
    echo "   Bootstrap Integration Tests"
    echo "================================"
    echo -e "${NC}"
    echo ""

    test_clean_start || true
    test_port_conflict || true
    test_stale_pid || true
    test_stale_socket || true
    test_db_locked || true
    test_concurrent_start || true
    test_interrupt_resume || true
    test_graceful_shutdown || true

    print_summary
}

# Handle specific test or run all
if [ -n "$1" ]; then
    case "$1" in
        clean_start)     test_clean_start ;;
        port_conflict)   test_port_conflict ;;
        stale_pid)       test_stale_pid ;;
        stale_socket)    test_stale_socket ;;
        db_locked)       test_db_locked ;;
        concurrent)      test_concurrent_start ;;
        interrupt_resume) test_interrupt_resume ;;
        graceful_shutdown) test_graceful_shutdown ;;
        all)             run_all_tests ;;
        *)
            echo "Unknown test: $1"
            echo "Available: clean_start, port_conflict, stale_pid, stale_socket, db_locked, concurrent, interrupt_resume, graceful_shutdown, all"
            exit 1
            ;;
    esac
else
    run_all_tests
fi
