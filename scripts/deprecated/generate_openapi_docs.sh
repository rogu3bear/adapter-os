#!/usr/bin/env bash
set -euo pipefail

# DEPRECATED: This script is deprecated and will be removed in a future release.
# Replacement: cargo xtask openapi-docs

echo "" >&2
echo "┌──────────────────────────────────────────────────────────────┐" >&2
echo "│  ⚠️  DEPRECATED: scripts/generate_openapi_docs.sh            │" >&2
echo "│                                                              │" >&2
echo "│  This script has been replaced by cargo xtask.               │" >&2
echo "│                                                              │" >&2
echo "│  Replacement command:                                        │" >&2
echo "│    cargo xtask openapi-docs                                  │" >&2
echo "│                                                              │" >&2
echo "│  Benefits of the new approach:                               │" >&2
echo "│    - Better integration with Rust tooling                    │" >&2
echo "│    - Doesn't require external dependencies                   │" >&2
echo "│    - Consistent with other dev workflows                     │" >&2
echo "│                                                              │" >&2
echo "│  See: docs/DEPRECATIONS.md for more information.             │" >&2
echo "└──────────────────────────────────────────────────────────────┘" >&2
echo "" >&2

# Delegate to cargo xtask
exec cargo xtask openapi-docs "$@"
