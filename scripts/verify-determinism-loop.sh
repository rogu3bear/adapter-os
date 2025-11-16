#!/usr/bin/env bash
set -euo pipefail

echo "This script is deprecated; use 'aosctl verify determinism-loop' instead." >&2
echo "" >&2
echo "Replacement command:" >&2
echo "  aosctl verify determinism-loop" >&2

exec aosctl verify determinism-loop "$@"
