#!/usr/bin/env bash
# CI gate: enforce unified error/logging helper usage in hotspot files.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

required_backend_fields=(
  "component"
  "operation"
  "code"
  "failure_code"
  "request_id"
  "tenant_id"
  "error_id"
  "diag_trace_id"
  "otel_trace_id"
  "retryable"
)

backend_helper_file="crates/adapteros-server-api/src/api_error.rs"
worker_helper_file="crates/adapteros-lora-worker/src/panic_utils.rs"

backend_hotspots=(
  "crates/adapteros-server-api/src/middleware/observability.rs"
  "crates/adapteros-server-api/src/middleware/policy_enforcement.rs"
  "crates/adapteros-server-api/src/supervisor_client.rs"
  "crates/adapteros-server-api/src/handlers/workers.rs"
  "crates/adapteros-server/src/boot/background_tasks.rs"
)

worker_hotspots=(
  "crates/adapteros-lora-worker/src/deadlock.rs"
  "crates/adapteros-lora-worker/src/uds_server.rs"
)

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

require_pattern() {
  local file="$1"
  local pattern="$2"
  local msg="$3"
  if ! rg -n "$pattern" "$file" >/dev/null; then
    fail "$msg (file: $file, pattern: $pattern)"
  fi
}

echo "== Unified Error Logging Contract Check =="

require_pattern "$backend_helper_file" 'pub fn log_error_event\(' \
  "Missing backend logging helper"
for field in "${required_backend_fields[@]}"; do
  require_pattern "$backend_helper_file" "\\b${field}\\b" \
    "Backend logging helper is missing canonical field '${field}'"
done

require_pattern "$worker_helper_file" 'pub fn log_structured_worker_error\(' \
  "Missing worker logging helper"
for field in "${required_backend_fields[@]}"; do
  require_pattern "$worker_helper_file" "\\b${field}\\b" \
    "Worker logging helper is missing canonical field '${field}'"
done

for file in "${backend_hotspots[@]}"; do
  require_pattern "$file" 'log_error_event\(' \
    "Hotspot file does not use backend unified logging helper"
done

for file in "${worker_hotspots[@]}"; do
  require_pattern "$file" 'log_structured_worker_error\(' \
    "Hotspot file does not use worker unified logging helper"
done

# Bypass checks: prevent direct ad-hoc DB/Internal log strings in hotspot paths.
bypass_patterns=(
  'error!\s*\(\s*"Database error:'
  'error!\s*\(\s*"Internal error:'
)

for file in "${backend_hotspots[@]}"; do
  for pattern in "${bypass_patterns[@]}"; do
    if rg -n "$pattern" "$file" >/dev/null; then
      fail "Found bypass logging pattern in hotspot file (file: $file, pattern: $pattern)"
    fi
  done
done

echo "OK: Unified error/logging contract checks passed"
