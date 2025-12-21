#!/usr/bin/env bash
set -euo pipefail

# DEPRECATED: This script is deprecated and will be removed in a future release.
# Replacement: ./start (unified boot system)

echo "" >&2
echo "┌──────────────────────────────────────────────────────────────┐" >&2
echo "│  ⚠️  DEPRECATED: scripts/init_cp.sh                          │" >&2
echo "│                                                              │" >&2
echo "│  This script has been superseded by the unified boot system. │" >&2
echo "│                                                              │" >&2
echo "│  Replacement command:                                        │" >&2
echo "│    ./start                                                   │" >&2
echo "│                                                              │" >&2
echo "│  The ./start script automatically:                           │" >&2
echo "│    - Creates required directories                            │" >&2
echo "│    - Runs database migrations                                │" >&2
echo "│    - Starts the control plane                                │" >&2
echo "│                                                              │" >&2
echo "│  See: docs/DEPRECATIONS.md for more information.             │" >&2
echo "└──────────────────────────────────────────────────────────────┘" >&2
echo "" >&2

# Delegate to the unified boot system
exec ./start "$@"
