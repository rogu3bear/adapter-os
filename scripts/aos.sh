#!/usr/bin/env bash
set -euo pipefail

echo "This script is deprecated; use the 'aos' Rust binary instead." >&2
echo "" >&2
echo "Examples:" >&2
echo "  aos start backend" >&2
echo "  aos status --json" >&2

exec aos "$@"
