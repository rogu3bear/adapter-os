#!/bin/bash
# Debug helper script for AdapterOS
# Usage: ./scripts/debug.sh [asan|tsan|ubsan|gdb|lldb|valgrind] [command]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

# Check if we're on macOS ARM64
if [[ "$(uname -m)" == "arm64" ]]; then
    log_warn "Running on ARM64 architecture. Some debuggers (rr, valgrind) are not available."
    log_info "Available sanitizers: AddressSanitizer, ThreadSanitizer, UndefinedBehaviorSanitizer"
fi

case "$1" in
    "asan")
        log_info "Building with AddressSanitizer..."
        export RUSTFLAGS="-Zsanitizer=address"
        export ASAN_OPTIONS="detect_leaks=1:detect_stack_use_after_return=1:strict_string_checks=1:detect_stack_use_after_scope=1"
        shift
        cargo build "$@"
        log_success "Built with AddressSanitizer"
        ;;

    "tsan")
        log_info "Building with ThreadSanitizer..."
        export RUSTFLAGS="-Zsanitizer=thread"
        export TSAN_OPTIONS="second_deadlock_stack=1:history_size=7"
        shift
        cargo build "$@"
        log_success "Built with ThreadSanitizer"
        ;;

    "ubsan")
        log_info "Building with UndefinedBehaviorSanitizer..."
        export RUSTFLAGS="-Zsanitizer=undefined"
        export UBSAN_OPTIONS="print_stacktrace=1:halt_on_error=1"
        shift
        cargo build "$@"
        log_success "Built with UndefinedBehaviorSanitizer"
        ;;

    "gdb")
        if command -v gdb >/dev/null 2>&1; then
            log_info "Starting GDB..."
            shift
            gdb "$@"
        else
            log_error "GDB not found. Install with: brew install gdb"
            exit 1
        fi
        ;;

    "lldb")
        if command -v lldb >/dev/null 2>&1; then
            log_info "Starting LLDB..."
            shift
            lldb "$@"
        else
            log_error "LLDB not found."
            exit 1
        fi
        ;;

    "determinism")
        log_info "Running determinism checks..."
        export AOS_DEBUG_DETERMINISM=1
        export RUST_BACKTRACE=1
        cargo test --test determinism_core_suite -- --nocapture
        log_success "Determinism checks completed"
        ;;

    "test-debug")
        log_info "Building tests for debugging..."
        export RUST_BACKTRACE=1
        cargo test --no-run "$@"
        log_success "Tests built for debugging"
        ;;

    "help"|*)
        echo "AdapterOS Debug Helper"
        echo ""
        echo "Usage: $0 <command> [args...]"
        echo ""
        echo "Commands:"
        echo "  asan [args]      Build with AddressSanitizer"
        echo "  tsan [args]      Build with ThreadSanitizer"
        echo "  ubsan [args]     Build with UndefinedBehaviorSanitizer"
        echo "  gdb [args]       Start GDB debugger"
        echo "  lldb [args]      Start LLDB debugger"
        echo "  determinism      Run determinism checks"
        echo "  test-debug       Build tests for debugging"
        echo "  help             Show this help"
        echo ""
        echo "Examples:"
        echo "  $0 asan --bin adapteros-server"
        echo "  $0 determinism"
        echo "  $0 test-debug --package adapteros-lora-router"
        ;;
esac