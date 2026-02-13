#!/bin/bash
# Verify worker spawn error/bypass behavior and supervisor crash handling.
#
# This script is intentionally self-contained and deterministic:
# - It runs the supervisor unit tests that cover both:
#   - missing worker binary => actionable error
#   - debug placeholder gate => explicit opt-in via env var
#
# NOTE: placeholder process lifecycle is handled by the Rust unit test itself.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

echo "=== Testing Worker Binary Fix ==="
echo ""

echo "1. Checking compilation..."
cargo check -p adapteros-orchestrator --lib
echo "✓ Compilation successful"
echo ""

echo "2. Running supervisor unit tests (includes spawn error path + placeholder gate)..."
cargo test -p adapteros-orchestrator --lib supervisor -- --nocapture
echo "✓ Supervisor tests passed"
echo ""

echo "=== Fix Summary ==="
echo "✓ Missing worker binary fails fast with clear error message"
echo "✓ Debug-only placeholder requires explicit AOS_WORKER_PLACEHOLDER_OK env var"
echo "✓ Supervisor crash/restart logic is covered without spawning/killing real processes"
