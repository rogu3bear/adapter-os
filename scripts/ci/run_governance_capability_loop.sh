#!/usr/bin/env bash
# CI helper: deterministic capability polling for governance preflight.
#
# Usage:
#   bash scripts/ci/run_governance_capability_loop.sh \
#     --repo rogu3bear/adapter-os \
#     --branch main \
#     --required-context 'FFI AddressSanitizer (push)' \
#     --output-dir var/evidence/governance-enforcement-<UTCSTAMP> \
#     --attempts 4 \
#     --sleep-seconds 5
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

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

usage() {
  cat <<'USAGE'
Usage: run_governance_capability_loop.sh --repo <owner/name> --branch <branch> --required-context <ctx> --output-dir <dir> [--attempts <N>] [--sleep-seconds <N>]

Runs deterministic preflight polling and emits:
  - capability-loop.log
  - preflight-attempt-XX.log / .exit
  - gate-state.txt
  - loop-summary.txt
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

REPO=""
BRANCH=""
REQUIRED_CONTEXT=""
OUTPUT_DIR=""
ATTEMPTS=4
SLEEP_SECONDS=5

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
    --output-dir)
      OUTPUT_DIR="${2:-}"
      shift 2
      ;;
    --attempts)
      ATTEMPTS="${2:-}"
      shift 2
      ;;
    --sleep-seconds)
      SLEEP_SECONDS="${2:-}"
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

if [[ -z "$REPO" || -z "$BRANCH" || -z "$REQUIRED_CONTEXT" || -z "$OUTPUT_DIR" ]]; then
  echo "::error::Missing required arguments." >&2
  usage >&2
  exit 2
fi

if ! [[ "$ATTEMPTS" =~ ^[0-9]+$ ]] || [[ "$ATTEMPTS" -lt 1 ]]; then
  emit_status "misconfigured" "invalid_attempts" "attempts=${ATTEMPTS}"
  exit 30
fi

if ! [[ "$SLEEP_SECONDS" =~ ^[0-9]+$ ]] || [[ "$SLEEP_SECONDS" -lt 0 ]]; then
  emit_status "misconfigured" "invalid_sleep_seconds" "sleep_seconds=${SLEEP_SECONDS}"
  exit 30
fi

mkdir -p "$OUTPUT_DIR"
LOOP_LOG="$OUTPUT_DIR/capability-loop.log"
SUMMARY_LOG="$OUTPUT_DIR/loop-summary.txt"
GATE_STATE_FILE="$OUTPUT_DIR/gate-state.txt"

: > "$LOOP_LOG"
: > "$SUMMARY_LOG"

printf 'repo=%s\nbranch=%s\nrequired_context=%s\nattempts=%s\nsleep_seconds=%s\n' \
  "$REPO" "$BRANCH" "$REQUIRED_CONTEXT" "$ATTEMPTS" "$SLEEP_SECONDS" > "$SUMMARY_LOG"

final_status="error"
final_reason="loop_not_started"
final_exit=40
completed_attempts=0

for ((i=1; i<=ATTEMPTS; i++)); do
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  attempt_id="$(printf '%02d' "$i")"
  attempt_log="$OUTPUT_DIR/preflight-attempt-${attempt_id}.log"
  attempt_exit_file="$OUTPUT_DIR/preflight-attempt-${attempt_id}.exit"

  probe_output=""
  probe_exit=0
  if probe_output="$(bash scripts/ci/check_governance_preflight.sh \
      --repo "$REPO" \
      --branch "$BRANCH" \
      --required-context "$REQUIRED_CONTEXT" 2>&1)"; then
    probe_exit=0
  else
    probe_exit=$?
  fi

  printf '%s\n' "$probe_output" > "$attempt_log"
  printf '%s\n' "$probe_exit" > "$attempt_exit_file"

  status="$(printf '%s' "$probe_output" | sed -n 's/.*status=\([^ ]*\).*/\1/p' | head -1)"
  reason="$(printf '%s' "$probe_output" | sed -n 's/.*reason=\([^ ]*\).*/\1/p' | head -1)"

  if [[ -z "$status" ]]; then
    status="error"
  fi
  if [[ -z "$reason" ]]; then
    reason="unknown"
  fi

  printf 'ts=%s attempt=%s/%s exit=%s status=%s reason=%s\n' \
    "$ts" "$i" "$ATTEMPTS" "$probe_exit" "$status" "$reason" >> "$LOOP_LOG"

  final_status="$status"
  final_reason="$reason"
  final_exit="$probe_exit"
  completed_attempts="$i"

  case "$status" in
    capable)
      break
      ;;
    misconfigured|error)
      break
      ;;
    blocked_external)
      if [[ "$i" -lt "$ATTEMPTS" && "$SLEEP_SECONDS" -gt 0 ]]; then
        sleep "$SLEEP_SECONDS"
      fi
      ;;
    *)
      # Unexpected token; treat as error and stop.
      final_status="error"
      final_reason="unknown_status"
      final_exit=40
      break
      ;;
  esac
done

printf '%s\n' "$final_status" > "$GATE_STATE_FILE"
printf 'completed_attempts=%s\nfinal_status=%s\nfinal_reason=%s\nfinal_exit=%s\n' \
  "$completed_attempts" "$final_status" "$final_reason" "$final_exit" >> "$SUMMARY_LOG"

emit_status "$final_status" "$final_reason" "attempts=${completed_attempts} gate_state=${GATE_STATE_FILE}"

case "$final_status" in
  capable)
    exit 0
    ;;
  blocked_external)
    exit 20
    ;;
  misconfigured)
    exit 30
    ;;
  error)
    exit 40
    ;;
  *)
    exit 40
    ;;
esac
