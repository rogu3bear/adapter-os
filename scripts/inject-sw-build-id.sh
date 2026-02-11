#!/usr/bin/env bash
# Inject build ID into service worker for cache busting.
# Usage: scripts/inject-sw-build-id.sh [sw.js path]

set -euo pipefail

SW_PATH="${1:-crates/adapteros-ui/dist/sw.js}"
BUILD_ID_FILE="target/build_id.txt"

if [ -f "$BUILD_ID_FILE" ]; then
    BUILD_ID=$(cat "$BUILD_ID_FILE" | tr -d '[:space:]')
else
    BUILD_ID="dev"
fi

if [ ! -f "$SW_PATH" ]; then
    echo "WARN: sw.js not found at $SW_PATH, skipping build ID injection" >&2
    exit 0
fi

sed -i '' "s/__AOS_BUILD_ID__/${BUILD_ID}/g" "$SW_PATH"
echo "Injected build ID '${BUILD_ID}' into ${SW_PATH}"
