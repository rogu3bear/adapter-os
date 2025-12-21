#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINE_LIMIT="${LINE_LIMIT:-200}"
PATHS=("crates" "tests")

cd "$ROOT_DIR"

candidates=()
if command -v rg >/dev/null 2>&1; then
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    candidates+=("$line")
  done < <(
    rg -l -g '*.rs' '(^|[^A-Za-z0-9_:])(info|warn|error|debug|trace)!\(' "${PATHS[@]}" || true
  )
else
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    candidates+=("$line")
  done < <(
    grep -R --line-number --include='*.rs' \
      --exclude-dir='target' \
      --exclude-dir='node_modules' \
      --exclude-dir='.git' \
      -E '(^|[^A-Za-z0-9_:])(info|warn|error|debug|trace)!\(' \
      "${PATHS[@]}" 2>/dev/null | cut -d: -f1 | sort -u
  )
fi

missing=()

has_unqualified_macro_in_code() {
  awk '
    BEGIN { in_block = 0 }
    {
      line = $0
      if (in_block) {
        end = index(line, "*/")
        if (end == 0) {
          next
        }
        line = substr(line, end + 2)
        in_block = 0
      }

      while ((start = index(line, "/*")) > 0) {
        end = index(substr(line, start + 2), "*/")
        if (end == 0) {
          line = substr(line, 1, start - 1)
          in_block = 1
          break
        }
        line = substr(line, 1, start - 1) substr(line, start + 2 + end)
      }

      sub(/\/\/.*/, "", line)
      if (line ~ /(^|[^A-Za-z0-9_:])(info|warn|error|debug|trace)!\(/) {
        exit 0
      }
    }
    END { exit 1 }
  ' "$1"
}

for file in "${candidates[@]}"; do
  if [ ! -f "$file" ]; then
    continue
  fi

  if ! has_unqualified_macro_in_code "$file"; then
    continue
  fi

  if command -v rg >/dev/null 2>&1; then
    if ! sed -n "1,${LINE_LIMIT}p" "$file" | rg -q '^\s*use\s+.*\btracing::'; then
      missing+=("$file")
    fi
  else
    if ! sed -n "1,${LINE_LIMIT}p" "$file" | grep -Eq '^\s*use\s+.*\btracing::'; then
      missing+=("$file")
    fi
  fi
done

if [ "${#missing[@]}" -eq 0 ]; then
  echo "No missing tracing imports detected."
  exit 0
fi

echo ""
echo "Missing Tracing Imports"
echo ""
echo "Multiple files use tracing macros without importing them explicitly."
echo "Files with logging but no tracing import visible in first ${LINE_LIMIT} lines:"
for file in "${missing[@]}"; do
  echo "- ${file#./}"
done
exit 1
