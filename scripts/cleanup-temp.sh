#!/usr/bin/env bash
# Temp file cleanup for adapterOS (path hygiene)
#
# Removes one-off tmp artifacts and crates/var dirs per CONTRIBUTING.md.
# Safe to run manually; var/ is gitignored.

set -euo pipefail

ROOT_DIR="${AOS_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
VAR_DIR="${AOS_VAR_DIR:-$ROOT_DIR/var}"
DRY_RUN=false

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Remove temporary files and path-hygiene violations.

OPTIONS:
    -n, --dry-run    Show what would be removed without deleting
    -h, --help       Show this help message

CLEANUP SCOPE:
    - var/tmp_*.json, var/tmp_*.txt (one-off debug artifacts)
    - var/tmp, var/datasets/temp, var/demo-playwright/tmp (nested tmp dirs)
    - crates/*/var (path hygiene: never create var/ inside crates)

See CONTRIBUTING.md "Path Hygiene" for rationale.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -n|--dry-run) DRY_RUN=true; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown option: $1"; usage; exit 1 ;;
    esac
done

cd "$ROOT_DIR"

removed=0

# 1. var/ tmp artifacts (tmp_*.json, tmp_*.txt)
while IFS= read -r -d '' f; do
    if [[ "$DRY_RUN" == true ]]; then
        echo "[dry-run] would remove $f"
    else
        rm -f "$f"
        echo "removed $f"
    fi
    ((removed++)) || true
done < <(find "$VAR_DIR" -maxdepth 1 -type f \( -name "tmp_*.json" -o -name "tmp_*.txt" \) -print0 2>/dev/null || true)

# 2. Nested tmp dirs under var/
for d in "$VAR_DIR/tmp" "$VAR_DIR/datasets/temp" "$VAR_DIR/demo-playwright/tmp"; do
    if [[ -d "$d" ]]; then
        if [[ "$DRY_RUN" == true ]]; then
            echo "[dry-run] would remove $d"
        else
            rm -rf "$d"
            echo "removed $d"
        fi
        ((removed++)) || true
    fi
done

# 3. crates/var (path hygiene)
while IFS= read -r -d '' d; do
    if [[ "$DRY_RUN" == true ]]; then
        echo "[dry-run] would remove $d"
    else
        rm -rf "$d"
        echo "removed $d"
    fi
    ((removed++)) || true
done < <(find ./crates -type d -name "var" -not -path "*/target/*" -print0 2>/dev/null || true)

if [[ $removed -eq 0 ]]; then
    echo "Nothing to clean."
else
    echo "Cleanup complete ($removed item(s))."
fi
