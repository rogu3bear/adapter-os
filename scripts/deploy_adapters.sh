#!/usr/bin/env bash
set -euo pipefail

echo "This script is deprecated; use 'aosctl deploy adapters' instead." >&2
echo "" >&2
echo "Replacement command:" >&2
echo "  aosctl deploy adapters \"\$@\"" >&2

exec aosctl deploy adapters "$@"
