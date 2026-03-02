#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

DRY_RUN=0
STRICT=0

usage() {
  cat <<'USAGE'
Usage: scripts/cleanup/local_tooling_artifacts.sh [--dry-run] [--strict]

Removes local transient tooling artifacts:
  - .playwright-cli/*
  - .playwright-mcp/*
  - target-pr*
  - var/reports/*
  - var/playwright/runs/*
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=1; shift ;;
    --strict) STRICT=1; shift ;;
    --help|-h) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

paths=(
  ".playwright-cli"
  ".playwright-mcp"
  "var/reports"
  "var/playwright/runs"
)

removed=0
report_remove() {
  local p="$1"
  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "[dry-run] remove $p"
  else
    rm -rf "$p"
    echo "removed $p"
  fi
  removed=$((removed + 1))
}

for p in "${paths[@]}"; do
  if [[ -e "$p" ]]; then
    report_remove "$p"
  elif [[ "$STRICT" -eq 1 ]]; then
    echo "[strict] missing path: $p" >&2
  fi
done

shopt -s nullglob
for p in target-pr*; do
  [[ -e "$p" ]] || continue
  report_remove "$p"
done
shopt -u nullglob

echo "done: removed_entries=$removed dry_run=$DRY_RUN strict=$STRICT"
