#!/usr/bin/env bash
# Dataset cleanup for adapterOS
#
# Removes redundant/junk datasets from the control-plane DB:
# - Probe datasets (probe-50, 100, 200, 500, 1000)
# - Gold pack originals (keeps _ingest versions only)
#
# Requires: server running (use AOS_DEV_NO_AUTH=1 ./start for dev, or aosctl auth login)
#
# Usage:
#   ./scripts/cleanup-datasets.sh           # dry run
#   ./scripts/cleanup-datasets.sh --apply  # actually delete

set -euo pipefail

ROOT_DIR="${AOS_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
API_BASE="${AOS_API_BASE:-http://127.0.0.1:18080}"
APPLY=false

for arg in "$@"; do
    case "$arg" in
        --apply) APPLY=true ;;
        -h|--help)
            echo "Usage: $0 [--apply]"
            echo "  --apply  Actually delete datasets (default: dry run)"
            exit 0
            ;;
    esac
done

cd "$ROOT_DIR"
DB="${AOS_DATABASE_URL:-sqlite://var/aos-cp.sqlite3}"
DB_PATH="${DB#sqlite:}"
DB_PATH="${DB_PATH#//}"
[[ "$DB_PATH" != /* ]] && DB_PATH="$ROOT_DIR/$DB_PATH"

# Get IDs to delete: probes (probe-*) and gold pack originals (part_XX without _ingest)
TO_DELETE=$(sqlite3 "$DB_PATH" "
  SELECT id FROM training_datasets
  WHERE name LIKE 'probe-%'
     OR (name LIKE 'adapteros_gold_pack_v1_part_%' AND name NOT LIKE '%_ingest');
" 2>/dev/null || true)

COUNT=0
for id in $TO_DELETE; do
    name=$(sqlite3 "$DB_PATH" "SELECT name FROM training_datasets WHERE id='$id';" 2>/dev/null || true)
    if [[ -n "$name" ]]; then
        if [[ "$APPLY" == true ]]; then
            code=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "${API_BASE}/v1/datasets/$id" 2>/dev/null || echo "000")
            if [[ "$code" == "204" ]]; then
                echo "Deleted: $id ($name)"
                ((COUNT++)) || true
            else
                echo "Failed to delete: $id ($name) (HTTP $code)" >&2
            fi
        else
            echo "[dry-run] would delete: $id ($name)"
            ((COUNT++)) || true
        fi
    fi
done

if [[ $COUNT -eq 0 ]]; then
    echo "Nothing to clean."
else
    if [[ "$APPLY" == true ]]; then
        echo "Cleanup complete ($COUNT dataset(s) deleted)."
    else
        echo "Dry run: $COUNT dataset(s) would be deleted. Run with --apply to delete."
    fi
fi
