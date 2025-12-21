#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if command -v rg >/dev/null 2>&1; then
  matches=$(rg -n "\\.infer\\(" crates/adapteros-server-api/src \
    --type rust \
    -g '!*inference_core.rs' \
    -g '!*uds_client.rs' || true)
else
  matches=$(grep -RIn "\\.infer(" crates/adapteros-server-api/src \
    --include="*.rs" \
    --exclude="inference_core.rs" \
    --exclude="uds_client.rs" || true)
fi

if [[ -n "$matches" ]]; then
  echo "Inference bypass detected: .infer( call outside inference_core.rs or uds_client.rs" >&2
  echo "$matches" >&2
  exit 1
fi
