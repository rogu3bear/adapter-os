#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

: "${RUNBOOK_DRILL_EVIDENCE_DIR:=$ROOT_DIR/.planning/prod-cut/evidence/runbooks}"
: "${RUNBOOK_DRILL_STRICT:=0}"

scenarios=(
  worker_crash
  determinism_violation
  latency_spike
  memory_pressure
  disk_full
)

required_files=(
  timeline.md
  detection_signal.md
  action_taken.md
  recovery_proof.md
  post_check.log
)

missing=0
placeholder=0

for scenario in "${scenarios[@]}"; do
  scenario_dir="$RUNBOOK_DRILL_EVIDENCE_DIR/$scenario"
  if [[ ! -d "$scenario_dir" ]]; then
    echo "Missing runbook evidence directory: $scenario_dir"
    missing=1
    continue
  fi

  for rel in "${required_files[@]}"; do
    file="$scenario_dir/$rel"
    if [[ ! -f "$file" ]]; then
      echo "Missing runbook evidence file: $file"
      missing=1
      continue
    fi

    if [[ "$RUNBOOK_DRILL_STRICT" == "1" ]]; then
      if rg -qi "TODO|TBD|pending capture" "$file"; then
        echo "Placeholder content remains in strict runbook evidence file: $file"
        placeholder=1
      fi
    fi
  done
done

if [[ "$missing" -ne 0 ]]; then
  echo "FAIL: Runbook drill evidence is incomplete."
  exit 1
fi

if [[ "$RUNBOOK_DRILL_STRICT" == "1" && "$placeholder" -ne 0 ]]; then
  echo "FAIL: Runbook drill evidence contains placeholders under strict mode."
  exit 1
fi

echo "=== Runbook Drill Evidence Check: PASSED ==="
