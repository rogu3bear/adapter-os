#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
./scripts/ci/check_openapi_drift.sh
./scripts/ci/build_leptos_wasm.sh
./scripts/check_inference_bypass.sh
make stability-check
