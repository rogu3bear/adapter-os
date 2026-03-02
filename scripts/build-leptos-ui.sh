#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANONICAL="$SCRIPT_DIR/build-ui.sh"

echo "[DEPRECATED] use scripts/build-ui.sh; compatibility path will be removed after 2026-06-30" >&2
exec "$CANONICAL" "$@"
