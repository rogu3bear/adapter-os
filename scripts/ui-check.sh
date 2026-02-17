#!/bin/bash
set -euo pipefail

# Single-instance UI check - prevents multiple agents from running cargo check simultaneously.
# Uses repo-local var/ path to avoid /tmp usage and preserve repo hygiene.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCK_DIR="$ROOT_DIR/var/run"
LOCKFILE="$LOCK_DIR/adapteros-ui-check.lock"
LOCKFD=200

mkdir -p "$LOCK_DIR"

if command -v flock >/dev/null 2>&1; then
    # Try to acquire lock (non-blocking), then wait if already held.
    exec {LOCKFD}>"$LOCKFILE"
    if ! flock -n "$LOCKFD"; then
        echo "Another UI check is running. Waiting..."
        flock "$LOCKFD"
    fi
else
    echo "flock not found; running UI check without single-instance lock."
fi

echo "Running UI check..."
RUSTC_WRAPPER= cargo check -p adapteros-ui --target wasm32-unknown-unknown 2>&1
