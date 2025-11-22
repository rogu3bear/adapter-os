#!/bin/bash
#
# Real MLX Integration Testing Script
# Comprehensive guide to running MLX backend tests
#
# Usage:
#   ./RUN_REAL_MLX_TESTS.sh                 # Run all tests
#   ./RUN_REAL_MLX_TESTS.sh verify          # Verify installation only
#   ./RUN_REAL_MLX_TESTS.sh memory          # Run memory tests
#   ./RUN_REAL_MLX_TESTS.sh forward         # Run forward pass tests
#   ./RUN_REAL_MLX_TESTS.sh seeding         # Run seeding tests
#   ./RUN_REAL_MLX_TESTS.sh health          # Run health tests
#   ./RUN_REAL_MLX_TESTS.sh debug           # Run with debug logging

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
CRATE_PATH="crates/adapteros-lora-mlx-ffi"
TEST_NAME="real_mlx_integration"
MLX_PATHS=(
    "/opt/homebrew/opt/mlx"
    "/usr/local/opt/mlx"
    "$MLX_PATH"
)

# Helper functions
print_header() {
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}========================================${NC}"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ $1${NC}"
}

# Check MLX installation
check_mlx_installation() {
    print_header "Checking MLX Installation"

    local mlx_found=0
    for path in "${MLX_PATHS[@]}"; do
        if [ -n "$path" ] && [ -d "$path" ]; then
            if [ -d "$path/include/mlx" ] && [ -d "$path/lib" ]; then
                print_success "Found MLX at: $path"

                # Check for library files
                if ls "$path/lib"/libmlx.* &> /dev/null; then
                    print_success "MLX library files found"
                fi

                # Try to detect version
                if [ -f "$path/include/mlx/version.h" ]; then
                    VERSION=$(grep "MLX_VERSION" "$path/include/mlx/version.h" | head -1 || echo "unknown")
                    print_info "Version: $VERSION"
                fi

                mlx_found=1
                break
            fi
        fi
    done

    if [ $mlx_found -eq 0 ]; then
        print_warning "MLX not found in standard locations"
        print_info "To install MLX:"
        echo "  brew install mlx"
        echo ""
        print_info "Or set MLX_PATH:"
        echo "  export MLX_PATH=/path/to/mlx"
    fi

    echo ""
    return $mlx_found
}

# Run test group
run_test_group() {
    local group=$1
    local description=$2
    local features=$3

    print_header "Running: $description"

    local cmd="cargo test -p $CRATE_PATH --test $TEST_NAME"

    if [ -n "$features" ]; then
        cmd="$cmd --features $features"
    fi

    if [ -n "$group" ]; then
        cmd="$cmd $group"
    fi

    cmd="$cmd -- --nocapture"

    print_info "Command: $cmd"
    echo ""

    if eval "$cmd"; then
        print_success "Test group completed successfully"
    else
        print_error "Test group failed"
        return 1
    fi

    echo ""
}

# Test MLX installation only
test_verify() {
    print_header "Verifying MLX Installation"

    cargo test -p $CRATE_PATH --test $TEST_NAME \
        model_loading::test_mlx_is_installed \
        -- --nocapture

    print_success "Verification complete"
}

# Test memory tracking
test_memory() {
    print_header "Testing Memory Tracking"

    cargo test -p $CRATE_PATH --test $TEST_NAME \
        memory_tracking \
        -- --nocapture --test-threads=1

    print_success "Memory tests complete"
}

# Test forward pass
test_forward() {
    print_header "Testing Forward Pass"

    cargo test -p $CRATE_PATH --test $TEST_NAME \
        forward_pass \
        -- --nocapture

    print_success "Forward pass tests complete"
}

# Test seeding
test_seeding() {
    print_header "Testing Deterministic Seeding"

    cargo test -p $CRATE_PATH --test $TEST_NAME \
        deterministic_seeding \
        -- --nocapture

    print_success "Seeding tests complete"
}

# Test health/resilience
test_health() {
    print_header "Testing Health & Resilience"

    cargo test -p $CRATE_PATH --test $TEST_NAME \
        health_and_resilience \
        -- --nocapture

    print_success "Health tests complete"
}

# Run all tests with debug output
test_debug() {
    print_header "Running All Tests with Debug Logging"

    RUST_LOG=debug cargo test -p $CRATE_PATH --test $TEST_NAME \
        -- --nocapture --test-threads=1

    print_success "Debug test run complete"
}

# Run all tests
test_all() {
    print_header "Running All Real MLX Integration Tests"

    local commands=(
        "cargo test -p $CRATE_PATH --test $TEST_NAME -- --nocapture"
    )

    for cmd in "${commands[@]}"; do
        print_info "Running: $cmd"
        if eval "$cmd"; then
            print_success "Tests passed"
        else
            print_error "Tests failed"
            exit 1
        fi
    done

    print_success "All tests completed successfully"
}

# Build test suite (no-run to check compilation)
test_build() {
    print_header "Building Test Suite"

    print_info "Building without feature flags..."
    if cargo build -p $CRATE_PATH --tests; then
        print_success "Stub mode compilation successful"
    else
        print_error "Stub mode compilation failed"
        exit 1
    fi

    print_info "Building with real-mlx feature..."
    if cargo build -p $CRATE_PATH --tests --features real-mlx; then
        print_success "Real MLX compilation successful"
    else
        print_warning "Real MLX compilation failed (MLX may not be installed)"
    fi

    echo ""
}

# Show usage information
show_usage() {
    cat << EOF
${BLUE}Real MLX Integration Testing Script${NC}

${GREEN}Usage:${NC}
  $0 [COMMAND]

${GREEN}Commands:${NC}
  verify      - Verify MLX installation
  build       - Build test suite (stub + real-mlx)
  memory      - Run memory tracking tests
  forward     - Run forward pass tests
  seeding     - Run deterministic seeding tests
  health      - Run health & resilience tests
  debug       - Run all tests with debug logging
  all         - Run all tests (default)
  help        - Show this help message

${GREEN}Examples:${NC}
  # Check if MLX is installed
  $0 verify

  # Run specific test group
  $0 memory

  # Run all tests with output
  $0 all

  # Debug with detailed logging
  $0 debug

${GREEN}Environment Variables:${NC}
  MLX_PATH              - Path to MLX installation
  MLX_FORCE_STUB        - Force stub implementation (set to 1)
  RUST_LOG              - Set logging level (debug, info, warn, error)
  RUST_BACKTRACE        - Set backtrace level (0, 1, full)

${GREEN}Requirements:${NC}
  - Rust (stable or nightly)
  - Cargo
  - MLX library (optional, for real-mlx feature)
    Install: brew install mlx

${GREEN}Output:${NC}
  Memory tests:
    Current memory usage: 256.50 MB
    Active allocations: 1024

  Seeding tests:
    MLX backend seeded for deterministic dropout/sampling

  Health tests:
    Circuit breaker state: Closed
    Operational: true

${BLUE}For detailed documentation, see:${NC}
  REAL_MLX_INTEGRATION_TESTING.md
  MLX_REAL_INTEGRATION_SUMMARY.md

EOF
}

# Main script logic
main() {
    local command="${1:-all}"

    case "$command" in
        verify)
            check_mlx_installation
            test_verify
            ;;
        build)
            test_build
            ;;
        memory)
            test_memory
            ;;
        forward)
            test_forward
            ;;
        seeding)
            test_seeding
            ;;
        health)
            test_health
            ;;
        debug)
            check_mlx_installation || true
            test_debug
            ;;
        all)
            check_mlx_installation || true
            test_all
            ;;
        help|--help|-h)
            show_usage
            ;;
        *)
            print_error "Unknown command: $command"
            echo ""
            show_usage
            exit 1
            ;;
    esac

    echo ""
    print_header "Test Run Summary"
    print_success "Test script execution completed"
}

# Run main function
main "$@"
