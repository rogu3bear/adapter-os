#!/usr/bin/env bash
# CI helper: render deterministic graduation receipts from governance audit report.
#
# Usage:
#   bash scripts/ci/render_governance_graduation_receipts.sh \
#     --report var/evidence/governance-graduation-<UTCSTAMP>/report.json \
#     --output-dir var/evidence/governance-graduation-<UTCSTAMP>
#
# Exit codes:
#   0  success
#   30 misconfigured
#   40 runtime/tooling error
#   2  usage

set -euo pipefail
export LC_ALL=C
export LANG=C

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

usage() {
  cat <<'USAGE'
Usage: render_governance_graduation_receipts.sh --report <report.json> [--output-dir <dir>]

Generates deterministic receipts:
  - graduation-matrix.txt
  - routing-actions.txt
  - routing-summary.txt
USAGE
}

emit_status() {
  local status="$1"
  local reason="$2"
  local detail="${3:-}"
  printf 'status=%s reason=%s' "$status" "$reason"
  if [[ -n "$detail" ]]; then
    printf ' detail=%s' "$detail"
  fi
  printf '\n'
}

REPORT=""
OUTPUT_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --report)
      REPORT="${2:-}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:-}"
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

if [[ -z "$REPORT" ]]; then
  echo "::error::--report is required" >&2
  usage >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  emit_status "error" "missing_tool_jq"
  exit 40
fi

if [[ ! -f "$REPORT" ]]; then
  emit_status "misconfigured" "report_missing" "report=${REPORT}"
  exit 30
fi

if ! jq -e '.results | type == "array"' "$REPORT" >/dev/null 2>&1; then
  emit_status "misconfigured" "invalid_report_schema" "report=${REPORT}"
  exit 30
fi

if [[ -z "$OUTPUT_DIR" ]]; then
  OUTPUT_DIR="$(dirname "$REPORT")"
fi

mkdir -p "$OUTPUT_DIR"
MATRIX_OUT="$OUTPUT_DIR/graduation-matrix.txt"
ROUTING_OUT="$OUTPUT_DIR/routing-actions.txt"
SUMMARY_OUT="$OUTPUT_DIR/routing-summary.txt"

jq -r '
  .results
  | sort_by(.id)
  | .[]
  | [
      ("id=" + .id),
      ("repo=" + .repo),
      ("branch=" + .branch),
      ("final_outcome=" + .outcome),
      ("raw_outcome=" + .raw_outcome),
      ("strict=" + (.strict // "unknown")),
      ("missing_contexts=" + ((.missing_contexts | length) | tostring)),
      ("endpoint_status=" + (.endpoint_status // "unknown")),
      ("approved_exception=" + (if .approved_exception_reason then "yes" else "no" end))
    ]
  | join(" ")
' "$REPORT" > "$MATRIX_OUT"

jq -r '
  .results
  | sort_by(.id)
  | .[]
  | . as $r
  | ($r.outcome) as $o
  | (
      if $o == "compliant" then "retain"
      elif $o == "drifted" then "remediate"
      elif $o == "blocked_external" then "escalate_blocker"
      elif $o == "approved_exception" then "review_exception"
      else "unknown"
      end
    ) as $action
  | [
      ("id=" + $r.id),
      ("outcome=" + $o),
      ("action=" + $action),
      ("reason=" + (($r.approved_exception_reason // $r.raw_reason // "n/a") | gsub("\\s+"; " ")))
    ]
  | join(" ")
' "$REPORT" > "$ROUTING_OUT"

if rg -q 'action=unknown' "$ROUTING_OUT"; then
  emit_status "misconfigured" "unknown_outcome_action" "routing=${ROUTING_OUT}"
  exit 30
fi

{
  echo "report=${REPORT}"
  echo "matrix=${MATRIX_OUT}"
  echo "routing=${ROUTING_OUT}"
  jq -r '.summary | "targets=\(.target_count) compliant=\(.compliant) drifted=\(.drifted) blocked_external=\(.blocked_external) approved_exception=\(.approved_exception)"' "$REPORT"
  jq -r '.results | group_by(.outcome) | map({outcome: .[0].outcome, count: length}) | sort_by(.outcome)[] | "outcome=" + .outcome + " count=" + (.count|tostring)' "$REPORT"
} > "$SUMMARY_OUT"

emit_status "ok" "rendered" "matrix=${MATRIX_OUT} routing=${ROUTING_OUT}"
exit 0
