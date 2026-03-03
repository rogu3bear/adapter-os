#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

ALLOWLIST_REGEX='(^crates/adapteros-config/tests/network_defaults_guard.rs:)|(^scripts/contracts/check_port_contract.sh:)|(^scripts/lib/ports.sh:)|(^crates/adapteros-core/src/defaults.rs:)|(^crates/adapteros-api-types/src/defaults.rs:)'

FORBIDDEN_PATTERN='localhost:8080|127\\.0\\.0\\.1:8080|localhost:3200|127\\.0\\.0\\.1:3200|localhost:3300|127\\.0\\.0\\.1:3300|localhost:3301|127\\.0\\.0\\.1:3301|localhost:9443|127\\.0\\.0\\.1:9443|localhost:9090|127\\.0\\.0\\.1:9090|localhost:50051|127\\.0\\.0\\.1:50051|localhost:5173|127\\.0\\.0\\.1:5173|localhost:3210|127\\.0\\.0\\.1:3210|localhost:4317|127\\.0\\.0\\.1:4317|localhost:8200|127\\.0\\.0\\.1:8200|localhost:9011|127\\.0\\.0\\.1:9011|localhost:5432|127\\.0\\.0\\.1:5432|localhost:4566|127\\.0\\.0\\.1:4566'

search_roots=(start crates scripts tests configs deploy monitoring xtask)
if [[ -d "etc" ]]; then
  search_roots+=(etc)
fi
if [[ -d ".github" ]]; then
  search_roots+=(".github")
fi

hits="$((rg -n --no-heading -S "$FORBIDDEN_PATTERN" \
  "${search_roots[@]}" \
  --glob '!**/target/**' \
  --glob '!**/node_modules/**' \
  --glob '!**/frontend/dist/**' \
  --glob '!**/static/**' || true) | rg -v "$ALLOWLIST_REGEX" || true)"

if [[ -n "$hits" ]]; then
  echo "ERROR: found forbidden legacy localhost port literals outside allowlist:" >&2
  echo "$hits" >&2
  exit 1
fi

required_checks=(
  'scripts/lib/ports.sh:AOS_PORT_PANE_BASE:-18080'
  'scripts/lib/ports.sh:AOS_SERVER_PORT:=$(aos_port_from_offset 0)'
  'scripts/lib/ports.sh:AOS_UI_PORT:=$(aos_port_from_offset 1)'
  'scripts/lib/ports.sh:AOS_PANEL_PORT:=$(aos_port_from_offset 2)'
  'scripts/lib/ports.sh:AOS_NODE_PORT:=$(aos_port_from_offset 3)'
  'scripts/lib/ports.sh:AOS_MODEL_SERVER_PORT:=$(aos_port_from_offset 5)'
  'scripts/lib/ports.sh:AOS_OTLP_PORT:=$(aos_port_from_offset 8)'
  'scripts/lib/ports.sh:AOS_KMS_EMULATOR_PORT:=$(aos_port_from_offset 10)'
  'scripts/lib/ports.sh:AOS_POSTGRES_PORT:=$(aos_port_from_offset 11)'
  'crates/adapteros-core/src/defaults.rs:DEFAULT_SERVER_PORT: u16 = 18080'
  'crates/adapteros-core/src/defaults.rs:DEFAULT_UI_PORT: u16 = 18081'
  'crates/adapteros-core/src/defaults.rs:DEFAULT_MODEL_SERVER_PORT: u16 = 18085'
  'crates/adapteros-core/src/defaults.rs:DEFAULT_TELEMETRY_PORT: u16 = 18088'
  'crates/adapteros-core/src/defaults.rs:DEFAULT_KMS_EMULATOR_PORT: u16 = 18090'
)

for entry in "${required_checks[@]}"; do
  file="${entry%%:*}"
  pattern="${entry#*:}"
  if ! rg -n -F --quiet "$pattern" "$file"; then
    echo "ERROR: required canonical port contract pattern missing: $entry" >&2
    exit 1
  fi
done

echo "=== Port Contract Check: PASSED ==="
