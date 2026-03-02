#!/usr/bin/env bash
set -euo pipefail

# Full tuning orchestrator (operator-assisted)
# Uses existing AdapterOS surfaces:
# - aosctl train start/status/report
# - aosctl audit-determinism
# - promotion gate endpoints

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
AOSCTL="${ROOT_DIR}/aosctl"
BASE_URL="${AOS_SERVER_URL:-http://127.0.0.1:18080}"
MODEL_ID="${AOS_BASE_MODEL_ID:-Qwen3.5-27B}"
MODEL_PATH="${AOS_MODEL_PATH:-${ROOT_DIR}/var/models/${MODEL_ID}}"
DATASET_PATH="${DATASET_PATH:-${ROOT_DIR}/var/datasets/generated/spark/train/adapteros_train_full_real_v2.jsonl}"

REPO_ID="${REPO_ID:-}"
DATASET_VERSION_IDS="${DATASET_VERSION_IDS:-}"
BRANCH="${BRANCH:-main}"
BACKEND_POLICY="${BACKEND_POLICY:-auto}"
BACKEND="${BACKEND:-mlx}"
REQUIRE_GPU="${REQUIRE_GPU:-true}"

if [[ -z "${REPO_ID}" ]]; then
  echo "ERROR: REPO_ID is required" >&2
  exit 1
fi
if [[ -z "${DATASET_VERSION_IDS}" ]]; then
  echo "ERROR: DATASET_VERSION_IDS is required (comma-separated)" >&2
  exit 1
fi
if [[ ! -x "${AOSCTL}" ]]; then
  echo "ERROR: aosctl launcher missing at ${AOSCTL}" >&2
  exit 1
fi
if [[ ! -d "${MODEL_PATH}" ]]; then
  echo "ERROR: model directory missing: ${MODEL_PATH}" >&2
  exit 1
fi
if [[ ! -f "${DATASET_PATH}" ]]; then
  echo "ERROR: dataset file missing: ${DATASET_PATH}" >&2
  exit 1
fi

export AOS_MODEL_CACHE_DIR="${ROOT_DIR}/var/models"
export AOS_BASE_MODEL_ID="${MODEL_ID}"

echo "[1/6] model seed"
"${AOSCTL}" models seed --model-path "${MODEL_PATH}" >/dev/null

echo "[2/6] dataset fingerprint"
shasum -a 256 "${DATASET_PATH}"

echo "[3/6] start training"
TRAIN_JSON="$(${AOSCTL} --json train start "${REPO_ID}" --branch "${BRANCH}" --base-model-id "${MODEL_ID}" --dataset-version-ids "${DATASET_VERSION_IDS}" --backend-policy "${BACKEND_POLICY}" --backend "${BACKEND}")"
echo "${TRAIN_JSON}"

JOB_ID="$(printf '%s' "${TRAIN_JSON}" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("id",""))')"
if [[ -z "${JOB_ID}" ]]; then
  echo "ERROR: failed to parse job_id from train start response" >&2
  exit 1
fi

echo "[4/6] training status"
STATUS_JSON="$("${AOSCTL}" --json train status "${JOB_ID}")"
echo "${STATUS_JSON}"

if [[ "${REQUIRE_GPU}" == "true" ]]; then
  BACKEND_USED="$(printf '%s' "${STATUS_JSON}" | python3 -c 'import json,sys; d=json.load(sys.stdin); print((d.get("backend") or "").lower())')"
  if [[ "${BACKEND_USED}" == "cpu" ]]; then
    echo "ERROR: full tuning landed on CPU backend; aborting hard gate." >&2
    exit 1
  fi
fi

echo "[5/6] determinism gate"
"${AOSCTL}" audit-determinism --backend mlx --model-path "${MODEL_PATH}"

echo "[6/6] report + gate endpoints (manual review required)"
"${AOSCTL}" --json train report --id "${JOB_ID}"
echo "CP promotion gate: ${BASE_URL}/v1/cp/promotion-gates/{cpid}"
echo "Golden gate: ${BASE_URL}/v1/golden/{run_id}/gates"

echo "DONE: job_id=${JOB_ID}"
