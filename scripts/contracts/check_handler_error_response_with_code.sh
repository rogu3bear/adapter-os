#!/usr/bin/env bash
# check_handler_error_response_with_code.sh — incremental guard that blocks
# newly introduced handler ErrorResponse::new(...) constructions without an
# explicit .with_code(...).
#
# Exit code 0 = no new violations.
# Exit code 1 = at least one newly introduced violation.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

BASE_REF="${BASE_REF:-}"
LOOKAHEAD_LINES="${LOOKAHEAD_LINES:-40}"

usage() {
  cat <<'EOF'
Usage:
  scripts/contracts/check_handler_error_response_with_code.sh [--base-ref <git-ref>] [--help]

Description:
  Incremental CI guard for handler error responses.
  Checks only newly introduced handler ErrorResponse::new(...) constructions in:
    crates/adapteros-server-api/src/handlers.rs
    crates/adapteros-server-api/src/handlers/**

  A violation is reported when the constructor statement does not include an
  explicit .with_code(...).
  The diff scope is computed from merge-base(BASE_REF, HEAD) to current working tree.

Base ref resolution order:
  1. --base-ref <git-ref>
  2. BASE_REF environment variable
  3. origin/$GITHUB_BASE_REF (for pull_request events)
  4. first existing ref of: origin/main, origin/master, main, master

Environment:
  LOOKAHEAD_LINES   Max lines to inspect from each new constructor site (default: 40)

Examples:
  scripts/contracts/check_handler_error_response_with_code.sh
  scripts/contracts/check_handler_error_response_with_code.sh --base-ref origin/main
  BASE_REF=main scripts/contracts/check_handler_error_response_with_code.sh
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-ref)
      BASE_REF="${2:-}"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$BASE_REF" ]] && [[ "${GITHUB_EVENT_NAME:-}" == "pull_request" ]] && [[ -n "${GITHUB_BASE_REF:-}" ]]; then
  if git rev-parse --verify "origin/${GITHUB_BASE_REF}" >/dev/null 2>&1; then
    BASE_REF="origin/${GITHUB_BASE_REF}"
  fi
fi

if [[ -z "$BASE_REF" ]]; then
  for candidate in origin/main origin/master main master; do
    if git rev-parse --verify "$candidate" >/dev/null 2>&1; then
      BASE_REF="$candidate"
      break
    fi
  done
fi

if [[ -z "$BASE_REF" ]]; then
  echo "FAIL: unable to resolve diff base ref." >&2
  echo "Pass --base-ref <git-ref> or set BASE_REF." >&2
  exit 1
fi

if ! MERGE_BASE="$(git merge-base "$BASE_REF" HEAD 2>/dev/null)"; then
  echo "FAIL: unable to compute merge-base for '$BASE_REF' and HEAD." >&2
  exit 1
fi

if ! [[ "$LOOKAHEAD_LINES" =~ ^[0-9]+$ ]] || [[ "$LOOKAHEAD_LINES" -lt 1 ]]; then
  echo "FAIL: LOOKAHEAD_LINES must be a positive integer (got '$LOOKAHEAD_LINES')." >&2
  exit 1
fi

constructor_has_with_code() {
  local file="$1"
  local line="$2"
  local end_line=$((line + LOOKAHEAD_LINES))
  local statement

  statement="$(
    awk -v start="$line" -v end="$end_line" '
      NR < start { next }
      NR > end { exit }
      {
        current = $0
        sub(/[[:space:]]*\/\/.*$/, "", current)
        printf "%s\n", current
        if (current ~ /;[[:space:]]*$/ || current ~ /\)[[:space:]]*,[[:space:]]*$/) {
          exit
        }
      }
    ' "$file"
  )"

  [[ "$statement" == *".with_code("* ]]
}

constructor_sites=()
while IFS= read -r site; do
  [[ -n "$site" ]] || continue
  constructor_sites+=("$site")
done < <(
  git diff --unified=0 --no-color "${MERGE_BASE}" -- \
    crates/adapteros-server-api/src/handlers.rs \
    crates/adapteros-server-api/src/handlers \
    | awk '
      /^\+\+\+ b\// {
        file = substr($0, 7)
        next
      }
      /^@@ / {
        line = 0
        if ($0 ~ /\+[0-9]+/) {
          header = $0
          sub(/^.*\+/, "", header)
          sub(/,.*/, "", header)
          sub(/ .*/, "", header)
          line = header + 0
        }
        next
      }
      file == "" || line == 0 { next }
      /^\+/ && $0 !~ /^\+\+\+/ {
        if ($0 ~ /ErrorResponse::new[[:space:]]*\(/) {
          printf "%s:%d\n", file, line
        }
        line++
        next
      }
      /^-/ && $0 !~ /^---/ { next }
      {
        line++
      }
    ' | sort -u
)

if [[ "${#constructor_sites[@]}" -eq 0 ]]; then
  echo "=== Handler ErrorResponse with_code Guard: PASSED (no new constructors) ==="
  exit 0
fi

declare -a violations=()

for site in "${constructor_sites[@]}"; do
  file="${site%:*}"
  line="${site##*:}"

  [[ -f "$file" ]] || continue

  if ! constructor_has_with_code "$file" "$line"; then
    snippet="$(sed -n "${line}p" "$file" | sed 's/^[[:space:]]*//')"
    violations+=("${file}:${line} ${snippet}")
  fi
done

if [[ "${#violations[@]}" -gt 0 ]]; then
  echo "FAIL: Newly introduced handler ErrorResponse::new(...) constructions are missing .with_code(...):"
  for entry in "${violations[@]}"; do
    echo "  - ${entry}"
  done
  echo ""
  echo "Fix: chain .with_code(\"ERROR_CODE\") on each newly introduced constructor."
  echo "Tip: use --base-ref <git-ref> (or BASE_REF) to control incremental comparison."
  exit 1
fi

echo "=== Handler ErrorResponse with_code Guard: PASSED ==="
