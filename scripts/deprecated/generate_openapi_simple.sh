#!/usr/bin/env bash
set -euo pipefail

# DEPRECATED: This script is deprecated and will be removed in a future release.
# Replacement: cargo xtask openapi-docs

echo "" >&2
echo "┌──────────────────────────────────────────────────────────────┐" >&2
echo "│  ⚠️  DEPRECATED: scripts/generate_openapi_simple.sh          │" >&2
echo "│                                                              │" >&2
echo "│  This script has been replaced by cargo xtask.               │" >&2
echo "│                                                              │" >&2
echo "│  Replacement command:                                        │" >&2
echo "│    cargo xtask openapi-docs                                  │" >&2
echo "│                                                              │" >&2
echo "│  Note: The old script required a running server.             │" >&2
echo "│  The new xtask command generates docs without a server.      │" >&2
echo "│                                                              │" >&2
echo "│  See: docs/DEPRECATIONS.md for more information.             │" >&2
echo "└──────────────────────────────────────────────────────────────┘" >&2
echo "" >&2

# Delegate to cargo xtask
exec cargo xtask openapi-docs "$@"
