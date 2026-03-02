#!/usr/bin/env bash
# =============================================================================
# adapterOS Test Pyramid Runner
# =============================================================================
#
# This script runs the test pyramid with correct thread caps, environment
# variables, and suite organization. It captures failure artifacts and
# provides actionable output.
#
# Usage:
#   ./scripts/test/run_test_pyramid.sh           # Run PR suite (default)
#   ./scripts/test/run_test_pyramid.sh --full    # Run full suite
#   ./scripts/test/run_test_pyramid.sh --nightly # Run nightly suite
#   ./scripts/test/run_test_pyramid.sh unit      # Run specific suite
#
# PRD References:
#   - PRD-DET-001: Determinism Hardening
#   - PRD-DET-002: Dual-Write Drift Detection
#
# =============================================================================

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# =============================================================================
# Configuration
# =============================================================================

# Runtime budgets (in seconds)
UNIT_TIMEOUT=600        # 10 minutes
INTEGRATION_TIMEOUT=1200 # 20 minutes
GOLD_E2E_TIMEOUT=300    # 5 minutes
FULL_E2E_TIMEOUT=1800   # 30 minutes
DETERMINISM_TIMEOUT=900 # 15 minutes
REPLAY_TIMEOUT=1200     # 20 minutes
STREAMING_TIMEOUT=600   # 10 minutes

# Thread caps
UNIT_THREADS=8
INTEGRATION_THREADS=4
E2E_THREADS=2
REPLAY_THREADS=1
STREAMING_THREADS=2

# Failure artifact directory
FAILURE_DIR="${REPO_ROOT}/target/test-failures"

# =============================================================================
# Environment Variables
# =============================================================================

setup_base_env() {
    export AOS_DEV_NO_AUTH=1
    export AOS_BACKEND=mock
    export RUST_BACKTRACE=1
    export RUST_LOG="adapteros=debug,tower_http=warn"
    export AOS_ALLOW_LEGACY_AOS=0
}

setup_determinism_env() {
    setup_base_env
    # Fixed seed: 42 repeated as hex
    export AOS_DETERMINISM_SEED="2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a"
}

setup_debug_env() {
    setup_determinism_env
    export AOS_DEBUG_DETERMINISM=1
}

setup_dual_write_env() {
    setup_determinism_env
    export AOS_STORAGE_BACKEND=dual_write
    export AOS_ATOMIC_DUAL_WRITE_STRICT=true
}

# =============================================================================
# Helper Functions
# =============================================================================

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $1"
}

log_section() {
    echo ""
    echo "============================================================================="
    echo -e "${BLUE}$1${NC}"
    echo "============================================================================="
}

print_env() {
    log_info "Environment variables set:"
    echo "  AOS_DEV_NO_AUTH=${AOS_DEV_NO_AUTH:-<unset>}"
    echo "  AOS_BACKEND=${AOS_BACKEND:-<unset>}"
    echo "  AOS_DETERMINISM_SEED=${AOS_DETERMINISM_SEED:-<unset>}"
    echo "  AOS_DEBUG_DETERMINISM=${AOS_DEBUG_DETERMINISM:-<unset>}"
    echo "  AOS_STORAGE_BACKEND=${AOS_STORAGE_BACKEND:-<unset>}"
    echo "  RUST_LOG=${RUST_LOG:-<unset>}"
}

ensure_failure_dir() {
    mkdir -p "${FAILURE_DIR}"
}

check_failures() {
    local count
    count=$(find "${FAILURE_DIR}" -name "*.json" -type f 2>/dev/null | wc -l | tr -d ' ')
    if [[ "$count" -gt 0 ]]; then
        log_warning "Failure artifacts found: ${count} files in ${FAILURE_DIR}"
        return 1
    fi
    return 0
}

run_with_timeout() {
    local timeout=$1
    local name=$2
    shift 2

    log_info "Running: $name (timeout: ${timeout}s)"

    if timeout "${timeout}" "$@"; then
        log_success "$name passed"
        return 0
    else
        local exit_code=$?
        if [[ $exit_code -eq 124 ]]; then
            log_error "$name timed out after ${timeout}s"
        else
            log_error "$name failed with exit code $exit_code"
        fi
        return $exit_code
    fi
}

# =============================================================================
# Test Suite Functions
# =============================================================================

run_unit_tests() {
    log_section "Unit Tests"
    setup_base_env
    print_env

    run_with_timeout "$UNIT_TIMEOUT" "Unit tests" \
        cargo test --workspace --lib -- --test-threads="${UNIT_THREADS}"
}

run_integration_tests() {
    log_section "Integration Tests"
    setup_base_env
    print_env

    run_with_timeout "$INTEGRATION_TIMEOUT" "Integration tests" \
        cargo test --workspace --tests -- \
            --test-threads="${INTEGRATION_THREADS}" \
            --skip gold_standard \
            --skip determinism_replay \
            --skip streaming_reliability
}

run_gold_standard_e2e() {
    log_section "Gold Standard E2E Test"
    setup_determinism_env
    print_env
    ensure_failure_dir

    run_with_timeout "$GOLD_E2E_TIMEOUT" "Gold standard E2E" \
        cargo test --test gold_standard_e2e -- \
            --test-threads="${E2E_THREADS}" \
            --nocapture
}

run_full_e2e() {
    log_section "Full E2E Tests"
    setup_determinism_env
    print_env
    ensure_failure_dir

    # Run all E2E tests including the canonical e2e_inference_test
    run_with_timeout "$FULL_E2E_TIMEOUT" "Full E2E suite" \
        cargo test -p adapteros-server-api --tests -- \
            --test-threads="${E2E_THREADS}" \
            --nocapture \
            e2e
}

run_determinism_suite() {
    log_section "Determinism Test Suite"
    setup_determinism_env
    print_env

    run_with_timeout "$DETERMINISM_TIMEOUT" "Determinism suite" \
        cargo test --workspace -- \
            --test-threads="${INTEGRATION_THREADS}" \
            determinism
}

run_replay_harness() {
    log_section "Determinism Replay Harness"
    setup_determinism_env
    print_env
    ensure_failure_dir

    # MUST run with --test-threads=1 for serial execution
    run_with_timeout "$REPLAY_TIMEOUT" "Replay harness" \
        cargo test -p adapter-os --no-default-features --test determinism_replay_harness -- \
            --test-threads="${REPLAY_THREADS}" \
            --nocapture
}

run_streaming_tests() {
    log_section "Streaming Reliability Tests"
    setup_base_env
    print_env
    ensure_failure_dir

    run_with_timeout "$STREAMING_TIMEOUT" "Streaming reliability" \
        cargo test -p adapteros-server-api --test streaming_reliability -- \
            --test-threads="${STREAMING_THREADS}" \
            --nocapture
}

run_dual_write_tests() {
    log_section "Dual-Write Drift Detection Tests"
    setup_dual_write_env
    print_env

    run_with_timeout "$DETERMINISM_TIMEOUT" "Dual-write tests" \
        cargo test -p adapteros-db --test atomic_dual_write_tests -- \
            --test-threads=2 \
            --nocapture
}

run_stress_tests() {
    log_section "Stress Tests"
    setup_determinism_env
    print_env

    # Run FIFO determinism stress and other stress tests
    run_with_timeout "$FULL_E2E_TIMEOUT" "Stress tests" \
        cargo test --workspace -- \
            --test-threads="${INTEGRATION_THREADS}" \
            stress
}

# =============================================================================
# Suite Compositions
# =============================================================================

run_pr_suite() {
    log_section "PR Test Suite (unit + integration + gold-standard-e2e)"

    local failed=0

    run_unit_tests || failed=1
    run_integration_tests || failed=1
    run_gold_standard_e2e || failed=1

    return $failed
}

run_full_suite() {
    log_section "Full Test Suite (PR + full-e2e + determinism)"

    local failed=0

    # PR suite
    run_unit_tests || failed=1
    run_integration_tests || failed=1
    run_gold_standard_e2e || failed=1

    # Additional full suite tests
    run_full_e2e || failed=1
    run_determinism_suite || failed=1

    return $failed
}

run_nightly_suite() {
    log_section "Nightly Test Suite (full + replay + streaming + stress)"

    local failed=0

    # Full suite
    run_unit_tests || failed=1
    run_integration_tests || failed=1
    run_gold_standard_e2e || failed=1
    run_full_e2e || failed=1
    run_determinism_suite || failed=1

    # Nightly-specific tests
    run_replay_harness || failed=1
    run_streaming_tests || failed=1
    run_dual_write_tests || failed=1
    run_stress_tests || failed=1

    return $failed
}

# =============================================================================
# Main
# =============================================================================

print_usage() {
    cat << EOF
adapterOS Test Pyramid Runner

Usage: $0 [OPTIONS] [SUITE]

Options:
    --full      Run full test suite (PR + e2e + determinism)
    --nightly   Run nightly test suite (full + replay + streaming + stress)
    --help, -h  Show this help message

Suites:
    unit            Run unit tests only
    integration     Run integration tests only
    gold-standard   Run gold standard E2E test only
    e2e-full        Run full E2E suite
    determinism     Run determinism test suite
    replay          Run replay harness
    streaming       Run streaming reliability tests
    dual-write      Run dual-write drift tests
    stress          Run stress tests

Examples:
    $0                  # Run PR suite (default)
    $0 --full           # Run full suite
    $0 --nightly        # Run nightly suite
    $0 unit             # Run unit tests only
    $0 replay           # Run replay harness only

Environment Variables:
    AOS_DEV_NO_AUTH             Disable authentication (set to 1)
    AOS_DETERMINISM_SEED        Fixed seed for determinism (hex)
    AOS_DEBUG_DETERMINISM       Enable determinism debug logging
    AOS_STORAGE_BACKEND         Storage backend (dual_write for tests)
    RUST_LOG                    Rust logging filter

Failure artifacts are written to: target/test-failures/
EOF
}

main() {
    cd "${REPO_ROOT}"

    # Parse arguments
    local mode="pr"
    local suite=""

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --full)
                mode="full"
                shift
                ;;
            --nightly)
                mode="nightly"
                shift
                ;;
            --help|-h)
                print_usage
                exit 0
                ;;
            unit|integration|gold-standard|e2e-full|determinism|replay|streaming|dual-write|stress)
                suite="$1"
                shift
                ;;
            *)
                log_error "Unknown argument: $1"
                print_usage
                exit 1
                ;;
        esac
    done

    # Print header
    echo ""
    echo "============================================================================="
    echo -e "${BLUE}adapterOS Test Pyramid${NC}"
    echo "============================================================================="
    echo "Mode: ${mode}"
    echo "Repo: ${REPO_ROOT}"
    echo "Time: $(date -Iseconds)"
    echo ""

    # Run tests
    local exit_code=0

    if [[ -n "$suite" ]]; then
        # Run specific suite
        case "$suite" in
            unit)           run_unit_tests || exit_code=1 ;;
            integration)    run_integration_tests || exit_code=1 ;;
            gold-standard)  run_gold_standard_e2e || exit_code=1 ;;
            e2e-full)       run_full_e2e || exit_code=1 ;;
            determinism)    run_determinism_suite || exit_code=1 ;;
            replay)         run_replay_harness || exit_code=1 ;;
            streaming)      run_streaming_tests || exit_code=1 ;;
            dual-write)     run_dual_write_tests || exit_code=1 ;;
            stress)         run_stress_tests || exit_code=1 ;;
        esac
    else
        # Run composite suite
        case "$mode" in
            pr)      run_pr_suite || exit_code=1 ;;
            full)    run_full_suite || exit_code=1 ;;
            nightly) run_nightly_suite || exit_code=1 ;;
        esac
    fi

    # Summary
    echo ""
    echo "============================================================================="
    if [[ $exit_code -eq 0 ]]; then
        log_success "All tests passed!"
    else
        log_error "Some tests failed!"

        # Check for failure artifacts
        if ! check_failures; then
            log_info "Review failure artifacts in: ${FAILURE_DIR}"
        fi
    fi
    echo "============================================================================="

    exit $exit_code
}

main "$@"
