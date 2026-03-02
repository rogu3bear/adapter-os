#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

STRICT=0
FORMAT="text"
MAX_MB="${MAX_TRACKED_FILE_MB:-25}"
MAX_BYTES=$((MAX_MB * 1024 * 1024))
ALLOWLIST_FILE="docs/governance/size-budget-allowlist.txt"

usage() {
  cat <<'USAGE'
Usage: scripts/ci/check_repo_size_budget.sh [--strict] [--format text|json]
Env:
  MAX_TRACKED_FILE_MB (default: 25)
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict) STRICT=1; shift ;;
    --format)
      FORMAT="${2:-}"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$FORMAT" != "text" && "$FORMAT" != "json" ]]; then
  echo "Invalid format: $FORMAT" >&2
  exit 2
fi

TRACKED_FILES=()
while IFS= read -r line; do
  TRACKED_FILES+=("$line")
done < <(git ls-files)

SIZE_ALLOWLIST=()
while IFS= read -r line; do
  SIZE_ALLOWLIST+=("$line")
done < <(grep -Ev '^\s*(#|$)' "$ALLOWLIST_FILE" 2>/dev/null || true)

is_size_allowlisted() {
  local f="$1"
  local item
  for item in "${SIZE_ALLOWLIST[@]}"; do
    [[ "$f" == "$item" ]] && return 0
  done
  return 1
}

oversized=()
binary_disallowed=()
largest_lines=()

for f in "${TRACKED_FILES[@]}"; do
  [[ -f "$f" ]] || continue
  size="$(wc -c < "$f" | tr -d ' ')"
  largest_lines+=("$size|$f")

  if [[ "$size" -gt "$MAX_BYTES" ]]; then
    if ! is_size_allowlisted "$f"; then
      oversized+=("$f")
    fi
  fi

  case "$f" in
    .playwright-cli/*|.playwright-mcp/*|var/*|target/*|target-*/*)
      if command -v file >/dev/null 2>&1; then
        mime="$(file -b --mime-type "$f" 2>/dev/null || echo application/octet-stream)"
        case "$mime" in
          text/*|application/json|application/x-empty|application/xml)
            ;;
          *)
            binary_disallowed+=("$f")
            ;;
        esac
      fi
      ;;
  esac
done

top_largest=()
while IFS= read -r line; do
  top_largest+=("$line")
done < <(printf '%s\n' "${largest_lines[@]}" | sort -t'|' -nrk1,1 | head -n 10)

status="pass"
if [[ ${#oversized[@]} -gt 0 || ${#binary_disallowed[@]} -gt 0 ]]; then
  status="fail"
fi

if [[ "$FORMAT" == "json" ]]; then
  python3 - <<'PY' "$status" "$STRICT" "$MAX_MB" "${oversized[*]:-}" "${binary_disallowed[*]:-}" "${top_largest[*]:-}"
import json,sys
status=sys.argv[1]
strict=bool(int(sys.argv[2]))
max_mb=int(sys.argv[3])
split=lambda s: [] if not s else s.split()
raw_top=split(sys.argv[6])
top=[]
for item in raw_top:
    if '|' in item:
        sz,p=item.split('|',1)
        top.append({"path":p,"size_bytes":int(sz)})
print(json.dumps({
  "check":"repo_size_budget",
  "status":status,
  "strict":strict,
  "max_tracked_file_mb":max_mb,
  "oversized_files":split(sys.argv[4]),
  "disallowed_binary_files":split(sys.argv[5]),
  "largest_tracked_files":top
}, indent=2, sort_keys=True))
PY
else
  echo "=== Repository Size Budget Check ==="
  echo "status: $status"
  echo "max tracked file size: ${MAX_MB}MB"
  if [[ ${#oversized[@]} -gt 0 ]]; then
    echo "Oversized tracked files:" 
    printf '  - %s\n' "${oversized[@]}"
  fi
  if [[ ${#binary_disallowed[@]} -gt 0 ]]; then
    echo "Disallowed binary files in generated/runtime paths:" 
    printf '  - %s\n' "${binary_disallowed[@]}"
  fi
  echo "Top 10 largest tracked files:"
  for item in "${top_largest[@]}"; do
    [[ -n "$item" ]] || continue
    sz="${item%%|*}"
    p="${item#*|}"
    mb=$(python3 - <<PY
s=$sz
print(f"{s/1048576:.2f}")
PY
)
    echo "  - ${p} (${mb}MB)"
  done
fi

if [[ "$status" == "fail" && "$STRICT" -eq 1 ]]; then
  exit 1
fi

exit 0
