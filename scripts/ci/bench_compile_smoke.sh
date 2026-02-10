#!/usr/bin/env bash
# =============================================================================
# Benchmark Compile Smoke Test
# =============================================================================
#
# Verifies that all benchmark targets compile without running them.
# This catches compile-time regressions in bench code that would otherwise
# only surface during expensive benchmark runs.
#
# Usage: ./scripts/ci/bench_compile_smoke.sh
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

echo "=== Benchmark Compile Smoke Test ==="

FAILED=0

# Crates with benchmark targets (excluding MLX FFI which needs macOS + mlx)
BENCH_CRATES=(
  "adapteros-core"
  "adapteros-db"
  "adapteros-telemetry"
  "adapteros-lora-worker"
  "adapteros-model-server"
)

for crate in "${BENCH_CRATES[@]}"; do
  echo "  Checking benchmarks: $crate"
  if ! cargo check -p "$crate" --benches 2>&1; then
    echo "::error::Benchmark compile failed for $crate"
    FAILED=1
  fi
done

# Metal kernel benchmarks only on macOS
if [[ "$(uname -s)" == "Darwin" ]]; then
  echo "  Checking benchmarks: adapteros-lora-kernel-mtl"
  if ! cargo check -p adapteros-lora-kernel-mtl --benches 2>&1; then
    echo "::error::Benchmark compile failed for adapteros-lora-kernel-mtl"
    FAILED=1
  fi

  echo "  Checking benchmarks: adapteros-lora-mlx-ffi"
  if ! cargo check -p adapteros-lora-mlx-ffi --benches 2>&1; then
    echo "::warning::Benchmark compile failed for adapteros-lora-mlx-ffi (may need mlx)"
    # Non-fatal on macOS; MLX may not be installed
  fi
else
  echo "  Skipping macOS-only benchmarks (adapteros-lora-kernel-mtl, adapteros-lora-mlx-ffi)"
fi

if [[ $FAILED -ne 0 ]]; then
  echo ""
  echo "FAIL: One or more benchmark targets failed to compile"
  exit 1
fi

echo "=== Benchmark Compile Smoke: PASSED ==="
