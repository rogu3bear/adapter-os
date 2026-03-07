#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

LOCAL_RELEASE_MODE=prod \
LOCAL_RELEASE_RUN_INFERENCE=1 \
LOCAL_REQUIRED_CLIPPY_SCOPE=all-targets \
LOCAL_RELEASE_GOVERNANCE_MODE=enforce \
AOS_REQUIRE_RELEASE_BINARIES=1 \
bash scripts/ci/local_release_gate.sh "$@"
