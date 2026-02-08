#!/usr/bin/env bash
set -euo pipefail

# One-command local fix path for OpenAPI drift.
#
# This updates docs/api/openapi.json from the current Rust routes/types and then
# prints the diff. CI runs the drift check in non-fix mode.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

./scripts/ci/check_openapi_drift.sh --fix

echo ""
echo "Diff vs HEAD (docs/api/openapi.json):"
echo "────────────────────────────────────────"
git diff -- docs/api/openapi.json || true
echo "────────────────────────────────────────"
echo ""
echo "Next:"
echo "  git add docs/api/openapi.json"

