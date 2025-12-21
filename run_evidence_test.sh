#!/bin/bash
# Script to run evidence_envelopes_tests with clean environment

# Kill any existing cargo processes
killall -9 cargo rustc 2>/dev/null

# Wait a bit
sleep 3

# Remove lock files
rm -f ~/.cargo/.package-cache* 2>/dev/null
rm -f /Users/mln-dev/Dev/adapter-os/target/debug/.cargo-lock 2>/dev/null

# Wait again
sleep 2

# Run the test
cd /Users/mln-dev/Dev/adapter-os
cargo test --package adapteros-db --test evidence_envelopes_tests --no-fail-fast -- --test-threads=1 2>&1
