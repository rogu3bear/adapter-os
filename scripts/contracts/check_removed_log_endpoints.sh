#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

declare -a forbidden_routes=(
    "/v1/services/{service_id}/logs"
    "/v1/services/:service_id/logs"
    "/v1/training/jobs/{job_id}/logs"
    "/v1/training/jobs/:job_id/logs"
)

declare -a candidate_files=(
    "crates/adapteros-server-api/src/routes/mod.rs"
    "crates/adapteros-server-api/src/routes/training_routes.rs"
    "crates/adapteros-server-api-admin/src/routes.rs"
    "crates/adapteros-service-supervisor/src/server.rs"
    "docs/api/openapi.json"
    "docs/generated/api-route-inventory.json"
    ".planning/prod-cut/artifacts/openapi_routes.json"
    ".planning/prod-cut/artifacts/runtime_routes.json"
    ".planning/prod-cut/artifacts/route_closure_matrix.csv"
)

declare -a existing_files=()
for file in "${candidate_files[@]}"; do
    if [[ -f "$file" ]]; then
        existing_files+=("$file")
    fi
done

if [[ "${#existing_files[@]}" -eq 0 ]]; then
    echo "No candidate files found for removed-log-endpoint guard; skipping."
    exit 0
fi

found=0
for route in "${forbidden_routes[@]}"; do
    hits="$(rg -n --fixed-strings "$route" "${existing_files[@]}" || true)"
    if [[ -n "$hits" ]]; then
        echo "::error::Retired log endpoint resurfaced: $route"
        echo "$hits"
        found=1
    fi
done

if [[ "$found" -ne 0 ]]; then
    exit 1
fi

echo "=== Removed Log Endpoint Guard: PASSED ==="
