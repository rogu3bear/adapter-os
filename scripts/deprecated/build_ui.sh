#!/usr/bin/env bash
set -euo pipefail

# DEPRECATED: This script is deprecated and will be removed in a future release.
# Replacement: pnpm build (in ui/ directory) or make ui

echo "" >&2
echo "┌──────────────────────────────────────────────────────────────┐" >&2
echo "│  ⚠️  DEPRECATED: scripts/build_ui.sh                         │" >&2
echo "│                                                              │" >&2
echo "│  This script used the obsolete WebAssembly/Trunk approach.   │" >&2
echo "│  The UI is now React-based and built with pnpm.              │" >&2
echo "│                                                              │" >&2
echo "│  Replacement commands:                                       │" >&2
echo "│    cd ui && pnpm build        # Direct pnpm build            │" >&2
echo "│    make ui                    # Via Makefile                 │" >&2
echo "│                                                              │" >&2
echo "│  See: docs/DEPRECATIONS.md for more information.             │" >&2
echo "└──────────────────────────────────────────────────────────────┘" >&2
echo "" >&2

# Delegate to pnpm build
cd "$(dirname "$0")/../ui"
exec pnpm build "$@"
