#!/usr/bin/env bash
# CI Gate: Ensure runtime route inventory paths are covered by OpenAPI paths.
#
# This check is intentionally one-way:
#   runtime route inventory -> OpenAPI
#
# OpenAPI may contain additional paths (aliases, legacy docs, staged routes);
# this script reports those as warnings but does not fail on them.
#
# Usage:
#   ./scripts/ci/check_route_inventory_openapi_coverage.sh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

INVENTORY_JSON="$ROOT_DIR/docs/generated/api-route-inventory.json"
OPENAPI_JSON="$ROOT_DIR/docs/api/openapi.json"
EXCLUSIONS_FILE="$ROOT_DIR/docs/api/openapi_route_coverage_exclusions.txt"
TMP_DIR="$ROOT_DIR/target/codegen/route-openapi-coverage"

mkdir -p "$TMP_DIR"

RUNTIME_PATHS="$TMP_DIR/runtime_paths.txt"
OPENAPI_PATHS="$TMP_DIR/openapi_paths.txt"
RUNTIME_NOT_IN_OPENAPI="$TMP_DIR/runtime_not_in_openapi.txt"
RUNTIME_NOT_IN_OPENAPI_FILTERED="$TMP_DIR/runtime_not_in_openapi.filtered.txt"
OPENAPI_NOT_IN_RUNTIME="$TMP_DIR/openapi_not_in_runtime.txt"
EXCLUSIONS_CLEAN="$TMP_DIR/exclusions.clean.txt"

if ! command -v jq >/dev/null 2>&1; then
    echo "::error::jq is required for route/openapi coverage checks"
    exit 1
fi

if [ ! -f "$INVENTORY_JSON" ]; then
    echo "::error::Missing route inventory: $INVENTORY_JSON"
    echo "Run ./scripts/dev/generate_route_map.sh to regenerate inventory artifacts."
    exit 1
fi

if [ ! -f "$OPENAPI_JSON" ]; then
    echo "::error::Missing OpenAPI spec: $OPENAPI_JSON"
    echo "Run ./scripts/ci/check_openapi_drift.sh --fix to regenerate."
    exit 1
fi

jq -r '.tiers | to_entries[] | .value[]' "$INVENTORY_JSON" \
    | sed '/^[[:space:]]*$/d' \
    | sort -u > "$RUNTIME_PATHS"

jq -r '.paths | keys[]' "$OPENAPI_JSON" \
    | sed '/^[[:space:]]*$/d' \
    | sort -u > "$OPENAPI_PATHS"

comm -23 "$RUNTIME_PATHS" "$OPENAPI_PATHS" > "$RUNTIME_NOT_IN_OPENAPI"
comm -13 "$RUNTIME_PATHS" "$OPENAPI_PATHS" > "$OPENAPI_NOT_IN_RUNTIME"

if [ -f "$EXCLUSIONS_FILE" ]; then
    sed -E 's/[[:space:]]*#.*$//; s/^[[:space:]]+//; s/[[:space:]]+$//' "$EXCLUSIONS_FILE" \
        | grep -E '^/' \
        | sort -u > "$EXCLUSIONS_CLEAN"
    grep -Fvx -f "$EXCLUSIONS_CLEAN" "$RUNTIME_NOT_IN_OPENAPI" > "$RUNTIME_NOT_IN_OPENAPI_FILTERED" || true
else
    cp "$RUNTIME_NOT_IN_OPENAPI" "$RUNTIME_NOT_IN_OPENAPI_FILTERED"
fi

runtime_count="$(wc -l < "$RUNTIME_PATHS" | tr -d ' ')"
openapi_count="$(wc -l < "$OPENAPI_PATHS" | tr -d ' ')"
missing_runtime_count="$(wc -l < "$RUNTIME_NOT_IN_OPENAPI_FILTERED" | tr -d ' ')"
openapi_only_count="$(wc -l < "$OPENAPI_NOT_IN_RUNTIME" | tr -d ' ')"

echo "=== Route Inventory vs OpenAPI Coverage ==="
echo "Runtime paths: $runtime_count"
echo "OpenAPI paths: $openapi_count"
echo "Runtime paths missing from OpenAPI (after exclusions): $missing_runtime_count"
echo "OpenAPI-only paths (informational): $openapi_only_count"

if [ "$missing_runtime_count" -gt 0 ]; then
    echo ""
    echo "::error::Runtime route paths are missing from OpenAPI (after exclusions):"
    cat "$RUNTIME_NOT_IN_OPENAPI_FILTERED"
    echo ""
    echo "Fix options:"
    echo "  1) Add missing paths to OpenAPI annotations/routes."
    echo "  2) If intentional, document exclusion in docs/api/openapi_route_coverage_exclusions.txt."
    exit 1
fi

if [ "$openapi_only_count" -gt 0 ]; then
    echo ""
    echo "::warning::OpenAPI has paths not present in runtime route inventory (showing first 40):"
    head -n 40 "$OPENAPI_NOT_IN_RUNTIME"
fi

echo "OK: Runtime route inventory is covered by OpenAPI paths."
