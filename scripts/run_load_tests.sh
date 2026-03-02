#!/usr/bin/env bash
# adapterOS Load Test Runner
#
# Runs comprehensive load tests for concurrent adapter operations
# with configurable parameters and generates performance reports.

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default configuration
DEFAULT_TEST_URL="http://localhost:9443"
DEFAULT_REQUESTS=100
DEFAULT_CONCURRENCY=20
LOG_DIR="var/load_test_results"

# Parse command line arguments
PROFILE="default"
REQUESTS=${AOS_STRESS_REQUESTS:-$DEFAULT_REQUESTS}
CONCURRENCY=${AOS_STRESS_CONCURRENCY:-$DEFAULT_CONCURRENCY}
TEST_URL=${MPLORA_TEST_URL:-$DEFAULT_TEST_URL}
RUN_ALL=false

usage() {
    cat << EOF
Usage: $0 [OPTIONS] [TEST_NAME]

Run adapterOS load tests for concurrent adapter operations.

OPTIONS:
    -h, --help              Show this help message
    -a, --all               Run all load tests
    -p, --profile PROFILE   Load test profile (light|medium|heavy|extreme)
    -r, --requests N        Number of requests (default: $DEFAULT_REQUESTS)
    -c, --concurrency N     Concurrency level (default: $DEFAULT_CONCURRENCY)
    -u, --url URL           adapterOS server URL (default: $DEFAULT_TEST_URL)
    -l, --log-dir DIR       Log directory (default: $LOG_DIR)
    --no-baseline           Skip baseline comparison

TEST NAMES:
    concurrent_inference    - 100+ concurrent inference requests
    hotswap                - Concurrent adapter hot-swaps
    lifecycle              - Adapter load/unload under request load
    stress                 - Configurable stress test
    all                    - Run all tests

PROFILES:
    light    - Low load:      50 requests,  10 concurrency
    medium   - Medium load:   200 requests, 25 concurrency
    heavy    - High load:     500 requests, 50 concurrency
    extreme  - Extreme load:  1000 requests, 100 concurrency

EXAMPLES:
    # Run all tests with default settings
    $0 --all

    # Run concurrent inference test with medium profile
    $0 --profile medium concurrent_inference

    # Run stress test with custom parameters
    $0 --requests 1000 --concurrency 100 stress

    # Run all tests against remote server
    $0 --url https://aos.example.com --all

EOF
    exit 0
}

# Print colored message
print_message() {
    local color=$1
    shift
    echo -e "${color}$@${NC}"
}

# Print section header
print_header() {
    echo ""
    echo "======================================================================"
    print_message "$BLUE" "$@"
    echo "======================================================================"
    echo ""
}

# Apply profile settings
apply_profile() {
    case "$PROFILE" in
        light)
            REQUESTS=50
            CONCURRENCY=10
            ;;
        medium)
            REQUESTS=200
            CONCURRENCY=25
            ;;
        heavy)
            REQUESTS=500
            CONCURRENCY=50
            ;;
        extreme)
            REQUESTS=1000
            CONCURRENCY=100
            ;;
        default)
            # Use current settings
            ;;
        *)
            print_message "$RED" "Unknown profile: $PROFILE"
            print_message "$YELLOW" "Valid profiles: light, medium, heavy, extreme"
            exit 1
            ;;
    esac
}

# Setup logging
setup_logging() {
    mkdir -p "$LOG_DIR"
    local timestamp=$(date +"%Y%m%d_%H%M%S")
    LOG_FILE="$LOG_DIR/load_test_${timestamp}.log"
    RESULT_FILE="$LOG_DIR/results_${timestamp}.txt"

    print_message "$GREEN" "Log directory: $LOG_DIR"
    print_message "$GREEN" "Log file: $LOG_FILE"
    print_message "$GREEN" "Results file: $RESULT_FILE"
}

# Check prerequisites
check_prerequisites() {
    print_header "Checking Prerequisites"

    # Check if cargo is available
    if ! command -v cargo &> /dev/null; then
        print_message "$RED" "Error: cargo not found. Please install Rust."
        exit 1
    fi

    # Check if server is reachable
    if ! curl -s -f -o /dev/null "$TEST_URL/health" 2>/dev/null; then
        print_message "$YELLOW" "Warning: Cannot reach adapterOS server at $TEST_URL"
        print_message "$YELLOW" "Some tests may fail if the server is not running."
        read -p "Continue anyway? (y/N) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    else
        print_message "$GREEN" "Server is reachable at $TEST_URL"
    fi
}

# Print test configuration
print_config() {
    print_header "Load Test Configuration"
    echo "Profile:         $PROFILE"
    echo "Requests:        $REQUESTS"
    echo "Concurrency:     $CONCURRENCY"
    echo "Server URL:      $TEST_URL"
    echo "Log Directory:   $LOG_DIR"
    echo ""
}

# Run a single test
run_test() {
    local test_name=$1
    local test_function=$2

    print_header "Running Test: $test_name"

    export AOS_STRESS_REQUESTS=$REQUESTS
    export AOS_STRESS_CONCURRENCY=$CONCURRENCY
    export MPLORA_TEST_URL=$TEST_URL

    local start_time=$(date +%s)

    if cargo test --test integration --features extended-tests \
        "$test_function" -- --nocapture 2>&1 | tee -a "$LOG_FILE"; then
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))

        print_message "$GREEN" "✓ $test_name PASSED (${duration}s)"
        echo "$test_name,PASSED,$duration" >> "$RESULT_FILE"
        return 0
    else
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))

        print_message "$RED" "✗ $test_name FAILED (${duration}s)"
        echo "$test_name,FAILED,$duration" >> "$RESULT_FILE"
        return 1
    fi
}

# Run all tests
run_all_tests() {
    local failed_tests=0

    run_test "High Concurrent Inference Load" "test_high_concurrent_inference_load" || ((failed_tests++))
    run_test "Concurrent Adapter Hot-Swaps" "test_concurrent_adapter_hotswap" || ((failed_tests++))
    run_test "Adapter Lifecycle Under Load" "test_adapter_lifecycle_under_load" || ((failed_tests++))
    run_test "Configurable Stress Test" "test_configurable_stress_test" || ((failed_tests++))

    return $failed_tests
}

# Generate summary report
generate_report() {
    local failed_tests=$1

    print_header "Load Test Summary"

    if [ -f "$RESULT_FILE" ]; then
        echo "Test Results:"
        echo "----------------------------------------"
        while IFS=',' read -r test_name status duration; do
            if [ "$status" = "PASSED" ]; then
                print_message "$GREEN" "✓ $test_name ($duration seconds)"
            else
                print_message "$RED" "✗ $test_name ($duration seconds)"
            fi
        done < "$RESULT_FILE"
        echo "----------------------------------------"

        local total_tests=$(wc -l < "$RESULT_FILE")
        local passed_tests=$((total_tests - failed_tests))

        echo ""
        echo "Total Tests:  $total_tests"
        print_message "$GREEN" "Passed:       $passed_tests"
        if [ $failed_tests -gt 0 ]; then
            print_message "$RED" "Failed:       $failed_tests"
        else
            echo "Failed:       0"
        fi

        local success_rate=$((passed_tests * 100 / total_tests))
        echo "Success Rate: ${success_rate}%"
        echo ""

        print_message "$BLUE" "Full logs: $LOG_FILE"
        print_message "$BLUE" "Results:   $RESULT_FILE"
    fi

    if [ $failed_tests -eq 0 ]; then
        print_message "$GREEN" "All tests passed!"
        return 0
    else
        print_message "$RED" "$failed_tests test(s) failed."
        return 1
    fi
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            usage
            ;;
        -a|--all)
            RUN_ALL=true
            shift
            ;;
        -p|--profile)
            PROFILE="$2"
            shift 2
            ;;
        -r|--requests)
            REQUESTS="$2"
            shift 2
            ;;
        -c|--concurrency)
            CONCURRENCY="$2"
            shift 2
            ;;
        -u|--url)
            TEST_URL="$2"
            shift 2
            ;;
        -l|--log-dir)
            LOG_DIR="$2"
            shift 2
            ;;
        *)
            TEST_NAME="$1"
            shift
            ;;
    esac
done

# Main execution
main() {
    print_header "adapterOS Load Test Runner"

    apply_profile
    setup_logging
    check_prerequisites
    print_config

    local failed_tests=0

    if [ "$RUN_ALL" = true ]; then
        run_all_tests || failed_tests=$?
    elif [ -n "$TEST_NAME" ]; then
        case "$TEST_NAME" in
            concurrent_inference)
                run_test "High Concurrent Inference Load" "test_high_concurrent_inference_load" || ((failed_tests++))
                ;;
            hotswap)
                run_test "Concurrent Adapter Hot-Swaps" "test_concurrent_adapter_hotswap" || ((failed_tests++))
                ;;
            lifecycle)
                run_test "Adapter Lifecycle Under Load" "test_adapter_lifecycle_under_load" || ((failed_tests++))
                ;;
            stress)
                run_test "Configurable Stress Test" "test_configurable_stress_test" || ((failed_tests++))
                ;;
            all)
                run_all_tests || failed_tests=$?
                ;;
            *)
                print_message "$RED" "Unknown test: $TEST_NAME"
                print_message "$YELLOW" "Run '$0 --help' for usage information"
                exit 1
                ;;
        esac
    else
        print_message "$YELLOW" "No test specified. Use --all to run all tests or specify a test name."
        print_message "$YELLOW" "Run '$0 --help' for usage information"
        exit 1
    fi

    generate_report $failed_tests
}

main
