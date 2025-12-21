#!/bin/bash
# Quick test execution script for error recovery integration tests

set -e

echo "=================================="
echo "Error Recovery Integration Tests"
echo "=================================="
echo ""

echo "1. Running basic (non-feature-gated) tests..."
cargo test --test error_recovery_integration \
    circuit_breaker_error_recovery \
    hash_verification_failures \
    error_type_construction \
    error_context_chaining \
    memory_and_resource_errors \
    invalid_manifest_handling \
    concurrent_error_scenarios \
    policy_security_errors \
    database_crypto_errors \
    chaos_integration \
    -- --test-threads=1

echo ""
echo "2. Running Metal GPU tests (macOS only)..."
if [[ "$OSTYPE" == "darwin"* ]]; then
    cargo test --test error_recovery_integration metal_gpu_recovery -- --test-threads=1
else
    echo "   Skipped (not macOS)"
fi

echo ""
echo "3. Running extended tests (requires feature flag)..."
cargo test --test error_recovery_integration \
    --features extended-tests \
    hotswap_quarantine \
    deterministic_executor_recovery \
    resource_leak_detection \
    -- --test-threads=1

echo ""
echo "=================================="
echo "All error recovery tests completed!"
echo "=================================="
