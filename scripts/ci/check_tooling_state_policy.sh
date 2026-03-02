#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

STRICT=0
FORMAT="text"

usage() {
  cat <<'USAGE'
Usage: scripts/ci/check_tooling_state_policy.sh [--strict] [--format text|json]
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

ALLOWLIST_FILE="docs/governance/tooling-config-allowlist.json"
TRACKED_FILES=()
while IFS= read -r line; do
  TRACKED_FILES+=("$line")
done < <(git ls-files)

ALLOWLIST=()
while IFS= read -r line; do
  ALLOWLIST+=("$line")
done < <(python3 - <<'PY' "$ALLOWLIST_FILE"
import json,sys
with open(sys.argv[1],encoding='utf-8') as f:
    data=json.load(f)
for item in data.get('allowed_paths',[]):
    print(item)
PY
)

is_allowlisted() {
  local f="$1"
  local item
  for item in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$item" ]] && return 0
  done
  return 1
}

is_tooling_path() {
  local f="$1"
  case "$f" in
    .mcp.json|.playwright-cli/*|.playwright-mcp/*|.codex/*|.claude/*|.agents/*|.harmony/*|.integrator/*|.worker_logs/*|.cursor/*|.vscode/*)
      return 0
      ;;
  esac
  return 1
}

violations=()
for f in "${TRACKED_FILES[@]}"; do
  if is_tooling_path "$f"; then
    if ! is_allowlisted "$f"; then
      violations+=("$f")
    fi
  fi
done

status="pass"
if [[ ${#violations[@]} -gt 0 ]]; then
  status="fail"
fi

if [[ "$FORMAT" == "json" ]]; then
  python3 - <<'PY' "$status" "$STRICT" "${violations[*]:-}"
import json,sys
status=sys.argv[1]
strict=bool(int(sys.argv[2]))
violations=[] if not sys.argv[3] else sys.argv[3].split()
print(json.dumps({
  "check":"tooling_state_policy",
  "status":status,
  "strict":strict,
  "violations":violations
}, indent=2, sort_keys=True))
PY
else
  echo "=== Tooling State Policy Check ==="
  echo "status: $status"
  if [[ ${#violations[@]} -gt 0 ]]; then
    echo "Tracked tooling paths outside allowlist:" 
    printf '  - %s\n' "${violations[@]}"
  fi
fi

if [[ "$status" == "fail" && "$STRICT" -eq 1 ]]; then
  exit 1
fi

exit 0
