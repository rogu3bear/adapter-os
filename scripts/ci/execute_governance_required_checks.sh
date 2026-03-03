#!/usr/bin/env bash
# CI helper: execute canonical required_status_checks enforcement when capable,
# with deterministic no-write handling when blocked and rollback-on-verify-fail.
#
# Usage:
#   bash scripts/ci/execute_governance_required_checks.sh \
#     --repo rogu3bear/adapter-os \
#     --branch main \
#     --required-context 'FFI AddressSanitizer (push)' \
#     --manifest docs/governance/target-manifest.json \
#     --output-dir var/evidence/governance-enforcement-exec-<UTCSTAMP>
#
# Exit codes:
#   0  enforced_verified
#   20 blocked_external
#   30 misconfigured
#   40 runtime/tooling error
#   50 verification_failed_rollback_applied
#   2  usage

set -euo pipefail
export LC_ALL=C
export LANG=C

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

usage() {
  cat <<'USAGE'
Usage: execute_governance_required_checks.sh --repo <owner/name> --branch <branch> --required-context <ctx> [--manifest <path>] [--output-dir <dir>]

Deterministic flow:
1) preflight gate (no side effects)
2) if capable: pre-read required_status_checks
3) union(existing_contexts, canonical_contexts)
4) PATCH strict + contexts
5) post-read verification
6) rollback to pre-read policy if verification fails

Artifacts include:
- preflight-before.log/.exit, gate-state.txt
- pre-read.json, existing-contexts.txt, checklist-contexts.txt, final-contexts.txt
- write.json, post-read.json, post-contexts.txt, preflight-after.log/.exit
- verification.txt
- rollback-write.json, rollback-post-read.json (failure path)
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

REPO=""
BRANCH=""
REQUIRED_CONTEXT=""
MANIFEST="docs/governance/target-manifest.json"
OUTPUT_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo)
      REPO="${2:-}"
      shift 2
      ;;
    --branch)
      BRANCH="${2:-}"
      shift 2
      ;;
    --required-context)
      REQUIRED_CONTEXT="${2:-}"
      shift 2
      ;;
    --manifest)
      MANIFEST="${2:-}"
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

if [[ -z "$REPO" || -z "$BRANCH" || -z "$REQUIRED_CONTEXT" ]]; then
  echo "::error::Missing required arguments" >&2
  usage >&2
  exit 2
fi

for tool in gh jq rg; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    emit_status "error" "missing_tool_${tool}"
    exit 40
  fi
done

if [[ -z "$OUTPUT_DIR" ]]; then
  utcstamp="$(date -u +%Y%m%dT%H%M%SZ)"
  OUTPUT_DIR="var/evidence/governance-enforcement-exec-${utcstamp}"
fi

mkdir -p "$OUTPUT_DIR"

METADATA_FILE="$OUTPUT_DIR/metadata.txt"
PREFLIGHT_BEFORE_LOG="$OUTPUT_DIR/preflight-before.log"
PREFLIGHT_BEFORE_EXIT="$OUTPUT_DIR/preflight-before.exit"
GATE_STATE_FILE="$OUTPUT_DIR/gate-state.txt"
BLOCKED_NOTE_FILE="$OUTPUT_DIR/blocked-note.txt"
BLOCKED_WRITE_FILE="$OUTPUT_DIR/blocked-write-attempts.txt"
CHECKLIST_CONTEXTS="$OUTPUT_DIR/checklist-contexts.txt"
PRE_READ_JSON="$OUTPUT_DIR/pre-read.json"
EXISTING_CONTEXTS="$OUTPUT_DIR/existing-contexts.txt"
FINAL_CONTEXTS="$OUTPUT_DIR/final-contexts.txt"
WRITE_JSON="$OUTPUT_DIR/write.json"
POST_READ_JSON="$OUTPUT_DIR/post-read.json"
POST_CONTEXTS="$OUTPUT_DIR/post-contexts.txt"
PREFLIGHT_AFTER_LOG="$OUTPUT_DIR/preflight-after.log"
PREFLIGHT_AFTER_EXIT="$OUTPUT_DIR/preflight-after.exit"
VERIFICATION_FILE="$OUTPUT_DIR/verification.txt"
ROLLBACK_WRITE_JSON="$OUTPUT_DIR/rollback-write.json"
ROLLBACK_POST_READ_JSON="$OUTPUT_DIR/rollback-post-read.json"

{
  printf 'repo=%s\n' "$REPO"
  printf 'branch=%s\n' "$BRANCH"
  printf 'required_context=%s\n' "$REQUIRED_CONTEXT"
  printf 'manifest=%s\n' "$MANIFEST"
  printf 'generated_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$METADATA_FILE"

# Validate manifest if present; fallback to required_context only when manifest is unavailable.
if [[ -f "$MANIFEST" ]]; then
  set +e
  bash scripts/ci/validate_governance_target_manifest.sh --manifest "$MANIFEST" > "$OUTPUT_DIR/manifest-validation.txt" 2>&1
  manifest_exit=$?
  set -e
  if [[ "$manifest_exit" -ne 0 ]]; then
    emit_status "misconfigured" "manifest_validation_failed" "output_dir=${OUTPUT_DIR}"
    exit 30
  fi

  jq -r '.canonical_policy.required_contexts // [] | .[]' "$MANIFEST" | sed '/^[[:space:]]*$/d' | sort -u > "$CHECKLIST_CONTEXTS"
else
  printf '%s\n' "$REQUIRED_CONTEXT" > "$CHECKLIST_CONTEXTS"
fi

if ! rg -Fxq -- "$REQUIRED_CONTEXT" "$CHECKLIST_CONTEXTS"; then
  printf '%s\n' "$REQUIRED_CONTEXT" >> "$CHECKLIST_CONTEXTS"
  sort -u -o "$CHECKLIST_CONTEXTS" "$CHECKLIST_CONTEXTS"
fi

# 1) Preflight gate (hard stop before any write)
preflight_before_output=""
preflight_before_exit=0
if preflight_before_output="$(bash scripts/ci/check_governance_preflight.sh \
  --repo "$REPO" \
  --branch "$BRANCH" \
  --required-context "$REQUIRED_CONTEXT" 2>&1)"; then
  preflight_before_exit=0
else
  preflight_before_exit=$?
fi
printf '%s\n' "$preflight_before_output" > "$PREFLIGHT_BEFORE_LOG"
printf '%s\n' "$preflight_before_exit" > "$PREFLIGHT_BEFORE_EXIT"

preflight_before_status="$(printf '%s' "$preflight_before_output" | sed -n 's/.*status=\([^ ]*\).*/\1/p' | head -1)"
if [[ -z "$preflight_before_status" ]]; then
  preflight_before_status="error"
fi
printf '%s\n' "$preflight_before_status" > "$GATE_STATE_FILE"

case "$preflight_before_status" in
  blocked_external)
    printf 'status=blocked_external reason=preflight_http_403\n' > "$BLOCKED_NOTE_FILE"
    {
      echo 'write_attempts=0'
      echo 'policy_mutations=0'
      echo 'rollback_attempts=0'
      echo 'reason=blocked_external_hard_gate'
    } > "$BLOCKED_WRITE_FILE"
    emit_status "blocked_external" "preflight_http_403" "output_dir=${OUTPUT_DIR}"
    exit 20
    ;;
  misconfigured)
    printf 'status=misconfigured reason=preflight_target_or_context_invalid\n' > "$BLOCKED_NOTE_FILE"
    emit_status "misconfigured" "preflight_misconfigured" "output_dir=${OUTPUT_DIR}"
    exit 30
    ;;
  capable)
    # proceed
    ;;
  *)
    printf 'status=error reason=preflight_unexpected_status detail=%s\n' "$(sanitize_detail "$preflight_before_output")" > "$BLOCKED_NOTE_FILE"
    emit_status "error" "preflight_failed" "output_dir=${OUTPUT_DIR}"
    exit 40
    ;;
esac

# 2) Capable path: baseline read
endpoint="repos/${REPO}/branches/${BRANCH}/protection/required_status_checks"
if ! gh api "$endpoint" > "$PRE_READ_JSON" 2> "$OUTPUT_DIR/pre-read.err"; then
  emit_status "error" "pre_read_failed" "output_dir=${OUTPUT_DIR}"
  exit 40
fi

jq -r '.contexts // [] | .[]' "$PRE_READ_JSON" | sed '/^[[:space:]]*$/d' | sort -u > "$EXISTING_CONTEXTS"
cat "$EXISTING_CONTEXTS" "$CHECKLIST_CONTEXTS" | sed '/^[[:space:]]*$/d' | sort -u > "$FINAL_CONTEXTS"

# 3) PATCH strict + union contexts
patch_args=(--method PATCH "$endpoint" -f strict=true)
while IFS= read -r context; do
  [[ -z "$context" ]] && continue
  patch_args+=(-f "contexts[]=${context}")
done < "$FINAL_CONTEXTS"

if ! gh api "${patch_args[@]}" > "$WRITE_JSON" 2> "$OUTPUT_DIR/write.err"; then
  emit_status "error" "write_failed" "output_dir=${OUTPUT_DIR}"
  exit 40
fi

# 4) Post-read + postflight probe
if ! gh api "$endpoint" > "$POST_READ_JSON" 2> "$OUTPUT_DIR/post-read.err"; then
  emit_status "error" "post_read_failed" "output_dir=${OUTPUT_DIR}"
  exit 40
fi
jq -r '.contexts // [] | .[]' "$POST_READ_JSON" | sed '/^[[:space:]]*$/d' | sort -u > "$POST_CONTEXTS"

preflight_after_output=""
preflight_after_exit=0
if preflight_after_output="$(bash scripts/ci/check_governance_preflight.sh \
  --repo "$REPO" \
  --branch "$BRANCH" \
  --required-context "$REQUIRED_CONTEXT" 2>&1)"; then
  preflight_after_exit=0
else
  preflight_after_exit=$?
fi
printf '%s\n' "$preflight_after_output" > "$PREFLIGHT_AFTER_LOG"
printf '%s\n' "$preflight_after_exit" > "$PREFLIGHT_AFTER_EXIT"
preflight_after_status="$(printf '%s' "$preflight_after_output" | sed -n 's/.*status=\([^ ]*\).*/\1/p' | head -1)"

# 5) Verification matrix
strict_state="$(jq -r 'if (.strict | type) == "boolean" then (if .strict then "true" else "false" end) else "unknown" end' "$POST_READ_JSON")"
missing_checklist_count="$(comm -23 "$CHECKLIST_CONTEXTS" "$POST_CONTEXTS" | wc -l | tr -d ' ')"
missing_existing_count="$(comm -23 "$EXISTING_CONTEXTS" "$POST_CONTEXTS" | wc -l | tr -d ' ')"

verify_ok=1
if [[ "$strict_state" != "true" ]]; then
  verify_ok=0
fi
if [[ "$missing_checklist_count" != "0" ]]; then
  verify_ok=0
fi
if [[ "$missing_existing_count" != "0" ]]; then
  verify_ok=0
fi
if [[ "$preflight_after_status" != "capable" ]]; then
  verify_ok=0
fi

{
  echo "preflight_before_status=${preflight_before_status}"
  echo "preflight_after_status=${preflight_after_status:-unknown}"
  echo "strict_state=${strict_state}"
  echo "missing_checklist_contexts=${missing_checklist_count}"
  echo "missing_existing_contexts=${missing_existing_count}"
  echo "verify_ok=${verify_ok}"
} > "$VERIFICATION_FILE"

if [[ "$verify_ok" -eq 1 ]]; then
  emit_status "enforced_verified" "write_read_verify_passed" "output_dir=${OUTPUT_DIR}"
  exit 0
fi

# 6) Verification failed -> rollback to baseline
pre_strict="$(jq -r 'if (.strict | type) == "boolean" then (if .strict then "true" else "false" end) else "true" end' "$PRE_READ_JSON")"

rollback_args=(--method PATCH "$endpoint" -f "strict=${pre_strict}")
while IFS= read -r context; do
  [[ -z "$context" ]] && continue
  rollback_args+=(-f "contexts[]=${context}")
done < "$EXISTING_CONTEXTS"

if ! gh api "${rollback_args[@]}" > "$ROLLBACK_WRITE_JSON" 2> "$OUTPUT_DIR/rollback-write.err"; then
  emit_status "error" "verification_failed_and_rollback_failed" "output_dir=${OUTPUT_DIR}"
  exit 40
fi

if ! gh api "$endpoint" > "$ROLLBACK_POST_READ_JSON" 2> "$OUTPUT_DIR/rollback-post-read.err"; then
  emit_status "error" "verification_failed_and_rollback_read_failed" "output_dir=${OUTPUT_DIR}"
  exit 40
fi

{
  echo "rollback_applied=true"
  echo "rollback_strict=${pre_strict}"
} >> "$VERIFICATION_FILE"

emit_status "verification_failed" "rollback_applied" "output_dir=${OUTPUT_DIR}"
exit 50
