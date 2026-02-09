#!/bin/bash
# Script to run evidence_envelopes_tests with clean environment

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Kill any existing cargo processes
killall -9 cargo rustc 2>/dev/null

# Wait a bit
sleep 3

# Remove lock files
rm -f ~/.cargo/.package-cache* 2>/dev/null
rm -f "$REPO_ROOT/target/debug/.cargo-lock" 2>/dev/null

# Wait again
sleep 2

# Run the test
cd "$REPO_ROOT"
cargo test --package adapteros-db --test evidence_envelopes_tests --no-fail-fast -- --test-threads=1 2>&1
