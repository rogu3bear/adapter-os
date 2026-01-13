#!/usr/bin/env bash
#
# Script to run replay_determinism_tests with proper setup
#
# Usage: ./run_replay_determinism_tests.sh

set -euo pipefail

echo "Running replay_determinism_tests..."
echo "This may take several minutes for first run after clean build"
echo ""

# Set environment variables
export AOS_SKIP_MIGRATION_SIGNATURES=1
export RUST_BACKTRACE=1

# Run the tests
cargo test --package adapteros-server-api --test replay_determinism_tests -- --test-threads=1 "$@"
