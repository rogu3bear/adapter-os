#!/bin/bash

# adapterOS Performance Benchmark Runner
# This script runs the comprehensive benchmark suite and handles CI/CD integration

set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BENCHMARK_DIR="$PROJECT_ROOT/tests/benchmark"
RESULTS_DIR="${RESULTS_DIR:-benchmark_results}"
BASELINE_FILE="${BASELINE_FILE:-}"
FAIL_ON_REGRESSION="${FAIL_ON_REGRESSION:-true}"
GENERATE_HTML="${GENERATE_HTML:-true}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
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

# Check if we're in the right directory
check_environment() {
    if [[ ! -f "$PROJECT_ROOT/Cargo.toml" ]]; then
        log_error "Not in adapterOS project root. Please run from the project root directory."
        exit 1
    fi

    if [[ ! -d "$BENCHMARK_DIR" ]]; then
        log_error "Benchmark directory not found: $BENCHMARK_DIR"
        exit 1
    fi

    log_info "Environment check passed"
}

# Install dependencies if needed
install_dependencies() {
    log_info "Checking benchmark dependencies..."

    # Check if criterion is available
    if ! cargo check --manifest-path="$BENCHMARK_DIR/Cargo.toml" >/dev/null 2>&1; then
        log_info "Installing benchmark dependencies..."
        cargo build --manifest-path="$BENCHMARK_DIR/Cargo.toml" --release
    fi

    log_info "Dependencies ready"
}

# Run benchmarks using the benchmark runner
run_benchmarks() {
    log_info "Starting benchmark execution..."

    cd "$PROJECT_ROOT"

    # Build benchmark binary
    log_info "Building benchmark suite..."
    cargo build --manifest-path="$BENCHMARK_DIR/Cargo.toml" --release --bin adapteros-benchmarks

    # Prepare command arguments
    CMD_ARGS=(
        "--output-dir" "$RESULTS_DIR"
    )

    if [[ -n "$BASELINE_FILE" ]]; then
        CMD_ARGS+=("--baseline" "$BASELINE_FILE")
    fi

    if [[ "$FAIL_ON_REGRESSION" == "false" ]]; then
        CMD_ARGS+=("--no-fail-on-regression")
    fi

    if [[ "$GENERATE_HTML" == "false" ]]; then
        CMD_ARGS+=("--no-html")
    fi

    # Run benchmarks
    log_info "Running benchmarks with arguments: ${CMD_ARGS[*]}"
    if ! cargo run --manifest-path="$BENCHMARK_DIR/Cargo.toml" --release --bin adapteros-benchmarks -- run "${CMD_ARGS[@]}"; then
        log_error "Benchmark execution failed"
        exit 1
    fi

    log_success "Benchmark execution completed"
}

# Generate CI/CD summary
generate_ci_summary() {
    local results_file="$RESULTS_DIR/benchmark_report.json"

    if [[ ! -f "$results_file" ]]; then
        log_warn "Results file not found: $results_file"
        return
    fi

    log_info "Generating CI/CD summary..."

    # Extract key metrics using jq if available, otherwise use basic parsing
    if command -v jq >/dev/null 2>&1; then
        local total_benchmarks=$(jq '.summary.total_benchmarks' "$results_file")
        local total_time=$(jq '.summary.total_time_seconds' "$results_file")
        local regressions_count=$(jq '.summary.regressions | length' "$results_file")
        local improvements_count=$(jq '.summary.improvements | length' "$results_file")

        # Output GitHub Actions summary if in CI
        if [[ -n "$GITHUB_STEP_SUMMARY" ]]; then
            {
                echo "## adapterOS Benchmark Results"
                echo ""
                echo "| Metric | Value |"
                echo "|--------|-------|"
                echo "| Total Benchmarks | $total_benchmarks |"
                echo "| Total Time | ${total_time}s |"
                echo "| Regressions | $regressions_count |"
                echo "| Improvements | $improvements_count |"
                echo ""
            } >> "$GITHUB_STEP_SUMMARY"
        fi

        # Output to console
        echo "Benchmark Summary:"
        echo "  Total Benchmarks: $total_benchmarks"
        echo "  Total Time: ${total_time}s"
        echo "  Regressions: $regressions_count"
        echo "  Improvements: $improvements_count"

        # Check for regressions
        if [[ "$regressions_count" -gt 0 ]]; then
            log_error "Performance regressions detected!"
            jq -r '.summary.regressions[]' "$results_file" | while read -r regression; do
                log_error "  - $regression"
            done

            if [[ "$FAIL_ON_REGRESSION" == "true" ]]; then
                exit 1
            fi
        else
            log_success "No performance regressions detected"
        fi

    else
        log_warn "jq not available, skipping detailed CI summary"
    fi
}

# Archive results for CI/CD
archive_results() {
    if [[ -d "$RESULTS_DIR" ]]; then
        log_info "Archiving benchmark results..."

        # Create archive
        local archive_name="benchmark_results_$(date +%Y%m%d_%H%M%S).tar.gz"
        tar -czf "$archive_name" "$RESULTS_DIR"

        log_info "Results archived to: $archive_name"

        # Clean up old results if requested
        if [[ "${CLEANUP_RESULTS:-false}" == "true" ]]; then
            rm -rf "$RESULTS_DIR"
            log_info "Cleaned up results directory"
        fi
    fi
}

# Main execution
main() {
    log_info "adapterOS Benchmark Runner Starting"
    log_info "=================================="

    check_environment
    install_dependencies
    run_benchmarks
    generate_ci_summary
    archive_results

    log_success "Benchmark run completed successfully"
    log_info "Results available in: $RESULTS_DIR"
}

# Handle script arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --results-dir=*)
            RESULTS_DIR="${1#*=}"
            shift
            ;;
        --baseline=*)
            BASELINE_FILE="${1#*=}"
            shift
            ;;
        --no-fail-on-regression)
            FAIL_ON_REGRESSION="false"
            shift
            ;;
        --no-html)
            GENERATE_HTML="false"
            shift
            ;;
        --cleanup)
            CLEANUP_RESULTS="true"
            shift
            ;;
        --help)
            echo "adapterOS Benchmark Runner"
            echo ""
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --results-dir=DIR        Output directory for results (default: benchmark_results)"
            echo "  --baseline=FILE          Baseline results file for comparison"
            echo "  --no-fail-on-regression  Don't fail CI on performance regressions"
            echo "  --no-html                Skip HTML report generation"
            echo "  --cleanup                Remove results directory after archiving"
            echo "  --help                   Show this help message"
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

main "$@"