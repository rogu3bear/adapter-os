#!/usr/bin/env bash
# API Coverage Audit Script - A1-A4 Tasks
# Analyzes RBAC, audit logging, and OpenAPI coverage

set -euo pipefail

HANDLERS_DIR="crates/adapteros-server-api/src/handlers"
ROUTES_FILE="crates/adapteros-server-api/src/routes.rs"

echo "=== API Infrastructure Audit Report ==="
echo "Date: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
echo

# A1: OpenAPI Coverage
echo "## A1: OpenAPI Documentation Coverage"
echo "-----------------------------------"
total_handlers=$(find "$HANDLERS_DIR" -name "*.rs" -type f | wc -l | awk '{print $1}')
openapi_annotated=$(grep -r "#\[utoipa::path\]" "$HANDLERS_DIR" 2>/dev/null | wc -l | awk '{print $1}')
echo "Total handler files: $total_handlers"
echo "OpenAPI annotated functions: $openapi_annotated"
echo

# A2: RBAC Permission Checks
echo "## A2: RBAC Permission Coverage"
echo "-----------------------------"
require_permission=$(grep -r "require_permission" "$HANDLERS_DIR" 2>/dev/null | wc -l | awk '{print $1}')
require_role=$(grep -rE "require_role|require_any_role" "$HANDLERS_DIR" 2>/dev/null | wc -l | awk '{print $1}')
total_rbac=$((require_permission + require_role))
echo "require_permission calls: $require_permission"
echo "require_role/require_any_role calls: $require_role"
echo "Total RBAC checks: $total_rbac"
echo

# A3: Audit Logging Coverage
echo "## A3: Audit Logging Coverage"
echo "----------------------------"
log_success=$(grep -r "log_success" "$HANDLERS_DIR" 2>/dev/null | wc -l | awk '{print $1}')
log_failure=$(grep -r "log_failure" "$HANDLERS_DIR" 2>/dev/null | wc -l | awk '{print $1}')
total_audit=$((log_success + log_failure))
echo "log_success calls: $log_success"
echo "log_failure calls: $log_failure"
echo "Total audit log calls: $total_audit"
echo

# A4: Rate Limiting Status
echo "## A4: Rate Limiting Status"
echo "-------------------------"
if grep -q "rate_limiting_middleware" "$ROUTES_FILE"; then
    echo "✓ Rate limiting middleware: ACTIVE"
    if grep -q "X-RateLimit-Limit\|X-RateLimit-Remaining" "crates/adapteros-server-api/src/middleware_security.rs"; then
        echo "✓ Rate limit headers: IMPLEMENTED"
    fi
    if grep -q "StatusCode::TOO_MANY_REQUESTS\|429" "crates/adapteros-server-api/src/middleware_security.rs"; then
        echo "✓ 429 Too Many Requests: IMPLEMENTED"
    fi
else
    echo "✗ Rate limiting middleware: NOT FOUND"
fi
echo

# Handler files missing RBAC
echo "## Handlers Missing RBAC Checks"
echo "------------------------------"
for file in "$HANDLERS_DIR"/*.rs; do
    if ! grep -q "require_permission\|require_role\|require_any_role" "$file"; then
        basename "$file"
    fi
done
echo

# Handler files missing audit logging
echo "## Handlers Missing Audit Logging"
echo "--------------------------------"
for file in "$HANDLERS_DIR"/*.rs; do
    # Skip read-only handlers (list, get, view)
    if grep -q "pub async fn.*\(create\|delete\|update\|register\|start\|cancel\|upload\)" "$file"; then
        if ! grep -q "log_success\|log_failure\|log_action" "$file"; then
            basename "$file"
        fi
    fi
done
echo

echo "=== Summary ==="
echo "OpenAPI: $openapi_annotated functions documented"
echo "RBAC: $total_rbac permission checks"
echo "Audit: $total_audit audit log calls"
echo "Rate Limiting: $(grep -q 'rate_limiting_middleware' "$ROUTES_FILE" && echo 'ACTIVE' || echo 'INACTIVE')"
echo
echo "Report complete."
