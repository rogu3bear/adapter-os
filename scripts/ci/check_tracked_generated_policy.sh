#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

STRICT=0
FORMAT="text"

usage() {
  cat <<'USAGE'
Usage: scripts/ci/check_tracked_generated_policy.sh [--strict] [--format text|json]
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

ALLOWLIST_FILE="docs/governance/generated-artifact-allowlist.json"
POLICY_DOC="docs/governance/GENERATED_ARTIFACT_POLICY.md"

TRACKED_FILES=()
while IFS= read -r line; do
  TRACKED_FILES+=("$line")
done < <(git ls-files)

ALLOWLIST=()
while IFS= read -r line; do
  ALLOWLIST+=("$line")
done < <(python3 - <<'PY' "$ALLOWLIST_FILE"
import json,sys
p=sys.argv[1]
with open(p,encoding='utf-8') as f:
    data=json.load(f)
for item in data.get('generated_artifacts',[]):
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

violations=()
for f in "${TRACKED_FILES[@]}"; do
  case "$f" in
    .playwright-cli/*|.playwright-mcp/*|var/*|target/*|target-*/*)
      if ! is_allowlisted "$f"; then
        violations+=("$f")
      fi
      ;;
  esac
done

missing_doc_refs=()
if [[ -f "$POLICY_DOC" ]]; then
  for item in "${ALLOWLIST[@]}"; do
    if ! rg -Fq -- "$item" "$POLICY_DOC"; then
      missing_doc_refs+=("$item")
    fi
  done
else
  missing_doc_refs+=("$POLICY_DOC")
fi

missing_files=()
for item in "${ALLOWLIST[@]}"; do
  if ! git ls-files --error-unmatch "$item" >/dev/null 2>&1; then
    missing_files+=("$item")
  fi
done

status="pass"
if [[ ${#violations[@]} -gt 0 || ${#missing_doc_refs[@]} -gt 0 || ${#missing_files[@]} -gt 0 ]]; then
  status="fail"
fi

if [[ "$FORMAT" == "json" ]]; then
  python3 - <<'PY' "$status" "$STRICT" "${violations[*]:-}" "${missing_doc_refs[*]:-}" "${missing_files[*]:-}"
import json,sys
status=sys.argv[1]
strict=bool(int(sys.argv[2]))
split=lambda s: [] if not s else s.split()
out={
  "check":"tracked_generated_policy",
  "status":status,
  "strict":strict,
  "violations":split(sys.argv[3]),
  "missing_policy_doc_refs":split(sys.argv[4]),
  "missing_allowlisted_files":split(sys.argv[5]),
}
print(json.dumps(out,indent=2,sort_keys=True))
PY
else
  echo "=== Tracked Generated Policy Check ==="
  echo "status: $status"
  if [[ ${#violations[@]} -gt 0 ]]; then
    echo "Unauthorized tracked files in banned generated/log locations:"
    printf '  - %s\n' "${violations[@]}"
  fi
  if [[ ${#missing_doc_refs[@]} -gt 0 ]]; then
    echo "Allowlisted artifacts missing documentation references in $POLICY_DOC:"
    printf '  - %s\n' "${missing_doc_refs[@]}"
  fi
  if [[ ${#missing_files[@]} -gt 0 ]]; then
    echo "Allowlisted artifacts not tracked:"
    printf '  - %s\n' "${missing_files[@]}"
  fi
fi

if [[ "$status" == "fail" && "$STRICT" -eq 1 ]]; then
  exit 1
fi

exit 0
