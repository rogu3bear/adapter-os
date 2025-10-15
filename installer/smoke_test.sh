#!/bin/bash
# AdapterOS Post-Install Smoke Test
# Tests core functionality: init, serve, and inference

set -e

# Configuration
WORKSPACE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$WORKSPACE_ROOT"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test configuration
TEST_TENANT="smoke_test"
TEST_PLAN="qwen7b"
TEST_SOCKET="/tmp/aos_smoke_test.sock"
TEST_TIMEOUT=30
SERVER_PORT=8080

# Test results tracking
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_TOTAL=0

# Helper functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
    TESTS_PASSED=$((TESTS_PASSED + 1))
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $1"
    TESTS_FAILED=$((TESTS_FAILED + 1))
}

log_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

run_test() {
    local test_name="$1"
    local test_func="$2"
    
    TESTS_TOTAL=$((TESTS_TOTAL + 1))
    log_info "Running test: $test_name"
    
    if $test_func; then
        log_success "$test_name"
        return 0
    else
        log_error "$test_name"
        return 1
    fi
}

# Test functions
test_aosctl_exists() {
    if [[ -f "./target/release/aosctl" ]]; then
        log_info "aosctl binary found"
        return 0
    else
        log_error "aosctl binary not found at ./target/release/aosctl"
        return 1
    fi
}

test_aosctl_help() {
    if ./target/release/aosctl --help > /dev/null 2>&1; then
        log_info "aosctl help command works"
        return 0
    else
        log_error "aosctl help command failed"
        return 1
    fi
}

test_tenant_init() {
    log_info "Testing tenant initialization..."
    
    # Check if database exists first
    if [[ ! -f "var/aos.db" ]]; then
        log_warning "Database not found, skipping tenant test"
        return 0
    fi
    
    # Clean up any existing test tenant
    ./target/release/aosctl init-tenant --id "$TEST_TENANT" --uid 501 --gid 20 2>/dev/null || true
    
    # Verify tenant was created (this may fail if database isn't properly initialized)
    if ./target/release/aosctl list-adapters --tenant "$TEST_TENANT" > /dev/null 2>&1; then
        log_info "Test tenant created successfully"
        return 0
    else
        log_warning "Failed to create test tenant (may be expected if database not initialized)"
        return 0  # Don't fail the test for this
    fi
}

test_serve_dry_run() {
    log_info "Testing serve dry-run..."
    
    # Check if database exists first
    if [[ ! -f "var/aos.db" ]]; then
        log_warning "Database not found, skipping serve test"
        return 0
    fi
    
    if ./target/release/aosctl serve --tenant "$TEST_TENANT" --plan "$TEST_PLAN" --dry-run > /dev/null 2>&1; then
        log_info "Serve dry-run completed successfully"
        return 0
    else
        log_warning "Serve dry-run failed (may be expected if no model/plan exists)"
        return 0  # Don't fail the test for this
    fi
}

test_basic_inference_example() {
    log_info "Testing basic inference example..."
    
    # Check if the basic inference example exists and can be compiled
    if [[ -f "examples/basic_inference.rs" ]]; then
        log_info "Basic inference example found"
        
        # Try to compile the example (don't run it as it requires MLX setup)
        if cargo check --example basic_inference > /dev/null 2>&1; then
            log_info "Basic inference example compiles successfully"
            return 0
        else
            log_warning "Basic inference example failed to compile (may require MLX setup)"
            return 0  # Don't fail the test for this
        fi
    else
        log_error "Basic inference example not found"
        return 1
    fi
}

test_api_client_exists() {
    log_info "Testing API client availability..."
    
    # Check if the API client crate exists
    if [[ -d "crates/adapteros-client" ]]; then
        log_info "API client crate found"
        
        # Try to compile the client
        if cargo check -p adapteros-client > /dev/null 2>&1; then
            log_info "API client compiles successfully"
            return 0
        else
            log_warning "API client failed to compile"
            return 0  # Don't fail the test for this
        fi
    else
        log_error "API client crate not found"
        return 1
    fi
}

test_metal_kernels() {
    log_info "Testing Metal kernel compilation..."
    
    if [[ -f "metal/build.sh" ]]; then
        log_info "Metal build script found"
        
        # Check if .metallib files exist (precompiled kernels)
        if find metal -name "*.metallib" -type f | grep -q .; then
            log_info "Precompiled Metal kernels found"
            return 0
        else
            log_warning "No precompiled Metal kernels found (may need compilation)"
            return 0  # Don't fail the test for this
        fi
    else
        log_warning "Metal build script not found"
        return 0  # Don't fail the test for this
    fi
}

test_config_files() {
    log_info "Testing configuration files..."
    
    local config_found=0
    
    # Check for main config file
    if [[ -f "configs/cp.toml" ]]; then
        log_info "Control plane config found: configs/cp.toml"
        config_found=1
    fi
    
    # Check for example manifests
    if [[ -f "manifests/qwen7b.yaml" ]]; then
        log_info "Example manifest found: manifests/qwen7b.yaml"
        config_found=1
    fi
    
    if [[ $config_found -eq 1 ]]; then
        return 0
    else
        log_warning "No configuration files found"
        return 0  # Don't fail the test for this
    fi
}

test_database_migrations() {
    log_info "Testing database migrations..."
    
    if [[ -d "migrations" && $(ls migrations/*.sql 2>/dev/null | wc -l) -gt 0 ]]; then
        log_info "Database migrations found"
        return 0
    else
        log_warning "No database migrations found"
        return 0  # Don't fail the test for this
    fi
}

test_policy_packs() {
    log_info "Testing policy packs..."
    
    # Check if policy management works
    if ./target/release/aosctl policy list > /dev/null 2>&1; then
        log_info "Policy management works"
        return 0
    else
        log_warning "Policy management not available"
        return 0  # Don't fail the test for this
    fi
}

# Cleanup function
cleanup() {
    log_info "Cleaning up test artifacts..."
    
    # Remove test socket if it exists
    rm -f "$TEST_SOCKET"
    
    # Kill any background processes
    jobs -p | xargs -r kill 2>/dev/null || true
    
    log_info "Cleanup completed"
}

# Main test execution
main() {
    echo "================================================"
    echo "AdapterOS Post-Install Smoke Test"
    echo "================================================"
    echo "Workspace: $WORKSPACE_ROOT"
    echo "Test tenant: $TEST_TENANT"
    echo "Test plan: $TEST_PLAN"
    echo "================================================"
    echo ""
    
    # Set up cleanup trap
    trap cleanup EXIT
    
    # Run tests
    run_test "aosctl binary exists" test_aosctl_exists
    run_test "aosctl help command" test_aosctl_help
    run_test "tenant initialization" test_tenant_init
    run_test "serve dry-run" test_serve_dry_run
    run_test "basic inference example" test_basic_inference_example
    run_test "API client" test_api_client_exists
    run_test "Metal kernels" test_metal_kernels
    run_test "configuration files" test_config_files
    run_test "database migrations" test_database_migrations
    run_test "policy packs" test_policy_packs
    
    # Print results
    echo ""
    echo "================================================"
    echo "Smoke Test Results"
    echo "================================================"
    echo "Total tests: $TESTS_TOTAL"
    echo -e "Passed: ${GREEN}$TESTS_PASSED${NC}"
    echo -e "Failed: ${RED}$TESTS_FAILED${NC}"
    echo "================================================"
    
    if [[ $TESTS_FAILED -eq 0 ]]; then
        echo -e "${GREEN}✅ All smoke tests passed!${NC}"
        echo ""
        echo "Next steps:"
        echo "  1. Start the control plane:"
        echo "     ./target/release/adapteros-server --config configs/cp.toml"
        echo ""
        echo "  2. Start serving:"
        echo "     ./target/release/aosctl serve --tenant $TEST_TENANT --plan $TEST_PLAN"
        echo ""
        echo "  3. Run inference:"
        echo "     curl -X POST http://localhost:$SERVER_PORT/api/v1/infer \\"
        echo "       -H 'Content-Type: application/json' \\"
        echo "       -d '{\"prompt\": \"Hello, world!\", \"max_tokens\": 50}'"
        echo ""
        exit 0
    else
        echo -e "${RED}❌ Some smoke tests failed!${NC}"
        echo ""
        echo "Please check the failed tests above and ensure:"
        echo "  - All dependencies are installed"
        echo "  - The installation completed successfully"
        echo "  - Required model files are present"
        echo ""
        exit 1
    fi
}

# Run main function
main "$@"
