#!/usr/bin/env bash
# CI Gate: validate governance drift target manifest shape and deterministic semantics.
#
# Usage:
#   bash scripts/ci/validate_governance_target_manifest.sh \
#     --manifest docs/governance/target-manifest.json
#
# Exit codes:
#   0  valid
#   30 misconfigured
#   40 error
#   2  usage

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
export LC_ALL=C
export LANG=C

MANIFEST="docs/governance/target-manifest.json"

usage() {
  cat <<'USAGE'
Usage: validate_governance_target_manifest.sh [--manifest <path>]

Validates:
  - schema_version and canonical_policy fields
  - required_contexts non-empty and unique
  - targets non-empty with unique id and repo+branch pairs
  - target exception schema

Status model + exit codes:
  valid         (0)
  misconfigured (30)
  error         (40)
USAGE
}

emit_status() {
  local status="$1"
  local reason="$2"
  local detail="${3:-}"

  printf 'status=%s manifest=%s reason=%s' "$status" "$MANIFEST" "$reason"
  if [[ -n "$detail" ]]; then
    printf ' detail=%s' "$detail"
  fi
  printf '\n'
}

sanitize_detail() {
  printf '%s' "$1" \
    | tr '\n' ' ' \
    | sed -E 's/[[:space:]]+/ /g' \
    | cut -c1-240
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest)
      MANIFEST="${2:-}"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "::error::Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if ! command -v jq >/dev/null 2>&1; then
  emit_status "error" "missing_tool_jq"
  exit 40
fi

if [[ ! -f "$MANIFEST" ]]; then
  emit_status "misconfigured" "manifest_missing"
  exit 30
fi

if ! jq -e . "$MANIFEST" >/dev/null 2>&1; then
  emit_status "misconfigured" "manifest_json_invalid"
  exit 30
fi

errors=()
add_error() {
  errors+=("$1")
}

schema_version="$(jq -r '.schema_version // empty' "$MANIFEST")"
if [[ "$schema_version" != "1" ]]; then
  add_error "schema_version must be 1"
fi

if ! jq -e '.canonical_policy.name | strings | length > 0' "$MANIFEST" >/dev/null; then
  add_error "canonical_policy.name must be a non-empty string"
fi

if ! jq -e '.canonical_policy.strict_required | type == "boolean"' "$MANIFEST" >/dev/null; then
  add_error "canonical_policy.strict_required must be boolean"
fi

canonical_count="$(jq '.canonical_policy.required_contexts // [] | length' "$MANIFEST")"
if [[ "$canonical_count" -eq 0 ]]; then
  add_error "canonical_policy.required_contexts must contain at least one context"
fi

canonical_unique="$(jq '[.canonical_policy.required_contexts // [] | .[]] | unique | length' "$MANIFEST")"
if [[ "$canonical_count" -ne "$canonical_unique" ]]; then
  add_error "canonical_policy.required_contexts contains duplicates"
fi

target_count="$(jq '.targets // [] | length' "$MANIFEST")"
if [[ "$target_count" -eq 0 ]]; then
  add_error "targets must contain at least one target"
fi

dup_ids="$(jq -r '[.targets[]?.id // empty] | group_by(.)[] | select(length > 1) | .[0]' "$MANIFEST")"
if [[ -n "$dup_ids" ]]; then
  while IFS= read -r id; do
    [[ -z "$id" ]] && continue
    add_error "duplicate target id: $id"
  done <<< "$dup_ids"
fi

dup_repo_branch="$(jq -r '[.targets[]? | ((.repo // "") + "#" + (.branch // ""))] | group_by(.)[] | select(length > 1) | .[0]' "$MANIFEST")"
if [[ -n "$dup_repo_branch" ]]; then
  while IFS= read -r pair; do
    [[ -z "$pair" ]] && continue
    add_error "duplicate repo+branch target: $pair"
  done <<< "$dup_repo_branch"
fi

for ((i=0; i<target_count; i++)); do
  t_path=".targets[$i]"
  t_id="$(jq -r "$t_path.id // empty" "$MANIFEST")"
  t_repo="$(jq -r "$t_path.repo // empty" "$MANIFEST")"
  t_branch="$(jq -r "$t_path.branch // empty" "$MANIFEST")"
  t_probe="$(jq -r "$t_path.probe_context // empty" "$MANIFEST")"

  if [[ -z "$t_id" ]]; then
    add_error "targets[$i].id is required"
  fi
  if [[ -z "$t_repo" ]]; then
    add_error "targets[$i].repo is required"
  fi
  if [[ -z "$t_branch" ]]; then
    add_error "targets[$i].branch is required"
  fi

  t_context_count="$(jq "$t_path.required_contexts // [] | length" "$MANIFEST")"
  if jq -e "$t_path | has(\"required_contexts\")" "$MANIFEST" >/dev/null; then
    if [[ "$t_context_count" -eq 0 ]]; then
      add_error "targets[$i].required_contexts must not be empty when provided"
    fi
    t_context_unique="$(jq "[$t_path.required_contexts // [] | .[]] | unique | length" "$MANIFEST")"
    if [[ "$t_context_count" -ne "$t_context_unique" ]]; then
      add_error "targets[$i].required_contexts contains duplicates"
    fi
  fi

  if ! jq -e "$t_path.approved_exceptions // [] | type == \"array\"" "$MANIFEST" >/dev/null; then
    add_error "targets[$i].approved_exceptions must be an array when provided"
  fi

  exception_count="$(jq "$t_path.approved_exceptions // [] | length" "$MANIFEST")"
  for ((j=0; j<exception_count; j++)); do
    ex_type="$(jq -r "$t_path.approved_exceptions[$j].type // empty" "$MANIFEST")"
    ex_reason="$(jq -r "$t_path.approved_exceptions[$j].reason // empty" "$MANIFEST")"
    if [[ -z "$ex_type" ]]; then
      add_error "targets[$i].approved_exceptions[$j].type is required"
    fi
    if [[ -z "$ex_reason" ]]; then
      add_error "targets[$i].approved_exceptions[$j].reason is required"
    fi
  done

  if [[ -n "$t_probe" ]]; then
    if ! jq -e --arg probe "$t_probe" --argjson idx "$i" '
      . as $root
      | ($root.targets[$idx].required_contexts // $root.canonical_policy.required_contexts // [])
      | index($probe) != null
    ' "$MANIFEST" >/dev/null; then
      add_error "targets[$i].probe_context must exist in effective required contexts"
    fi
  fi
done

if [[ ${#errors[@]} -gt 0 ]]; then
  emit_status "misconfigured" "validation_failed" "error_count=${#errors[@]}"
  for err in "${errors[@]}"; do
    printf 'error=%s\n' "$(sanitize_detail "$err")"
  done
  exit 30
fi

digest=""
if command -v shasum >/dev/null 2>&1; then
  digest="$(shasum -a 256 "$MANIFEST" | awk '{print $1}')"
fi

emit_status "valid" "validated" "target_count=${target_count} canonical_context_count=${canonical_count} digest=${digest:-na}"
jq -r '.targets | sort_by(.id)[] | "target=" + .id + " repo=" + .repo + " branch=" + .branch' "$MANIFEST"
exit 0
