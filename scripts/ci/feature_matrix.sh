#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

PROFILE="${PROFILE:-debug}"
CARGO_ARGS=(--workspace --no-run --locked)

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

run "Default features (Cargo.toml defaults)" cargo test "${CARGO_ARGS[@]}"
run "All features (drift check)" cargo test "${CARGO_ARGS[@]}" --all-features
