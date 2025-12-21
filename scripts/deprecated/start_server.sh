#!/usr/bin/env bash
set -euo pipefail

# DEPRECATED: This script is deprecated and will be removed in a future release.
# Replacement: ./start (unified boot system)

echo "" >&2
echo "┌──────────────────────────────────────────────────────────────┐" >&2
echo "│  ⚠️  DEPRECATED: scripts/start_server.sh                     │" >&2
echo "│                                                              │" >&2
echo "│  This script has been superseded by the unified boot system. │" >&2
echo "│                                                              │" >&2
echo "│  Replacement commands:                                       │" >&2
echo "│    ./start                    # Start all services           │" >&2
echo "│    ./start backend            # Start backend only           │" >&2
echo "│                                                              │" >&2
echo "│  The ./start script provides:                                │" >&2
echo "│    - Boot state management                                   │" >&2
echo "│    - Port conflict detection                                 │" >&2
echo "│    - Graceful shutdown handling                              │" >&2
echo "│    - Service health monitoring                               │" >&2
echo "│                                                              │" >&2
echo "│  See: docs/DEPRECATIONS.md for more information.             │" >&2
echo "└──────────────────────────────────────────────────────────────┘" >&2
echo "" >&2

# Delegate to the unified boot system (backend only)
exec ./start backend "$@"
