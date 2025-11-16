#!/usr/bin/env bash
set -euo pipefail

echo "This script is deprecated; use 'aosctl maintenance gc-bundles' instead." >&2
echo "" >&2
echo "Replacement command:" >&2
echo "  aosctl maintenance gc-bundles" >&2

exec aosctl maintenance gc-bundles "$@"
