#!/usr/bin/env bash
# PRD-13 Path Policy Enforcement
# Ensures .env.production uses repo-scoped ./var paths by default
# System-wide /var paths must be commented (as override examples)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ENV_PRODUCTION="$REPO_ROOT/.env.production"

echo "=== PRD-13 Path Policy Check ==="
echo "Checking: $ENV_PRODUCTION"

# Check if file exists
if [[ ! -f "$ENV_PRODUCTION" ]]; then
    echo "ERROR: .env.production not found"
    exit 1
fi

# Find uncommented lines with /var/ paths (excluding /etc/ which is acceptable)
# Pattern: lines that don't start with # and contain /var/
VIOLATIONS=$(grep -n '^[^#]*=/var/' "$ENV_PRODUCTION" 2>/dev/null || true)

if [[ -n "$VIOLATIONS" ]]; then
    echo "ERROR: Found uncommented /var/ paths in .env.production"
    echo "These should use ./var/ (repo-scoped) by default."
    echo ""
    echo "Violations:"
    echo "$VIOLATIONS"
    echo ""
    echo "Fix: Change active defaults to ./var/ paths."
    echo "     Keep /var/ paths as commented examples for system-wide deployments."
    exit 1
fi

# Verify that ./var defaults exist
REPO_PATHS=$(grep -c '^[^#]*=\./var' "$ENV_PRODUCTION" 2>/dev/null || echo "0")
if [[ "$REPO_PATHS" -lt 3 ]]; then
    echo "WARNING: Expected at least 3 ./var path defaults, found $REPO_PATHS"
    echo "Verify that AOS_VAR_DIR, AOS_DATABASE_URL, AOS_SERVER_UDS_SOCKET use ./var"
fi

echo "PASS: .env.production uses repo-scoped ./var paths by default"
echo "      /var paths are properly commented as override examples"
exit 0
