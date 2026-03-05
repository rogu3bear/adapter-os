#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CSV_PATH="${ROOT}/.planning/prod_speed_blockers.csv"
WAIVER_PATH="${ROOT}/.planning/prod_speed_blocker_waivers.csv"

if [[ ! -f "${CSV_PATH}" ]]; then
  echo "[check_prod_speed_blockers] missing file: ${CSV_PATH}" >&2
  exit 2
fi

expected=50

line_count="$(tail -n +2 "${CSV_PATH}" | wc -l | tr -d ' ')"
if [[ "${line_count}" -ne "${expected}" ]]; then
  echo "[check_prod_speed_blockers] expected ${expected} blockers, found ${line_count}" >&2
  exit 3
fi

for i in $(seq 1 50); do
  id="$(printf "B%02d" "${i}")"
  count="$(awk -F, -v id="${id}" '$1==id {c++} END {print c+0}' "${CSV_PATH}")"
  if [[ "${count}" -ne 1 ]]; then
    echo "[check_prod_speed_blockers] expected exactly one row for ${id}, found ${count}" >&2
    exit 4
  fi
done

# Ensure 4 steps are present per row.
missing_steps="$(awk -F, 'NR>1 {if ($4=="" || $5=="" || $6=="" || $7=="") print $1}' "${CSV_PATH}")"
if [[ -n "${missing_steps}" ]]; then
  echo "[check_prod_speed_blockers] rows missing one or more steps:" >&2
  echo "${missing_steps}" >&2
  exit 5
fi

# Report unresolved inventory (non-failing by default).
unresolved="$(awk -F, 'NR>1 && ($3=="unresolved_id" || $3=="blocked_scope" || $3=="external_blocked") {c++} END {print c+0}' "${CSV_PATH}")"
echo "[check_prod_speed_blockers] OK: ${expected} blockers validated; unresolved=${unresolved}"

if [[ "${STRICT_BLOCKER_CLOSURE:-0}" == "1" && "${unresolved}" -gt 0 ]]; then
  if [[ ! -f "${WAIVER_PATH}" ]]; then
    echo "[check_prod_speed_blockers] STRICT_BLOCKER_CLOSURE=1 unresolved blockers remain (${unresolved}) and waiver file is missing" >&2
    exit 6
  fi

  now_epoch="$(date -u +%s)"
  unresolved_ids="$(awk -F, 'NR>1 && ($3=="unresolved_id" || $3=="blocked_scope" || $3=="external_blocked") {print $1}' "${CSV_PATH}")"
  missing=0
  expired=0

  while IFS= read -r bid; do
    [[ -z "${bid}" ]] && continue
    waiver_row="$(awk -F, -v id="${bid}" 'NR>1 && $1==id {print $0; exit}' "${WAIVER_PATH}")"
    if [[ -z "${waiver_row}" ]]; then
      echo "[check_prod_speed_blockers] missing waiver for unresolved blocker ${bid}" >&2
      missing=$((missing + 1))
      continue
    fi

    exp_utc="$(echo "${waiver_row}" | awk -F, '{print $5}')"
    exp_epoch="$(date -u -j -f "%Y-%m-%dT%H:%M:%SZ" "${exp_utc}" +%s 2>/dev/null || true)"
    if [[ -z "${exp_epoch}" ]]; then
      exp_epoch="$(date -u -d "${exp_utc}" +%s 2>/dev/null || true)"
    fi
    if [[ -z "${exp_epoch}" ]]; then
      echo "[check_prod_speed_blockers] invalid waiver expiry for ${bid}: ${exp_utc}" >&2
      expired=$((expired + 1))
      continue
    fi
    if (( exp_epoch < now_epoch )); then
      echo "[check_prod_speed_blockers] expired waiver for ${bid}: ${exp_utc}" >&2
      expired=$((expired + 1))
    fi
  done <<< "${unresolved_ids}"

  if [[ "${missing}" -gt 0 || "${expired}" -gt 0 ]]; then
    echo "[check_prod_speed_blockers] STRICT_BLOCKER_CLOSURE=1 failed: missing_waivers=${missing} expired_waivers=${expired}" >&2
    exit 6
  fi
  echo "[check_prod_speed_blockers] STRICT_BLOCKER_CLOSURE=1 passed with active waivers for unresolved blockers"
fi
