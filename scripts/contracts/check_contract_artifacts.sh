#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

scripts/contracts/generate_contract_artifacts.py --check

echo "=== Contract Artifact Check: PASSED ==="
