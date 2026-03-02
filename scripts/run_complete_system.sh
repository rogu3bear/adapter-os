#!/bin/bash
# DEPRECATED: Use ./start instead
#
# This script has been replaced by the unified ./start entry point.
# Redirecting to ./start for backwards compatibility.

echo ""
echo "WARNING: scripts/run_complete_system.sh is deprecated."
echo "Please use: ./start"
echo ""
echo "This legacy shim will prompt before continuing (15s timeout, default: No)."
echo ""

read -r -t 15 -p "Proceed with deprecated scripts/run_complete_system.sh? [y/N]: " REPLY || REPLY=""
echo ""

if [[ ! "$REPLY" =~ ^[Yy]$ ]]; then
  echo "Aborting. Run ./start instead."
  exit 1
fi

cd "$(dirname "${BASH_SOURCE[0]}")/.."
exec ./start "$@"
