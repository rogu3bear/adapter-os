#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANONICAL="$SCRIPT_DIR/install_git_hooks.sh"

echo "[DEPRECATED] use scripts/install_git_hooks.sh; compatibility path will be removed after 2026-06-30" >&2
exec "$CANONICAL" "$@"
