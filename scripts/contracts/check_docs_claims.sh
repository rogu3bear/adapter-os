#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

fail() {
  echo "FAIL: $1"
  exit 1
}

require_file() {
  local path="$1"
  [[ -f "$path" ]] || fail "Missing required doc: $path"
}

require_match() {
  local pattern="$1"
  local file="$2"
  local msg="$3"
  rg -q "$pattern" "$file" || fail "$msg ($file)"
}

require_file "docs/CANONICAL_SOURCES.md"
require_file "docs/RECTIFICATION_GAP_REPORT.md"
require_file "docs/generated/api-route-inventory.json"
require_file "docs/generated/ui-route-inventory.json"
require_file "docs/generated/middleware-chain.json"

scripts/contracts/generate_contract_artifacts.py --check

# Startup doc claims: both quickstarts should mention canonical startup path
require_match "./start" "QUICKSTART.md" "Top-level QUICKSTART must reference ./start"
require_match "trunk serve" "QUICKSTART.md" "Top-level QUICKSTART must mention trunk dev mode"
require_match "./start" "docs/QUICKSTART.md" "docs/QUICKSTART must reference ./start"

# Canonical source index should point at critical runtime files
require_match "crates/adapteros-server/src/main.rs" "docs/CANONICAL_SOURCES.md" "Canonical index missing server main"
require_match "crates/adapteros-server-api/src/routes/mod.rs" "docs/CANONICAL_SOURCES.md" "Canonical index missing route source"
require_match "crates/adapteros-ui/src/lib.rs" "docs/CANONICAL_SOURCES.md" "Canonical index missing UI route source"

# Gap report must include drift severity matrix
require_match "Claim-vs-Source Matrix" "docs/RECTIFICATION_GAP_REPORT.md" "Gap report missing matrix section"
require_match "Severity" "docs/RECTIFICATION_GAP_REPORT.md" "Gap report missing severity column"

echo "=== Documentation Claims Check: PASSED ==="
