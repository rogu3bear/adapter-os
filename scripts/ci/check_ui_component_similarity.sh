#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

python3 scripts/ui_component_similarity.py \
  --threshold 0.80 \
  --exclude-file-suffix components/icons.rs \
  --max-qualifying 8
