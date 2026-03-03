#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

usage() {
  cat <<'USAGE'
Emit a stage-gated training rollout plan (10k -> 100k -> 1M) for one or more datasets.

This script ONLY prints commands and stop conditions. It never starts training.

Usage:
  scripts/datasets/stage_gate_plan.sh [options]

Required (at least one input source):
  --dataset-version-id <id>      Dataset version id (repeatable, comma-separated allowed)
  --dataset-id <id>              Dataset id (repeatable, comma-separated allowed)

Optional:
  --repo-id <id>                 Existing adapter repository id (placeholder if omitted)
  --base-model-id <id>           Base model id (placeholder if omitted)
  --base-url <url>               API base URL (default: AOS_BASE_URL or http://localhost:18080/api)
  --tenant <tenant>              Tenant id (default: AOS_TENANT_ID or default)
  --aosctl <path>                aosctl binary path (default: ./aosctl)
  --gate-loss-max <float>        Max acceptable eval loss per stage (default: 2.0)
  --gate-regress-pct <float>     Max allowed regression vs previous stage in % (default: 5)
  --help                         Show this help

Examples:
  scripts/datasets/stage_gate_plan.sh --dataset-version-id dv_123
  scripts/datasets/stage_gate_plan.sh --dataset-id ds_a,ds_b --repo-id repo_42 --base-model-id phi-3-mini
USAGE
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

trim() {
  local s="$1"
  s="${s#${s%%[![:space:]]*}}"
  s="${s%${s##*[![:space:]]}}"
  printf '%s' "$s"
}

append_csv_values() {
  local var_name="$1"
  local raw="$2"
  local item
  local cleaned

  IFS=',' read -r -a items <<< "$raw"
  for item in "${items[@]}"; do
    cleaned="$(trim "$item")"
    if [ -n "$cleaned" ]; then
      eval "$var_name+=(\"$cleaned\")"
    fi
  done
}

BASE_URL="${AOS_BASE_URL:-http://localhost:18080/api}"
TENANT_ID="${AOS_TENANT_ID:-default}"
AOSCTL="./aosctl"
REPO_ID=""
BASE_MODEL_ID=""
GATE_LOSS_MAX="2.0"
GATE_REGRESS_PCT="5"

DATASET_IDS=()
DATASET_VERSION_IDS=()

while (($# > 0)); do
  case "$1" in
    --dataset-id)
      [ $# -ge 2 ] || die "--dataset-id requires a value"
      append_csv_values DATASET_IDS "$2"
      shift 2
      ;;
    --dataset-version-id)
      [ $# -ge 2 ] || die "--dataset-version-id requires a value"
      append_csv_values DATASET_VERSION_IDS "$2"
      shift 2
      ;;
    --repo-id)
      [ $# -ge 2 ] || die "--repo-id requires a value"
      REPO_ID="$(trim "$2")"
      shift 2
      ;;
    --base-model-id)
      [ $# -ge 2 ] || die "--base-model-id requires a value"
      BASE_MODEL_ID="$(trim "$2")"
      shift 2
      ;;
    --base-url)
      [ $# -ge 2 ] || die "--base-url requires a value"
      BASE_URL="$(trim "$2")"
      shift 2
      ;;
    --tenant)
      [ $# -ge 2 ] || die "--tenant requires a value"
      TENANT_ID="$(trim "$2")"
      shift 2
      ;;
    --aosctl)
      [ $# -ge 2 ] || die "--aosctl requires a value"
      AOSCTL="$(trim "$2")"
      shift 2
      ;;
    --gate-loss-max)
      [ $# -ge 2 ] || die "--gate-loss-max requires a value"
      GATE_LOSS_MAX="$(trim "$2")"
      shift 2
      ;;
    --gate-regress-pct)
      [ $# -ge 2 ] || die "--gate-regress-pct requires a value"
      GATE_REGRESS_PCT="$(trim "$2")"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1"
      ;;
  esac
done

if [ "${#DATASET_IDS[@]}" -eq 0 ] && [ "${#DATASET_VERSION_IDS[@]}" -eq 0 ]; then
  die "provide at least one --dataset-id and/or --dataset-version-id"
fi

if [[ "$BASE_URL" != */api ]]; then
  BASE_URL="${BASE_URL%/}/api"
fi

REPO_ID="${REPO_ID:-<REPO_ID>}"
BASE_MODEL_ID="${BASE_MODEL_ID:-<BASE_MODEL_ID>}"

emit_header() {
  cat <<EOF_HDR
# Stage-Gated Training Plan (commands only)

Generated at: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
Tenant: ${TENANT_ID}
Base URL: ${BASE_URL}
aosctl: ${AOSCTL}
Repository: ${REPO_ID}
Base model: ${BASE_MODEL_ID}

Global gate thresholds:
- eval_loss <= ${GATE_LOSS_MAX}
- regression_vs_previous_stage_pct <= ${GATE_REGRESS_PCT}

Stop policy:
- Any failed preflight/gate command is a hard STOP.
- Do not proceed to the next stage until all gate checks pass.
- Abort entire rollout on repeated failure of same gate after one retry.
EOF_HDR
}

emit_input_inventory() {
  local id

  echo "Input inventory:"

  if [ "${#DATASET_IDS[@]}" -gt 0 ]; then
    for id in "${DATASET_IDS[@]}"; do
      printf -- "- dataset_id=%s\n" "$id"
    done
  fi

  if [ "${#DATASET_VERSION_IDS[@]}" -gt 0 ]; then
    for id in "${DATASET_VERSION_IDS[@]}"; do
      printf -- "- dataset_version_id=%s\n" "$id"
    done
  fi

  cat <<'EOF_INV'

# Preflight resolution commands (run before stage 10k):
# Resolve dataset -> latest ready dataset_version_id where needed.
EOF_INV

  if [ "${#DATASET_IDS[@]}" -gt 0 ]; then
    for id in "${DATASET_IDS[@]}"; do
      cat <<EOF_CMD
${AOSCTL} --json dataset versions list ${id} | jq -r '.versions[] | select(.status=="ready") | .id' | tail -n 1
EOF_CMD
    done
  fi

  if [ "${#DATASET_VERSION_IDS[@]}" -gt 0 ]; then
    for id in "${DATASET_VERSION_IDS[@]}"; do
      cat <<EOF_CMD
${AOSCTL} --json dataset version get ${id}
EOF_CMD
    done
  fi
}

emit_stage() {
  local stage_name="$1"
  local sample_budget="$2"
  local prev_stage_label="$3"

  cat <<EOF_STAGE

## ${stage_name} (sample budget: ${sample_budget})

### TODO
- Confirm all dataset versions are status=ready and schema-valid.
- Confirm repository/base model identifiers are concrete values (no placeholders).
- Capture baseline metrics snapshot before training command.

### Training command template (do not execute automatically)
${AOSCTL} --json train start ${REPO_ID} \\
  --base-model-id ${BASE_MODEL_ID} \\
  --dataset-version-ids <CSV_DATASET_VERSION_IDS> \\
  --backend mlx \\
  --max-samples ${sample_budget}

### Required gate checks (run after training completes)
# 1) Job reached terminal success.
${AOSCTL} --json train status <JOB_ID>

# 2) Metrics available for loss/quality.
${AOSCTL} --json train metrics <JOB_ID>

# 3) Repo health unchanged (no branch drift / unexpected default branch move).
${AOSCTL} --json adapter repos get ${REPO_ID}

# 4) Evaluate gate contract.
python3 - <<'PY'
import json, sys
metrics = json.load(sys.stdin)
eval_loss = float(metrics.get("eval_loss", 999))
regress_pct = float(metrics.get("regression_vs_previous_stage_pct", 999))
if eval_loss > ${GATE_LOSS_MAX}:
    raise SystemExit(f"STOP: eval_loss {eval_loss} > ${GATE_LOSS_MAX}")
if regress_pct > ${GATE_REGRESS_PCT}:
    raise SystemExit(f"STOP: regression {regress_pct}% > ${GATE_REGRESS_PCT}%")
print("PASS: gate thresholds satisfied")
PY

### Stop conditions
- STOP if job state is failed/cancelled/timed_out.
- STOP if metrics payload is missing eval_loss.
- STOP if gate contract script exits non-zero.
- STOP if adapter repo lookup fails or returns tenant mismatch.
EOF_STAGE

  if [ -n "$prev_stage_label" ]; then
    cat <<EOF_PREV
- STOP if ${stage_name} does not outperform or match ${prev_stage_label} within allowed regression threshold (${GATE_REGRESS_PCT}%).
EOF_PREV
  fi
}

emit_footer() {
  cat <<'EOF_FOOT'

## Promotion policy
- Promote only after 1M stage passes all gate checks.
- Promotion command template:
./aosctl --json adapter versions promote <ADAPTER_VERSION_ID> --repo-id <REPO_ID>

## Operator notes
- Replace placeholders first: <REPO_ID>, <BASE_MODEL_ID>, <CSV_DATASET_VERSION_IDS>, <JOB_ID>.
- Keep a run log with timestamps and command outputs per stage.
- If a stage fails twice for the same gate, open an investigation instead of escalating stage size.
EOF_FOOT
}

emit_header
emit_input_inventory
emit_stage "Stage 1" "10000" ""
emit_stage "Stage 2" "100000" "Stage 1"
emit_stage "Stage 3" "1000000" "Stage 2"
emit_footer
