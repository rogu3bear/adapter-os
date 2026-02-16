#!/usr/bin/env bash
# CI Gate: prevent worker contract drift across routes, OpenAPI, docs, and UI client.
#
# Scope is intentionally narrow:
# - /v1/workers* route registrations vs OpenAPI endpoint inventory (method + path)
# - SpawnWorkerRequest required fields in OpenAPI
# - Worker lifecycle/docs contract in docs/API_REFERENCE.md
# - Worker lifecycle client routes in crates/adapteros-ui/src/api/client.rs

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
export LC_ALL=C
export LANG=C

ROUTES_DIR="$ROOT_DIR/crates/adapteros-server-api/src/routes"
OPENAPI_JSON="$ROOT_DIR/docs/api/openapi.json"
DOCS_FILE="$ROOT_DIR/docs/API_REFERENCE.md"
UI_CLIENT_FILE="$ROOT_DIR/crates/adapteros-ui/src/api/client.rs"
TMP_DIR="$ROOT_DIR/target/codegen/worker-contract-drift"

mkdir -p "$TMP_DIR"

ROUTE_ENDPOINTS="$TMP_DIR/route_endpoints.txt"
OPENAPI_ENDPOINTS="$TMP_DIR/openapi_endpoints.txt"
ROUTE_MISSING_IN_OPENAPI="$TMP_DIR/route_missing_in_openapi.txt"
OPENAPI_MISSING_IN_ROUTE="$TMP_DIR/openapi_missing_in_route.txt"

for tool in jq rg; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "::error::Missing required tool: $tool"
        exit 1
    fi
done

for file in "$OPENAPI_JSON" "$DOCS_FILE" "$UI_CLIENT_FILE"; do
    if [ ! -f "$file" ]; then
        echo "::error::Missing required file: $file"
        exit 1
    fi
done

if [ ! -d "$ROUTES_DIR" ]; then
    echo "::error::Missing routes directory: $ROUTES_DIR"
    exit 1
fi

# Extract worker endpoints from route registrations.
# Output shape: "<path> <method>", lowercase method names.
rg --no-filename -UPo --replace '$1 $2' \
    '\.route\(\s*"(/v1/workers[^"]*)"\s*,\s*(get|post|put|patch|delete|axum::routing::delete)\s*\(' \
    "$ROUTES_DIR" -g '*.rs' \
    | sed -E 's/axum::routing:://' \
    | awk 'NF == 2 { print $1 " " $2 }' \
    | sort -u > "$ROUTE_ENDPOINTS"

if [ ! -s "$ROUTE_ENDPOINTS" ]; then
    echo "::error::No /v1/workers* route registrations were discovered under $ROUTES_DIR"
    echo "Action: verify route parsing pattern in this guard and worker route registration style."
    exit 1
fi

# Extract worker endpoints from OpenAPI path+method inventory.
jq -r '
  .paths
  | to_entries[]
  | select(.key | startswith("/v1/workers"))
  | .key as $path
  | .value
  | to_entries[]
  | select(.key | IN("get","post","put","patch","delete"))
  | "\($path) \(.key)"
' "$OPENAPI_JSON" | sort -u > "$OPENAPI_ENDPOINTS"

if [ ! -s "$OPENAPI_ENDPOINTS" ]; then
    echo "::error::OpenAPI has no /v1/workers* endpoints in $OPENAPI_JSON"
    echo "Action: add missing worker path annotations and regenerate OpenAPI."
    exit 1
fi

comm -23 "$ROUTE_ENDPOINTS" "$OPENAPI_ENDPOINTS" > "$ROUTE_MISSING_IN_OPENAPI"
comm -13 "$ROUTE_ENDPOINTS" "$OPENAPI_ENDPOINTS" > "$OPENAPI_MISSING_IN_ROUTE"

failure=0

if [ -s "$ROUTE_MISSING_IN_OPENAPI" ]; then
    echo "::error::Worker route endpoints missing from OpenAPI (method path):"
    cat "$ROUTE_MISSING_IN_OPENAPI"
    echo "Action: add/update worker utoipa annotations and regenerate:"
    echo "  ./scripts/ci/check_openapi_drift.sh --fix && git add docs/api/openapi.json"
    failure=1
fi

if [ -s "$OPENAPI_MISSING_IN_ROUTE" ]; then
    echo "::error::OpenAPI contains /v1/workers* endpoints not present in route registrations (method path):"
    cat "$OPENAPI_MISSING_IN_ROUTE"
    echo "Action: remove stale OpenAPI worker endpoints or restore matching route registrations."
    failure=1
fi

# Validate critical SpawnWorkerRequest required fields.
required_spawn_fields=(tenant_id node_id plan_id uds_path)
missing_spawn_fields=()
for field in "${required_spawn_fields[@]}"; do
    if ! jq -e --arg field "$field" \
        '.components.schemas.SpawnWorkerRequest.required // [] | index($field) != null' \
        "$OPENAPI_JSON" >/dev/null; then
        missing_spawn_fields+=("$field")
    fi
done

if [ "${#missing_spawn_fields[@]}" -gt 0 ]; then
    echo "::error::SpawnWorkerRequest is missing required OpenAPI field(s): ${missing_spawn_fields[*]}"
    echo "Action: mark these fields as required in SpawnWorkerRequest and regenerate OpenAPI."
    failure=1
fi

# Validate docs contract (lifecycle endpoints + critical spawn fields) in API reference.
required_docs_entries=(
    "POST /v1/workers/{worker_id}/drain"
    "POST /v1/workers/{worker_id}/stop"
    "POST /v1/workers/{worker_id}/restart"
    "DELETE /v1/workers/{worker_id}"
    "- \`tenant_id\` -"
    "- \`node_id\` -"
    "- \`plan_id\` -"
    "- \`uds_path\` -"
)
missing_docs_entries=()
for entry in "${required_docs_entries[@]}"; do
    if ! rg -Fq -- "$entry" "$DOCS_FILE"; then
        missing_docs_entries+=("$entry")
    fi
done

if [ "${#missing_docs_entries[@]}" -gt 0 ]; then
    echo "::error::Worker docs contract drift in $DOCS_FILE"
    echo "Missing required worker docs entries:"
    printf '  - %s\n' "${missing_docs_entries[@]}"
    echo "Action: update the Workers & Nodes section with lifecycle endpoints and required spawn fields."
    failure=1
fi

# Validate UI client routes for worker lifecycle operations.
required_ui_entries=(
    "/v1/workers/spawn"
    "/v1/workers/{}/drain"
    "/v1/workers/{}/stop"
    "/v1/workers/{}/restart"
    "/v1/workers/{}"
)
missing_ui_entries=()
for entry in "${required_ui_entries[@]}"; do
    if ! rg -Fq -- "$entry" "$UI_CLIENT_FILE"; then
        missing_ui_entries+=("$entry")
    fi
done

if [ "${#missing_ui_entries[@]}" -gt 0 ]; then
    echo "::error::UI worker lifecycle route contract drift in $UI_CLIENT_FILE"
    echo "Missing required worker route entries:"
    printf '  - %s\n' "${missing_ui_entries[@]}"
    echo "Action: ensure UI client exposes spawn/drain/stop/restart/delete worker routes."
    failure=1
fi

if [ "$failure" -ne 0 ]; then
    exit 1
fi

echo "OK: Worker contract drift checks passed."
