#!/usr/bin/env bash
#
# CI guard: enforce runtime log path/name contract for startup scripts.
#
# Contract:
# - Canonical runtime log file is var/logs/backend.log
# - Script references must not use var/logs/server.log
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

echo "Checking runtime log path contract..."

legacy_refs="$(rg -n 'var/logs/server\.log' scripts start -S -g '!scripts/ci/check_runtime_log_contract.sh' || true)"
if [ -n "$legacy_refs" ]; then
    echo "::error::Found legacy runtime log path references (var/logs/server.log). Use var/logs/backend.log instead."
    echo "$legacy_refs"
    exit 1
fi

for file in start scripts/service-manager.sh; do
    if ! rg -n 'backend\.log' "$file" >/dev/null; then
        echo "::error::Missing backend.log runtime logging contract in $file"
        exit 1
    fi
done

echo "✓ Runtime log path contract checks passed"
