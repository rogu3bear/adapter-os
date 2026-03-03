#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

required_patterns=(
  'crates/adapteros-server-api/src/runtime_config_store.rs:const MANAGED_KEYS'
  'crates/adapteros-server-api/src/runtime_config_store.rs:pub async fn persist_runtime_update'
  'crates/adapteros-server-api/src/handlers/settings.rs:pub async fn get_effective_settings'
  'crates/adapteros-server-api/src/handlers/settings.rs:pub async fn reconcile_settings'
  'crates/adapteros-server-api/src/routes/mod.rs:/v1/settings/effective'
  'crates/adapteros-server-api/src/routes/mod.rs:/v1/settings/reconcile'
  'crates/adapteros-api-types/src/settings.rs:pub struct EffectiveSettingsResponse'
  'crates/adapteros-api-types/src/settings.rs:pub struct SettingsReconcileResponse'
)

for entry in "${required_patterns[@]}"; do
  file="${entry%%:*}"
  pattern="${entry#*:}"
  if ! rg -n -F --quiet "$pattern" "$file"; then
    echo "ERROR: runtime settings contract missing pattern: $entry" >&2
    exit 1
  fi
done

echo "=== Runtime Settings Contract Check: PASSED ==="
