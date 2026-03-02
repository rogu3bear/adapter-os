#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UI_ROUTES_FILE="$ROOT_DIR/crates/adapteros-ui/src/lib.rs"
API_ROUTES_FILE="$ROOT_DIR/crates/adapteros-server-api/src/routes/mod.rs"
TRAINING_ROUTES_FILE="$ROOT_DIR/crates/adapteros-server-api/src/routes/training_routes.rs"

log() {
  printf '[workflow-check] %s\n' "$*"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'ERROR: missing required command: %s\n' "$1" >&2
    exit 1
  }
}

check_pattern() {
  local file="$1"
  local pattern="$2"
  local label="$3"
  if rg -n --no-heading "$pattern" "$file" >/dev/null; then
    log "ok: $label"
  else
    printf 'ERROR: missing %s (%s in %s)\n' "$label" "$pattern" "$file" >&2
    exit 1
  fi
}

run_opt_tests=0
if [[ "${1:-}" == "--with-tests" ]]; then
  run_opt_tests=1
fi

require_cmd rg

log "checking canonical UI routes"
check_pattern "$UI_ROUTES_FILE" 'path!\("/documents"\)' 'Documents route'
check_pattern "$UI_ROUTES_FILE" 'path!\("/datasets"\)' 'Datasets route'
check_pattern "$UI_ROUTES_FILE" 'path!\("/training"\)' 'Training route'
check_pattern "$UI_ROUTES_FILE" 'path!\("/adapters"\)' 'Adapters route'
check_pattern "$UI_ROUTES_FILE" 'path!\("/chat"\)' 'Chat route'
check_pattern "$UI_ROUTES_FILE" 'path!\("/runs"\)' 'Runs route'
check_pattern "$UI_ROUTES_FILE" 'path!\("/routing"\)' 'Routing route'

log "checking canonical API routes"
check_pattern "$API_ROUTES_FILE" '"/v1/documents/upload"' 'Document upload endpoint'
check_pattern "$API_ROUTES_FILE" '"/v1/datasets/from-documents"' 'Dataset-from-documents endpoint'
check_pattern "$API_ROUTES_FILE" '"/v1/infer/stream"' 'Streaming inference endpoint'
check_pattern "$API_ROUTES_FILE" '"/v1/diag/runs"' 'Diagnostic runs endpoint'
check_pattern "$API_ROUTES_FILE" '"/v1/replay"' 'Replay endpoint'
check_pattern "$API_ROUTES_FILE" '"/v1/adapteros/receipts/\{digest\}"' 'Receipt lookup endpoint'
check_pattern "$API_ROUTES_FILE" '"/v1/routing/decisions"' 'Routing decisions endpoint'
check_pattern "$TRAINING_ROUTES_FILE" '"/v1/training/jobs"' 'Training jobs endpoint'
check_pattern "$TRAINING_ROUTES_FILE" '"/v1/training/start"' 'Training start endpoint'

log "checking canonical UI components"
check_pattern "$ROOT_DIR/crates/adapteros-ui/src/pages/chat.rs" 'AdapterMagnet' 'Chat adapter magnet rendering'
check_pattern "$ROOT_DIR/crates/adapteros-ui/src/pages/flight_recorder.rs" 'TokenDecisionsPaged' 'Run detail token/routing visualization'
check_pattern "$ROOT_DIR/crates/adapteros-ui/src/pages/flight_recorder.rs" 'RunDetailHub' 'Run detail hub'

log "checking canonical docs"
check_pattern "$ROOT_DIR/docs/CANONICAL_USER_WORKFLOW.md" 'Canonical User Workflow' 'Canonical workflow document'
check_pattern "$ROOT_DIR/docs/UI_WALKTHROUGH.md" 'CANONICAL_USER_WORKFLOW.md' 'UI walkthrough link to canonical flow'

if [[ "$run_opt_tests" -eq 1 ]]; then
  log "running focused determinism/accounting tests"
  (cd "$ROOT_DIR" && cargo test --test determinism_core_suite -- --test-threads=8)
  (cd "$ROOT_DIR" && cargo test -p adapter-os --no-default-features --test determinism_replay_harness -- --test-threads=1 --nocapture)
  (cd "$ROOT_DIR" && cargo test --test prefix_kv_cache_integration)
fi

log "canonical workflow surface checks passed"
