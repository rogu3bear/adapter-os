#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
START_FILE="$ROOT_DIR/start"
SM_FILE="$ROOT_DIR/scripts/service-manager.sh"
RELEASE_GATE_FILE="$ROOT_DIR/scripts/ci/local_release_gate.sh"

require_match() {
  local pattern="$1"
  local file="$2"
  local msg="$3"
  if ! rg -q -- "$pattern" "$file"; then
    echo "FAIL: $msg"
    echo "  pattern: $pattern"
    echo "  file: $file"
    exit 1
  fi
}

for cmd in "up, start" "down, stop" "status" "backend" "worker" "secd" "node" "preflight"; do
  require_match "$cmd" "$START_FILE" "Missing ./start command in help surface: $cmd"
done

require_match "UI: Backend serves Leptos WASM from static/" "$START_FILE" "start help must document backend-served UI"
require_match "trunk serve" "$START_FILE" "start help must document trunk dev mode"
require_match "--quick" "$START_FILE" "start must support --quick"
require_match "--verify-chat" "$START_FILE" "start must support --verify-chat"

require_match "UI is served by the backend from static/" "$SM_FILE" "service-manager UI contract must declare backend-served static UI"
require_match "start ui" "$SM_FILE" "service-manager usage should still expose start ui compatibility command"
require_match "start-all" "$SM_FILE" "service-manager should support start-all aggregate command"
require_match "scripts/check-config.sh" "$RELEASE_GATE_FILE" "local release gate must run check-config preflight"
require_match "./start preflight" "$RELEASE_GATE_FILE" "local release gate must run canonical ./start preflight path"

echo "=== Startup Contract Check: PASSED ==="
