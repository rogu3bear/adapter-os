#!/bin/bash
# DEPRECATED: Use ./start instead
#
# This script has been replaced by the unified ./start entry point.
# Redirecting to ./start for backwards compatibility.

echo ""
echo "WARNING: scripts/start.sh is deprecated."
echo "Please use: ./start"
echo ""
echo "Redirecting in 2 seconds..."
echo ""
sleep 2

cd "$(dirname "${BASH_SOURCE[0]}")/.."
exec ./start "$@"
