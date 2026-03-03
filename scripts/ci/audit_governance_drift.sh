#!/usr/bin/env bash
# CI Gate: deterministic, read-only governance drift audit.
#
# Usage:
#   bash scripts/ci/audit_governance_drift.sh \
#     --manifest docs/governance/target-manifest.json \
#     --fail-on drifted
#
# Exit codes:
#   0  audit complete and no fail-on outcomes
#   1  audit complete but fail-on outcomes found
#   30 misconfigured input/manifest
#   40 runtime/tooling failure
#   2  usage

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
export LC_ALL=C
export LANG=C

MANIFEST="docs/governance/target-manifest.json"
OUTPUT_DIR=""
FAIL_ON="drifted"

usage() {
  cat <<'USAGE'
Usage: audit_governance_drift.sh [--manifest <path>] [--output-dir <dir>] [--fail-on <csv>]

Read-only audit for required_status_checks policy drift.

Options:
  --manifest   Target manifest path (default: docs/governance/target-manifest.json)
  --output-dir Output evidence directory (default: var/evidence/governance-drift-<UTCSTAMP>)
  --fail-on    Comma-separated final outcomes that should fail the command
               (default: drifted)

Final outcome classes:
  compliant | drifted | blocked_external | approved_exception
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
    --output-dir)
      OUTPUT_DIR="${2:-}"
      shift 2
      ;;
    --fail-on)
      FAIL_ON="${2:-}"
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

for tool in jq rg gh; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    emit_status "error" "missing_tool_${tool}"
    exit 40
  fi
done

if [[ -z "$OUTPUT_DIR" ]]; then
  utcstamp="$(date -u +%Y%m%dT%H%M%SZ)"
  OUTPUT_DIR="var/evidence/governance-drift-${utcstamp}"
fi

mkdir -p "$OUTPUT_DIR"

AUDIT_LOG="$OUTPUT_DIR/audit.log"
VALIDATION_LOG="$OUTPUT_DIR/manifest-validation.txt"
REPORT_JSON="$OUTPUT_DIR/report.json"
REPORT_TXT="$OUTPUT_DIR/report.txt"
RESULTS_NDJSON="$OUTPUT_DIR/results.ndjson"

log() {
  printf '%s\n' "$*" | tee -a "$AUDIT_LOG" >/dev/null
}

: > "$AUDIT_LOG"
: > "$RESULTS_NDJSON"

log "governance_drift_audit:start manifest=$MANIFEST output_dir=$OUTPUT_DIR fail_on=$FAIL_ON"

if ! bash scripts/ci/validate_governance_target_manifest.sh --manifest "$MANIFEST" > "$VALIDATION_LOG" 2>&1; then
  log "governance_drift_audit:manifest_validation_failed"
  cat "$VALIDATION_LOG" >> "$AUDIT_LOG"
  emit_status "misconfigured" "manifest_validation_failed" "validation_log=$VALIDATION_LOG"
  exit 30
fi

cat "$VALIDATION_LOG" >> "$AUDIT_LOG"

canonical_contexts_json="$(jq -c '.canonical_policy.required_contexts' "$MANIFEST")"
target_count="$(jq '.targets | length' "$MANIFEST")"

if [[ "$target_count" -eq 0 ]]; then
  emit_status "misconfigured" "manifest_targets_empty"
  exit 30
fi

jq -c '.targets | sort_by(.id)[]' "$MANIFEST" > "$OUTPUT_DIR/targets.ndjson"

while IFS= read -r target_json; do
  id="$(jq -r '.id' <<<"$target_json")"
  repo="$(jq -r '.repo' <<<"$target_json")"
  branch="$(jq -r '.branch' <<<"$target_json")"

  safe_id="$(printf '%s' "$id" | tr -cs 'A-Za-z0-9._-' '_')"

  expected_contexts_json="$(jq -c --argjson canonical "$canonical_contexts_json" '
    if (.required_contexts // [] | length) > 0 then
      .required_contexts
    else
      $canonical
    end | sort | unique
  ' <<<"$target_json")"

  probe_context="$(jq -r '.probe_context // empty' <<<"$target_json")"
  if [[ -z "$probe_context" ]]; then
    probe_context="$(jq -r '.[0]' <<<"$expected_contexts_json")"
  fi

  log "target:start id=$id repo=$repo branch=$branch probe_context=$probe_context"

  preflight_output=""
  preflight_exit=0
  if preflight_output="$(bash scripts/ci/check_governance_preflight.sh --repo "$repo" --branch "$branch" --required-context "$probe_context" 2>&1)"; then
    preflight_exit=0
  else
    preflight_exit=$?
  fi
  printf '%s\n' "$preflight_output" > "$OUTPUT_DIR/preflight-${safe_id}.log"

  preflight_status="$(printf '%s' "$preflight_output" | sed -n 's/.*status=\([^ ]*\).*/\1/p' | head -1)"
  if [[ -z "$preflight_status" ]]; then
    preflight_status="unknown"
  fi

  endpoint="repos/${repo}/branches/${branch}/protection/required_status_checks"
  api_output=""
  api_exit=0
  if api_output="$(gh api "$endpoint" 2>&1)"; then
    api_exit=0
    printf '%s\n' "$api_output" > "$OUTPUT_DIR/protection-${safe_id}.json"
  else
    api_exit=$?
    printf '%s\n' "$api_output" > "$OUTPUT_DIR/protection-${safe_id}.err"
  fi

  endpoint_status=""
  if [[ "$api_exit" -eq 0 ]]; then
    endpoint_status="200"
  else
    endpoint_status="$(printf '%s' "$api_output" | sed -n 's/.*HTTP \([0-9][0-9][0-9]\).*/\1/p' | head -1)"
    if [[ -z "$endpoint_status" ]]; then
      endpoint_status="unknown"
    fi
  fi

  observed_contexts_json='[]'
  strict_state="unknown"
  missing_contexts_json='[]'

  if [[ "$api_exit" -eq 0 ]]; then
    observed_contexts_json="$(printf '%s' "$api_output" | jq -c '.contexts // [] | sort | unique')"
    strict_state="$(printf '%s' "$api_output" | jq -r 'if (.strict | type) == "boolean" then (if .strict then "true" else "false" end) else "unknown" end')"
    missing_contexts_json="$(jq -nc --argjson expected "$expected_contexts_json" --argjson observed "$observed_contexts_json" '$expected - $observed')"
  fi

  raw_outcome="drifted"
  raw_reason="unknown"

  if [[ "$preflight_exit" -eq 20 || "$endpoint_status" == "403" ]]; then
    raw_outcome="blocked_external"
    raw_reason="http_403"
  elif [[ "$api_exit" -eq 0 ]]; then
    missing_count="$(jq 'length' <<<"$missing_contexts_json")"
    if [[ "$strict_state" == "true" && "$missing_count" -eq 0 ]]; then
      raw_outcome="compliant"
      raw_reason="required_contexts_present_and_strict_true"
    elif [[ "$missing_count" -gt 0 ]]; then
      raw_outcome="drifted"
      raw_reason="missing_required_contexts"
    elif [[ "$strict_state" != "true" ]]; then
      raw_outcome="drifted"
      raw_reason="strict_not_true"
    else
      raw_outcome="drifted"
      raw_reason="policy_mismatch"
    fi
  elif [[ "$preflight_exit" -eq 30 || "$endpoint_status" == "404" || "$endpoint_status" == "422" ]]; then
    raw_outcome="drifted"
    raw_reason="misconfigured_target"
  elif [[ "$preflight_exit" -eq 40 || "$api_exit" -ne 0 ]]; then
    raw_outcome="drifted"
    raw_reason="read_error"
  fi

  outcome="$raw_outcome"
  approved_exception_reason=""

  if [[ "$raw_outcome" != "compliant" ]]; then
    if jq -e --arg outcome "$raw_outcome" '.approved_exceptions // [] | map(.type) | index($outcome) != null' <<<"$target_json" >/dev/null; then
      outcome="approved_exception"
      approved_exception_reason="$(jq -r --arg outcome "$raw_outcome" '.approved_exceptions // [] | map(select(.type == $outcome)) | .[0].reason // ""' <<<"$target_json")"
    fi
  fi

  target_result_json="$(jq -nc \
    --arg id "$id" \
    --arg repo "$repo" \
    --arg branch "$branch" \
    --arg endpoint "$endpoint" \
    --arg outcome "$outcome" \
    --arg raw_outcome "$raw_outcome" \
    --arg raw_reason "$raw_reason" \
    --arg approved_exception_reason "$approved_exception_reason" \
    --arg preflight_status "$preflight_status" \
    --argjson preflight_exit "$preflight_exit" \
    --arg endpoint_status "$endpoint_status" \
    --arg strict "$strict_state" \
    --argjson expected_contexts "$expected_contexts_json" \
    --argjson observed_contexts "$observed_contexts_json" \
    --argjson missing_contexts "$missing_contexts_json" \
    '{
      id: $id,
      repo: $repo,
      branch: $branch,
      endpoint: $endpoint,
      outcome: $outcome,
      raw_outcome: $raw_outcome,
      raw_reason: $raw_reason,
      approved_exception_reason: (if $approved_exception_reason == "" then null else $approved_exception_reason end),
      preflight_status: $preflight_status,
      preflight_exit: $preflight_exit,
      endpoint_status: $endpoint_status,
      strict: $strict,
      expected_contexts: $expected_contexts,
      observed_contexts: $observed_contexts,
      missing_contexts: $missing_contexts
    }')"

  printf '%s\n' "$target_result_json" >> "$RESULTS_NDJSON"

  missing_count="$(jq 'length' <<<"$missing_contexts_json")"
  log "target:done id=$id outcome=$outcome raw_outcome=$raw_outcome preflight_exit=$preflight_exit endpoint_status=$endpoint_status missing_count=$missing_count"
done < "$OUTPUT_DIR/targets.ndjson"

fail_on_json="$(printf '%s' "$FAIL_ON" | tr ',' '\n' | sed '/^[[:space:]]*$/d' | jq -Rsc 'split("\n") | map(select(length > 0))')"

generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

jq -s \
  --arg generated_at "$generated_at" \
  --arg manifest "$MANIFEST" \
  --arg output_dir "$OUTPUT_DIR" \
  --argjson fail_on "$fail_on_json" '
  {
    generated_at: $generated_at,
    manifest_path: $manifest,
    output_dir: $output_dir,
    fail_on: $fail_on,
    summary: {
      target_count: length,
      compliant: ([.[] | select(.outcome == "compliant")] | length),
      drifted: ([.[] | select(.outcome == "drifted")] | length),
      blocked_external: ([.[] | select(.outcome == "blocked_external")] | length),
      approved_exception: ([.[] | select(.outcome == "approved_exception")] | length)
    },
    results: .
  }
' "$RESULTS_NDJSON" | jq -S '.' > "$REPORT_JSON"

{
  printf 'Governance Drift Audit\n'
  printf 'Generated: %s\n' "$generated_at"
  printf 'Manifest: %s\n' "$MANIFEST"
  printf 'Output dir: %s\n\n' "$OUTPUT_DIR"

  printf 'Summary\n'
  jq -r '.summary | "  targets=\(.target_count) compliant=\(.compliant) drifted=\(.drifted) blocked_external=\(.blocked_external) approved_exception=\(.approved_exception)"' "$REPORT_JSON"
  printf '\n'

  printf 'Per Target\n'
  printf '  %-22s %-18s %-18s %-8s %-8s\n' "id" "outcome" "raw_outcome" "strict" "missing"
  printf '  %-22s %-18s %-18s %-8s %-8s\n' "----------------------" "------------------" "------------------" "--------" "--------"
  jq -r '.results | sort_by(.id)[] | [.id, .outcome, .raw_outcome, .strict, (.missing_contexts | length | tostring)] | @tsv' "$REPORT_JSON" \
    | while IFS=$'\t' read -r id outcome raw_outcome strict missing; do
        printf '  %-22s %-18s %-18s %-8s %-8s\n' "$id" "$outcome" "$raw_outcome" "$strict" "$missing"
      done

  printf '\nArtifacts\n'
  printf '  - %s\n' "$VALIDATION_LOG"
  printf '  - %s\n' "$REPORT_JSON"
  printf '  - %s\n' "$REPORT_TXT"
  printf '  - %s\n' "$AUDIT_LOG"
} > "$REPORT_TXT"

log "governance_drift_audit:reports report_json=$REPORT_JSON report_txt=$REPORT_TXT"

if jq -e --argjson fail_on "$fail_on_json" '.results | map(select(.outcome as $o | $fail_on | index($o) != null)) | length > 0' "$REPORT_JSON" >/dev/null; then
  emit_status "drifted_fail" "fail_on_outcomes_present" "report=$REPORT_JSON"
  exit 1
fi

emit_status "ok" "audit_complete" "report=$REPORT_JSON"
exit 0
