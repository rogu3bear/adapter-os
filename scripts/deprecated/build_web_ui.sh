#!/usr/bin/env bash
set -euo pipefail

# DEPRECATED: This script is deprecated and will be removed in a future release.
# Replacement: cd crates/adapteros-ui && trunk build --release

echo "" >&2
echo "┌──────────────────────────────────────────────────────────────┐" >&2
echo "│  ⚠️  DEPRECATED: scripts/build_web_ui.sh                     │" >&2
echo "│                                                              │" >&2
echo "│  This script is redundant with pnpm build.                   │" >&2
echo "│                                                              │" >&2
echo "│  Replacement commands:                                       │" >&2
echo "│    cd crates/adapteros-ui && trunk build --release           │" >&2
echo "│                                                              │" >&2
echo "│  See: docs/DEPRECATIONS.md for more information.             │" >&2
echo "└──────────────────────────────────────────────────────────────┘" >&2
echo "" >&2

# Delegate to trunk build
cd "$(dirname "$0")/../crates/adapteros-ui"
exec trunk build --release
