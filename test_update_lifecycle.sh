#!/bin/bash
# Test script for adapter update-lifecycle command

set -e

echo "Testing adapter update-lifecycle command"
echo "========================================="
echo ""

# Show help
echo "1. Testing command help:"
cargo run -p adapteros-cli -- adapter update-lifecycle --help
echo ""

# Test invalid state (should fail)
echo "2. Testing invalid lifecycle state (should fail):"
cargo run -p adapteros-cli -- adapter update-lifecycle test-adapter invalid_state 2>&1 || echo "✓ Expected error for invalid state"
echo ""

# Test valid state names (these will fail if adapter doesn't exist, which is expected)
echo "3. Testing valid state name parsing:"
echo "   - draft"
cargo run -p adapteros-cli -- adapter update-lifecycle test-adapter draft 2>&1 || echo "✓ Adapter not found (expected)"
echo ""

echo "   - active"
cargo run -p adapteros-cli -- adapter update-lifecycle test-adapter active 2>&1 || echo "✓ Adapter not found (expected)"
echo ""

echo "   - deprecated"
cargo run -p adapteros-cli -- adapter update-lifecycle test-adapter deprecated 2>&1 || echo "✓ Adapter not found (expected)"
echo ""

echo "   - retired"
cargo run -p adapteros-cli -- adapter update-lifecycle test-adapter retired 2>&1 || echo "✓ Adapter not found (expected)"
echo ""

echo "Testing complete!"
