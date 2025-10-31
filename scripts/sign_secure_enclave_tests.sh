#!/bin/bash
# Script to codesign Secure Enclave test binaries with required entitlements

set -e

echo "🔐 Setting up codesigning for Secure Enclave tests..."

# Check if we're on macOS
if [[ "$OSTYPE" != "darwin"* ]]; then
    echo "❌ This script only runs on macOS"
    exit 1
fi

# Find the test binary
TEST_BINARY="target/debug/deps/secure_enclave_integration-*"
if ! ls $TEST_BINARY 1> /dev/null 2>&1; then
    echo "❌ Test binary not found. Run tests first:"
    echo "cargo test -p adapteros-secd --features secure-enclave --no-run"
    exit 1
fi

# Get the actual test binary path
TEST_BINARY_PATH=$(ls $TEST_BINARY | head -1)

echo "📍 Found test binary: $TEST_BINARY_PATH"

# Check if entitlements file exists
ENTITLEMENTS_FILE="crates/adapteros-secd/tests/secure_enclave_tests.entitlements"
if [ ! -f "$ENTITLEMENTS_FILE" ]; then
    echo "❌ Entitlements file not found: $ENTITLEMENTS_FILE"
    exit 1
fi

# Check if codesign is available
if ! command -v codesign &> /dev/null; then
    echo "❌ codesign command not found"
    exit 1
fi

# Get available signing identities
echo "🔍 Checking available signing identities..."
IDENTITIES=$(security find-identity -p codesigning -v 2>/dev/null)

# Prefer Apple Development certificate for Secure Enclave access
APPLE_DEV_IDENTITY=$(echo "$IDENTITIES" | grep "Apple Development" | head -1)
DEVELOPER_ID_IDENTITY=$(echo "$IDENTITIES" | grep "Developer ID" | head -1)
MAC_DEVELOPER_IDENTITY=$(echo "$IDENTITIES" | grep "Mac Developer" | head -1)

if [ -n "$APPLE_DEV_IDENTITY" ]; then
    SIGNING_IDENTITY=$(echo "$APPLE_DEV_IDENTITY" | awk -F'"' '{print $2}')
    echo "✅ Found Apple Development identity: $SIGNING_IDENTITY"
elif [ -n "$DEVELOPER_ID_IDENTITY" ]; then
    SIGNING_IDENTITY=$(echo "$DEVELOPER_ID_IDENTITY" | awk -F'"' '{print $2}')
    echo "✅ Found Developer ID identity: $SIGNING_IDENTITY"
elif [ -n "$MAC_DEVELOPER_IDENTITY" ]; then
    SIGNING_IDENTITY=$(echo "$MAC_DEVELOPER_IDENTITY" | awk -F'"' '{print $2}')
    echo "✅ Found Mac Developer identity: $SIGNING_IDENTITY"
else
    echo "⚠️ No suitable signing identity found. Using ad-hoc signing (may not work for Secure Enclave)."
    SIGNING_IDENTITY="-"
fi

# Codesign the test binary with entitlements
echo "🔐 Codesigning test binary with Secure Enclave entitlements..."
codesign --force --sign "$SIGNING_IDENTITY" \
    --entitlements "$ENTITLEMENTS_FILE" \
    --options runtime \
    "$TEST_BINARY_PATH"

# Verify the signature
echo "🔍 Verifying signature..."
codesign --verify --verbose=4 "$TEST_BINARY_PATH"

# Check entitlements
echo "🔍 Checking entitlements..."
codesign --display --entitlements - "$TEST_BINARY_PATH" | grep -A 20 "Entitlements"

echo "✅ Secure Enclave test binary signed successfully!"
echo "   Binary: $TEST_BINARY_PATH"
echo "   Identity: $SIGNING_IDENTITY"
echo "   Entitlements: $ENTITLEMENTS_FILE"

# Run the tests
echo "🧪 Running Secure Enclave tests..."
cargo test -p adapteros-secd --features secure-enclave secure_enclave
