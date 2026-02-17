#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

scripts/contracts/check_contract_artifacts.sh
scripts/contracts/check_api_route_tiers.py
scripts/contracts/check_ui_routes.py
scripts/contracts/check_middleware_chain.py
scripts/contracts/check_startup_contract.sh
scripts/contracts/check_determinism_contract.sh
scripts/contracts/check_docs_claims.sh
scripts/ci/check_route_inventory_openapi_coverage.sh
scripts/contracts/check_error_code_coverage.sh
scripts/contracts/check_handler_error_response_with_code.sh
scripts/ci/check_handler_annotations.sh

echo "=== Rectification Contract Suite: PASSED ==="
