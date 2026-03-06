#!/usr/bin/env bash

set -euo pipefail

REAL_RUSTC="$1"
shift

if [[ "$(uname)" == "Darwin" ]] && ! command -v cmake >/dev/null 2>&1; then
    cat >&2 <<'EOF'

❌ adapterOS build preflight failed.
Missing required build tool: `cmake`.

Install it with:
  brew install cmake

This workspace includes native dependencies that invoke cmake during Cargo builds.
EOF
    exit 1
fi

if [[ -n "${AOS_INNER_RUSTC_WRAPPER:-}" ]]; then
    exec "${AOS_INNER_RUSTC_WRAPPER}" "${REAL_RUSTC}" "$@"
fi

exec "${REAL_RUSTC}" "$@"
