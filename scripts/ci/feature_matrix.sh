#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

PROFILE="${PROFILE:-debug}"
CARGO_ARGS=(--workspace --no-run --locked)
CARGO_CMD=(cargo)

if command -v rustup >/dev/null 2>&1; then
  if rustup run nightly cargo --version >/dev/null 2>&1; then
    CARGO_CMD=(rustup run nightly cargo)
  fi
fi

if [[ "$PROFILE" == "release" ]]; then
  CARGO_ARGS+=(--release)
fi

run() {
  local desc="$1"
  shift
  local cmd=("$@")
  echo ""
  echo "-> ${desc}"
  printf "   "
  printf "%q " "${cmd[@]}"
  echo ""
  "${cmd[@]}"
}

run "Default features (Cargo.toml defaults)" "${CARGO_CMD[@]}" test "${CARGO_ARGS[@]}"
run "All features (drift check)" "${CARGO_CMD[@]}" test "${CARGO_ARGS[@]}" --all-features
