#!/usr/bin/env bash
# CI gate: validates boot + health without full training
# Wraps smoke_happy_path.sh for PLAN_3 compliance
#
# This is the lightweight CI regression gate. For full training validation,
# use scripts/golden_path_adapter_chat.sh (not suitable for every commit).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANONICAL="$SCRIPT_DIR/../test/smoke_happy_path.sh"
echo "[DEPRECATED] use scripts/test/smoke_happy_path.sh; compatibility path will be removed after 2026-06-30" >&2
exec "$CANONICAL" "$@"
