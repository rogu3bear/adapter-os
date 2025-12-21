#!/usr/bin/env bash
set -euo pipefail

echo "This script is deprecated; use 'aosctl db migrate' instead." >&2
echo "" >&2
echo "Replacement command:" >&2
echo "  aosctl db migrate" >&2

exec aosctl db migrate "$@"
