#!/usr/bin/env bash
# CI Gate: deterministic, read-only governance preflight for branch protection.
#
# Usage:
#   bash scripts/ci/check_governance_preflight.sh \
#     --repo rogu3bear/adapter-os \
#     --branch main \
#     --required-context 'FFI AddressSanitizer (push)'
#
# Exit codes:
#   0  capable
#   20 blocked_external
#   30 misconfigured
#   40 error
#   2  usage

set -euo pipefail
export LC_ALL=C
export LANG=C

usage() {
  cat <<'EOF'
Usage: check_governance_preflight.sh --repo <owner/name> --branch <branch> --required-context <context>

Deterministically probes:
  repos/<repo>/branches/<branch>/protection/required_status_checks

Status model + exit codes:
  capable          (0): required context exists in required_status_checks.
  blocked_external (20): required-check API capability blocked (HTTP 403).
  misconfigured    (30): bad repo/branch target, unprotected branch, or missing context.
  error            (40): unexpected runtime/auth/network/tooling failure.
EOF
}

emit_status() {
  local status="$1"
  local reason="$2"
  local detail="${3:-}"

  printf 'status=%s repo=%s branch=%s required_context="%s" endpoint=%s reason=%s' \
    "$status" "$REPO" "$BRANCH" "$REQUIRED_CONTEXT" "$ENDPOINT" "$reason"
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
  echo "::error::Missing required arguments." >&2
  usage >&2
  exit 2
fi

if ! command -v gh >/dev/null 2>&1; then
  emit_status "error" "missing_tool_gh"
  exit 40
fi

ENDPOINT="repos/${REPO}/branches/${BRANCH}/protection/required_status_checks"
PROBE_OUTPUT=""
PROBE_EXIT=0

if PROBE_OUTPUT="$(gh api "$ENDPOINT" 2>&1)"; then
  PROBE_EXIT=0
else
  PROBE_EXIT=$?
fi

if [[ "$PROBE_EXIT" -eq 0 ]]; then
  if printf '%s' "$PROBE_OUTPUT" | rg -Fq -- "\"${REQUIRED_CONTEXT}\""; then
    emit_status "capable" "required_context_present"
    exit 0
  fi

  emit_status "misconfigured" "required_context_missing" "$(sanitize_detail "$PROBE_OUTPUT")"
  exit 30
fi

if printf '%s' "$PROBE_OUTPUT" | rg -q 'HTTP 403|\"status\"[[:space:]]*:[[:space:]]*\"?403\"?|Upgrade to GitHub Pro'; then
  emit_status "blocked_external" "http_403" "$(sanitize_detail "$PROBE_OUTPUT")"
  exit 20
fi

if printf '%s' "$PROBE_OUTPUT" | rg -q 'HTTP 404|HTTP 422|\"status\"[[:space:]]*:[[:space:]]*\"?(404|422)\"?'; then
  emit_status "misconfigured" "invalid_target_or_unprotected_branch" "$(sanitize_detail "$PROBE_OUTPUT")"
  exit 30
fi

emit_status "error" "probe_failed_exit_${PROBE_EXIT}" "$(sanitize_detail "$PROBE_OUTPUT")"
exit 40
