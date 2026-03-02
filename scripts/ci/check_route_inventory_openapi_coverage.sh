#!/usr/bin/env bash
# CI Gate: enforce route closure between runtime inventory and OpenAPI.
#
# Contract model:
#   - Runtime path-shapes must be represented in OpenAPI (after approved exclusions).
#   - OpenAPI-only path-shapes must be explicitly allowlisted in strict mode.
#   - Parameter-name mismatches are tracked by shape and must be allowlisted in strict mode.
#
# Usage:
#   ./scripts/ci/check_route_inventory_openapi_coverage.sh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

INVENTORY_JSON="$ROOT_DIR/docs/generated/api-route-inventory.json"
OPENAPI_JSON="$ROOT_DIR/docs/api/openapi.json"
OUT_DIR="${ROUTE_CLOSURE_OUT_DIR:-$ROOT_DIR/.planning/prod-cut/artifacts}"
GENERATOR="$ROOT_DIR/scripts/contracts/generate_route_closure_artifacts.py"
: "${ROUTE_COVERAGE_STRICT_OPENAPI_ONLY:=1}"
: "${ROUTE_COVERAGE_STRICT_PARAM_MISMATCH:=1}"

if [[ ! -x "$GENERATOR" ]]; then
    echo "::error::Missing executable: $GENERATOR"
    exit 1
fi

if [[ ! -f "$INVENTORY_JSON" ]]; then
    echo "::error::Missing route inventory: $INVENTORY_JSON"
    echo "Run scripts/contracts/generate_contract_artifacts.py and commit generated files."
    exit 1
fi

if [[ ! -f "$OPENAPI_JSON" ]]; then
    echo "::error::Missing OpenAPI spec: $OPENAPI_JSON"
    echo "Run ./scripts/ci/check_openapi_drift.sh --fix to regenerate."
    exit 1
fi

cmd=(python3 "$GENERATOR" --out-dir "$OUT_DIR")
if [[ "$ROUTE_COVERAGE_STRICT_OPENAPI_ONLY" == "1" ]]; then
    cmd+=(--strict-openapi-only)
fi
if [[ "$ROUTE_COVERAGE_STRICT_PARAM_MISMATCH" == "1" ]]; then
    cmd+=(--strict-param-mismatch)
fi

"${cmd[@]}"

SUMMARY_MD="$OUT_DIR/route_closure_summary.md"
if [[ -f "$SUMMARY_MD" ]]; then
    echo ""
    echo "=== Route Closure Summary ==="
    cat "$SUMMARY_MD"
fi

echo "OK: Route inventory/OpenAPI closure checks passed."
