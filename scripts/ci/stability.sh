#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
./scripts/ci/check_openapi_drift.sh
./scripts/ci/build_leptos_wasm.sh
./scripts/check_inference_bypass.sh
bash scripts/test/all.sh all
cargo test --test determinism_core_suite -- --test-threads=8
cargo test -p adapteros-lora-router --test determinism
bash scripts/check_fast_math_flags.sh
