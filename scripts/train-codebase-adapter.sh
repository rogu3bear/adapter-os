#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

usage() {
  cat <<'USAGE'
Train a demo adapter from the current AdapterOS codebase and run one inference.

Pipeline:
  1) Build local codebase dataset (scripts/build-codebase-dataset.sh)
  2) Convert code chunks -> training JSONL (input == target)
  3) Upload + validate dataset via control-plane API
  4) Create/find adapter repository
  5) Start training job, poll until completion
  6) Promote produced adapter version
  7) Load adapter + run one inference and print output

Requirements:
  - AdapterOS control plane running (default: http://localhost:8080)
  - curl, jq, python3

Authentication:
  - If auth is enabled, set AOS_AUTH_TOKEN (or AOS_USERNAME/AOS_PASSWORD).
  - If running dev no-auth: start server with AOS_DEV_NO_AUTH=1 and no token is needed.

Usage:
  scripts/train-codebase-adapter.sh [OPTIONS]

Options:
  --base-url <url>        Control plane base URL (default: http://localhost:8080)
  --tenant <tenant>       Tenant ID (default: default)
  --token <token>         Bearer token (or set AOS_AUTH_TOKEN)
  --repo-name <name>      Adapter repository name (default: codebase-demo)
  --repo-id <id>          Use an existing adapter repo id (skips lookup/create)
  --branch <branch>       Target branch (default: main)
  --max-chunks <n>        Max codebase chunks to train on (default: 40; min: 10)
  --example-max-chars <n> Max chars per example after chunking (default: 4096)
  --rank <n>              LoRA rank (default: 4)
  --alpha <f>             LoRA alpha (default: 8)
  --epochs <n>            Training epochs (default: 1)
  --batch-size <n>        Batch size (default: 2)
  --learning-rate <f>     Learning rate (default: 0.0001)
  --timeout-sec <n>       Max seconds to wait for training (default: 1800)
  --prompt <text>         Inference prompt
  --max-tokens <n>        Inference max_tokens (default: 160)
  -h, --help              Show this help

Environment (equivalents):
  AOS_BASE_URL, AOS_TENANT_ID, AOS_AUTH_TOKEN, AOS_USERNAME, AOS_PASSWORD,
  AOS_ADAPTER_REPO_NAME, AOS_ADAPTER_REPO_ID, AOS_TARGET_BRANCH,
  AOS_CODEBASE_MAX_CHUNKS, AOS_CODEBASE_EXAMPLE_MAX_CHARS,
  AOS_TRAIN_RANK, AOS_TRAIN_ALPHA, AOS_TRAIN_EPOCHS,
  AOS_TRAIN_BATCH_SIZE, AOS_TRAIN_LEARNING_RATE, AOS_TRAIN_TIMEOUT_SEC,
  AOS_INFER_PROMPT, AOS_INFER_MAX_TOKENS

Examples:
  AOS_DEV_NO_AUTH=1 scripts/train-codebase-adapter.sh
  AOS_AUTH_TOKEN=... scripts/train-codebase-adapter.sh --max-chunks 60 --epochs 2
USAGE
}

die() {
  printf "error: %s\n" "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

step() {
  printf "\n==> %s\n" "$*"
}

info() {
  printf "[info] %s\n" "$*"
}

warn() {
  printf "[warn] %s\n" "$*" >&2
}

trim() {
  # shellcheck disable=SC2001
  printf "%s" "$1" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//'
}

json_escape_preview() {
  python3 - "$1" <<'PY'
import json
import sys

text = sys.argv[1]
print(json.dumps(text)[:200])
PY
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

BASE_URL="${AOS_BASE_URL:-http://localhost:8080}"
TENANT_ID="${AOS_TENANT_ID:-default}"
AUTH_TOKEN="${AOS_AUTH_TOKEN:-}"

REPO_NAME="${AOS_ADAPTER_REPO_NAME:-codebase-demo}"
REPO_ID="${AOS_ADAPTER_REPO_ID:-}"
TARGET_BRANCH="${AOS_TARGET_BRANCH:-main}"

MAX_CHUNKS="${AOS_CODEBASE_MAX_CHUNKS:-40}"
EXAMPLE_MAX_CHARS="${AOS_CODEBASE_EXAMPLE_MAX_CHARS:-4096}"

TRAIN_RANK="${AOS_TRAIN_RANK:-4}"
TRAIN_ALPHA="${AOS_TRAIN_ALPHA:-8}"
TRAIN_EPOCHS="${AOS_TRAIN_EPOCHS:-1}"
TRAIN_BATCH_SIZE="${AOS_TRAIN_BATCH_SIZE:-2}"
TRAIN_LEARNING_RATE="${AOS_TRAIN_LEARNING_RATE:-0.0001}"
TRAIN_TIMEOUT_SEC="${AOS_TRAIN_TIMEOUT_SEC:-1800}"

INFER_PROMPT="${AOS_INFER_PROMPT:-Explain (briefly) what AdapterOS is and how LoRA adapters are used in it.}"
INFER_MAX_TOKENS="${AOS_INFER_MAX_TOKENS:-160}"

while (($# > 0)); do
  case "$1" in
    --base-url) BASE_URL="$2"; shift 2 ;;
    --tenant) TENANT_ID="$2"; shift 2 ;;
    --token) AUTH_TOKEN="$2"; shift 2 ;;
    --repo-name) REPO_NAME="$2"; shift 2 ;;
    --repo-id) REPO_ID="$2"; shift 2 ;;
    --branch) TARGET_BRANCH="$2"; shift 2 ;;
    --max-chunks) MAX_CHUNKS="$2"; shift 2 ;;
    --example-max-chars) EXAMPLE_MAX_CHARS="$2"; shift 2 ;;
    --rank) TRAIN_RANK="$2"; shift 2 ;;
    --alpha) TRAIN_ALPHA="$2"; shift 2 ;;
    --epochs) TRAIN_EPOCHS="$2"; shift 2 ;;
    --batch-size) TRAIN_BATCH_SIZE="$2"; shift 2 ;;
    --learning-rate) TRAIN_LEARNING_RATE="$2"; shift 2 ;;
    --timeout-sec) TRAIN_TIMEOUT_SEC="$2"; shift 2 ;;
    --prompt) INFER_PROMPT="$2"; shift 2 ;;
    --max-tokens) INFER_MAX_TOKENS="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *)
      die "unknown option: $1 (use --help)"
      ;;
  esac
done

if [[ -z "${BASE_URL}" ]]; then
  die "BASE_URL is empty"
fi

MAX_CHUNKS="$(trim "$MAX_CHUNKS")"
EXAMPLE_MAX_CHARS="$(trim "$EXAMPLE_MAX_CHARS")"
TRAIN_RANK="$(trim "$TRAIN_RANK")"
TRAIN_ALPHA="$(trim "$TRAIN_ALPHA")"
TRAIN_EPOCHS="$(trim "$TRAIN_EPOCHS")"
TRAIN_BATCH_SIZE="$(trim "$TRAIN_BATCH_SIZE")"
TRAIN_LEARNING_RATE="$(trim "$TRAIN_LEARNING_RATE")"
TRAIN_TIMEOUT_SEC="$(trim "$TRAIN_TIMEOUT_SEC")"
INFER_MAX_TOKENS="$(trim "$INFER_MAX_TOKENS")"

[[ "$MAX_CHUNKS" =~ ^[0-9]+$ ]] || die "--max-chunks must be an integer"
[[ "$EXAMPLE_MAX_CHARS" =~ ^[0-9]+$ ]] || die "--example-max-chars must be an integer"
[[ "$TRAIN_RANK" =~ ^[0-9]+$ ]] || die "--rank must be an integer"
[[ "$TRAIN_EPOCHS" =~ ^[0-9]+$ ]] || die "--epochs must be an integer"
[[ "$TRAIN_BATCH_SIZE" =~ ^[0-9]+$ ]] || die "--batch-size must be an integer"
[[ "$TRAIN_TIMEOUT_SEC" =~ ^[0-9]+$ ]] || die "--timeout-sec must be an integer"
[[ "$INFER_MAX_TOKENS" =~ ^[0-9]+$ ]] || die "--max-tokens must be an integer"

if ((MAX_CHUNKS < 10)); then
  die "--max-chunks must be >= 10 (dataset validator requires at least 10 examples)"
fi
if ((EXAMPLE_MAX_CHARS < 256)); then
  die "--example-max-chars must be >= 256"
fi

AUTH_HEADER=""
if [[ -n "${AUTH_TOKEN}" ]]; then
  AUTH_HEADER="Authorization: Bearer ${AUTH_TOKEN}"
fi

api_json() {
  local method="$1"
  local path="$2"
  local body="${3:-}"

  local url="${BASE_URL}${path}"
  local tmp
  tmp="$(mktemp)"
  local http_code="000"

  local -a args
  args=(-sS -o "$tmp" -w "%{http_code}" -X "$method")
  if [[ -n "${AUTH_HEADER}" ]]; then
    args+=(-H "${AUTH_HEADER}")
  fi

  if [[ "$method" != "GET" && "$method" != "HEAD" ]]; then
    args+=(-H "Content-Type: application/json")
    args+=(-d "${body}")
  fi

  if ! http_code="$(curl "${args[@]}" "$url")"; then
    http_code="000"
  fi

  if [[ "$http_code" == "000" ]]; then
    local curl_err
    curl_err="$(cat "$tmp" 2>/dev/null || true)"
    rm -f "$tmp"
    die "request failed: ${method} ${url} (curl error). ${curl_err}"
  fi

  if [[ "$http_code" -lt 200 || "$http_code" -ge 300 ]]; then
    local resp
    resp="$(cat "$tmp" 2>/dev/null || true)"
    rm -f "$tmp"
    printf "%s\n" "$resp" >&2
    die "request failed: ${method} ${url} (HTTP ${http_code})"
  fi

  cat "$tmp"
  rm -f "$tmp"
}

api_json_soft() {
  # Like api_json, but does not exit on non-2xx. Writes body to stdout on 2xx.
  local method="$1"
  local path="$2"
  local body="${3:-}"

  local url="${BASE_URL}${path}"
  local tmp
  tmp="$(mktemp)"
  local http_code="000"

  local -a args
  args=(-sS -o "$tmp" -w "%{http_code}" -X "$method")
  if [[ -n "${AUTH_HEADER}" ]]; then
    args+=(-H "${AUTH_HEADER}")
  fi

  if [[ "$method" != "GET" && "$method" != "HEAD" ]]; then
    args+=(-H "Content-Type: application/json")
    args+=(-d "${body}")
  fi

  if ! http_code="$(curl "${args[@]}" "$url")"; then
    http_code="000"
  fi

  if [[ "$http_code" == "000" ]]; then
    rm -f "$tmp"
    return 1
  fi

  if [[ "$http_code" -lt 200 || "$http_code" -ge 300 ]]; then
    printf "%s\n" "$(cat "$tmp" 2>/dev/null || true)" >&2
    rm -f "$tmp"
    return 1
  fi

  cat "$tmp"
  rm -f "$tmp"
  return 0
}

api_multipart_upload_dataset() {
  local file_path="$1"
  local dataset_name="$2"
  local dataset_description="$3"

  [[ -f "$file_path" ]] || die "dataset file not found: $file_path"

  local url="${BASE_URL}/v1/datasets/upload"
  local tmp
  tmp="$(mktemp)"
  local http_code="000"

  local -a args
  args=(-sS -o "$tmp" -w "%{http_code}" -X POST)
  if [[ -n "${AUTH_HEADER}" ]]; then
    args+=(-H "${AUTH_HEADER}")
  fi
  args+=(
    -F "name=${dataset_name}"
    -F "description=${dataset_description}"
    -F "format=jsonl"
    -F "file=@${file_path}"
  )

  if ! http_code="$(curl "${args[@]}" "$url")"; then
    http_code="000"
  fi

  if [[ "$http_code" == "000" ]]; then
    local curl_err
    curl_err="$(cat "$tmp" 2>/dev/null || true)"
    rm -f "$tmp"
    die "request failed: POST ${url} (curl error). ${curl_err}"
  fi

  if [[ "$http_code" -lt 200 || "$http_code" -ge 300 ]]; then
    local resp
    resp="$(cat "$tmp" 2>/dev/null || true)"
    rm -f "$tmp"
    printf "%s\n" "$resp" >&2
    die "request failed: POST ${url} (HTTP ${http_code})"
  fi

  cat "$tmp"
  rm -f "$tmp"
}

maybe_login() {
  if [[ -n "${AUTH_TOKEN}" ]]; then
    return 0
  fi
  if [[ -z "${AOS_USERNAME:-}" || -z "${AOS_PASSWORD:-}" ]]; then
    return 0
  fi

  step "Authenticating via /v1/auth/login"
  local login_req
  login_req="$(jq -n --arg u "${AOS_USERNAME}" --arg p "${AOS_PASSWORD}" '{username: $u, password: $p}')"
  local resp
  if ! resp="$(api_json_soft "POST" "/v1/auth/login" "$login_req")"; then
    warn "login failed; continuing without auth token (set AOS_AUTH_TOKEN or use AOS_DEV_NO_AUTH=1)"
    return 0
  fi
  local token
  token="$(printf "%s" "$resp" | jq -r '.token // empty')"
  if [[ -z "$token" ]]; then
    warn "Login succeeded but no token field found; continuing without auth header"
    return 0
  fi

  AUTH_TOKEN="$token"
  AUTH_HEADER="Authorization: Bearer ${AUTH_TOKEN}"
  info "Authenticated (token acquired)"
}

step "Checking prerequisites"
need_cmd curl
need_cmd jq
need_cmd python3

step "Checking control plane connectivity (${BASE_URL})"
if ! curl -sS --max-time 2 "${BASE_URL}/healthz" >/dev/null 2>&1; then
  die "cannot reach control plane at ${BASE_URL}. Start it (e.g. make dev-no-auth) and retry."
fi

maybe_login || true

CODEBASE_DATASET_DIR="${REPO_ROOT}/var/datasets/codebase"
CODEBASE_MANIFEST="${CODEBASE_DATASET_DIR}/manifest.jsonl"
CODEBASE_CHUNKS_DIR="${CODEBASE_DATASET_DIR}/chunks"
TRAINING_JSONL="${CODEBASE_DATASET_DIR}/training.jsonl"

step "Building local codebase dataset"
"${REPO_ROOT}/scripts/build-codebase-dataset.sh" --out "${CODEBASE_DATASET_DIR}"
[[ -f "${CODEBASE_MANIFEST}" ]] || die "missing manifest after dataset build: ${CODEBASE_MANIFEST}"
[[ -d "${CODEBASE_CHUNKS_DIR}" ]] || die "missing chunks dir after dataset build: ${CODEBASE_CHUNKS_DIR}"

step "Generating training JSONL (${MAX_CHUNKS} chunks, input == target)"
python3 - "${CODEBASE_MANIFEST}" "${CODEBASE_DATASET_DIR}" "${TRAINING_JSONL}" "${MAX_CHUNKS}" "${EXAMPLE_MAX_CHARS}" <<'PY'
import json
import sys
from pathlib import Path

manifest_path = Path(sys.argv[1]).resolve()
dataset_dir = Path(sys.argv[2]).resolve()
out_path = Path(sys.argv[3]).resolve()
max_chunks = int(sys.argv[4])
max_chars = int(sys.argv[5])

chunks_dir = dataset_dir / "chunks"
if not manifest_path.is_file():
    raise SystemExit(f"manifest not found: {manifest_path}")
if not chunks_dir.is_dir():
    raise SystemExit(f"chunks dir not found: {chunks_dir}")

examples = []
with manifest_path.open("r", encoding="utf-8") as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        entry = json.loads(line)
        chunk_path = (dataset_dir / entry["chunk_path"]).resolve()
        if not chunk_path.is_file():
            continue
        text = chunk_path.read_text(encoding="utf-8", errors="replace")
        text = text.replace("\r\n", "\n").replace("\r", "\n").strip()
        if len(text) > max_chars:
            text = text[:max_chars].rstrip()
        if not text:
            continue
        examples.append(
            {
                "input": text,
                "target": text,
                "metadata": {
                    "chunk_id": entry.get("chunk_id"),
                    "file_path": entry.get("file_path"),
                    "start_line": entry.get("start_line"),
                    "end_line": entry.get("end_line"),
                    "language": entry.get("language"),
                    "schema_version": entry.get("schema_version"),
                    "dataset_sha256": None,
                },
            }
        )
        if len(examples) >= max_chunks:
            break

if len(examples) < 10:
    raise SystemExit(f"need at least 10 examples; got {len(examples)} (increase --max-chunks)")

out_path.parent.mkdir(parents=True, exist_ok=True)
with out_path.open("w", encoding="utf-8", newline="\n") as f:
    for ex in examples:
        f.write(json.dumps(ex, ensure_ascii=False))
        f.write("\n")

print(f"wrote {len(examples)} examples to {out_path}")
PY

step "Uploading dataset to control plane"
DATASET_NAME="codebase-demo-$(date +%Y%m%d-%H%M%S)"
DATASET_DESC="Demo: codebase chunks (input == target) from AdapterOS repository"
DATASET_UPLOAD_RESP="$(api_multipart_upload_dataset "${TRAINING_JSONL}" "${DATASET_NAME}" "${DATASET_DESC}")"
DATASET_ID="$(printf "%s" "$DATASET_UPLOAD_RESP" | jq -r '.dataset_id // empty')"
[[ -n "${DATASET_ID}" ]] || die "dataset upload succeeded but no dataset_id found"
info "dataset_id=${DATASET_ID}"

step "Validating dataset (${DATASET_ID})"
VALIDATE_RESP="$(api_json "POST" "/v1/datasets/${DATASET_ID}/validate" '{}')"
VALIDATION_STATUS="$(printf "%s" "$VALIDATE_RESP" | jq -r '.validation_status // empty')"
IS_VALID="$(printf "%s" "$VALIDATE_RESP" | jq -r '.is_valid // empty')"
info "validation_status=${VALIDATION_STATUS} is_valid=${IS_VALID}"
if [[ "${IS_VALID}" != "true" ]]; then
  printf "%s\n" "$VALIDATE_RESP" >&2
  die "dataset validation failed (dataset_id=${DATASET_ID})"
fi

step "Resolving dataset_version_id for training"
VERSIONS_RESP="$(api_json "GET" "/v1/datasets/${DATASET_ID}/versions")"
DATASET_VERSION_ID="$(printf "%s" "$VERSIONS_RESP" | jq -r '.versions | sort_by(.version_number) | last | .dataset_version_id // empty')"
[[ -n "${DATASET_VERSION_ID}" ]] || die "no dataset versions found for dataset_id=${DATASET_ID}"
info "dataset_version_id=${DATASET_VERSION_ID}"

if [[ -z "${REPO_ID}" ]]; then
  step "Finding/creating adapter repository (${REPO_NAME})"
  REPOS_RESP="$(api_json "GET" "/v1/adapter-repositories")"
  REPO_ID="$(printf "%s" "$REPOS_RESP" | jq -r --arg name "${REPO_NAME}" '[.[] | select(.name == $name)][0].id // empty')"
  if [[ -z "${REPO_ID}" ]]; then
    CREATE_REPO_REQ="$(jq -n \
      --arg tenant_id "${TENANT_ID}" \
      --arg name "${REPO_NAME}" \
      --arg desc "Demo repo for codebase adapter training" \
      --arg branch "${TARGET_BRANCH}" \
      '{tenant_id: $tenant_id, name: $name, description: $desc, default_branch: $branch}')"
    CREATE_REPO_RESP="$(api_json "POST" "/v1/adapter-repositories" "${CREATE_REPO_REQ}")"
    REPO_ID="$(printf "%s" "$CREATE_REPO_RESP" | jq -r '.repo_id // empty')"
    [[ -n "${REPO_ID}" ]] || die "repo create succeeded but no repo_id found"
    info "created repo_id=${REPO_ID}"
  else
    info "using existing repo_id=${REPO_ID}"
  fi
else
  info "using provided repo_id=${REPO_ID}"
fi

step "Starting training job"
ADAPTER_NAME="codebase-demo-$(date +%Y%m%d-%H%M%S)"
TRAIN_DESC="Demo: train adapter from codebase chunks dataset_id=${DATASET_ID}"

TRAIN_REQ="$(jq -n \
  --arg adapter_name "${ADAPTER_NAME}" \
  --arg repo_id "${REPO_ID}" \
  --arg branch "${TARGET_BRANCH}" \
  --arg dataset_id "${DATASET_ID}" \
  --arg dataset_version_id "${DATASET_VERSION_ID}" \
  --arg desc "${TRAIN_DESC}" \
  --argjson rank "${TRAIN_RANK}" \
  --argjson alpha "${TRAIN_ALPHA}" \
  --argjson epochs "${TRAIN_EPOCHS}" \
  --argjson lr "${TRAIN_LEARNING_RATE}" \
  --argjson bs "${TRAIN_BATCH_SIZE}" \
  '{
    adapter_name: $adapter_name,
    repo_id: $repo_id,
    target_branch: $branch,
    dataset_id: $dataset_id,
    dataset_version_ids: [{dataset_version_id: $dataset_version_id, weight: 1.0}],
    category: "codebase",
    description: $desc,
    config: {
      rank: $rank,
      alpha: $alpha,
      epochs: $epochs,
      learning_rate: $lr,
      batch_size: $bs
    },
    post_actions: {
      package: true,
      register: true,
      create_stack: false,
      tier: "warm"
    }
  }')"

TRAIN_START_RESP="$(api_json "POST" "/v1/training/start" "${TRAIN_REQ}")"
JOB_ID="$(printf "%s" "$TRAIN_START_RESP" | jq -r '.id // .job_id // empty')"
[[ -n "${JOB_ID}" ]] || die "training start succeeded but no job id found"
info "job_id=${JOB_ID}"

STARTED_AT="$(date +%s)"
LAST_STATUS=""
FINAL_JOB_JSON=""

step "Polling training job (timeout ${TRAIN_TIMEOUT_SEC}s)"
while true; do
  JOB_JSON="$(api_json "GET" "/v1/training/jobs/${JOB_ID}")"
  FINAL_JOB_JSON="$JOB_JSON"

  STATUS="$(printf "%s" "$JOB_JSON" | jq -r '.status // empty | ascii_downcase')"
  PROGRESS="$(printf "%s" "$JOB_JSON" | jq -r '.progress_pct // empty')"
  CUR_EPOCH="$(printf "%s" "$JOB_JSON" | jq -r '.current_epoch // empty')"
  TOT_EPOCHS="$(printf "%s" "$JOB_JSON" | jq -r '.total_epochs // empty')"
  LOSS="$(printf "%s" "$JOB_JSON" | jq -r '.current_loss // empty')"
  BACKEND="$(printf "%s" "$JOB_JSON" | jq -r '.backend // empty')"

  if [[ "${STATUS}" != "${LAST_STATUS}" ]]; then
    info "status=${STATUS} progress=${PROGRESS}% epoch=${CUR_EPOCH}/${TOT_EPOCHS} loss=${LOSS} backend=${BACKEND}"
    LAST_STATUS="${STATUS}"
  fi

  case "${STATUS}" in
    completed|ready)
      break
      ;;
    failed|cancelled|canceled)
      break
      ;;
    pending|running|training|"")
      ;;
    *)
      warn "unknown status token: ${STATUS}"
      ;;
  esac

  NOW="$(date +%s)"
  if ((NOW - STARTED_AT > TRAIN_TIMEOUT_SEC)); then
    printf "%s\n" "$JOB_JSON" >&2
    die "training timeout exceeded (${TRAIN_TIMEOUT_SEC}s); job_id=${JOB_ID}"
  fi

  sleep 5
done

FINAL_STATUS="$(printf "%s" "$FINAL_JOB_JSON" | jq -r '.status // empty | ascii_downcase')"
if [[ "${FINAL_STATUS}" == "failed" || "${FINAL_STATUS}" == "cancelled" || "${FINAL_STATUS}" == "canceled" ]]; then
  ERROR_MSG="$(printf "%s" "$FINAL_JOB_JSON" | jq -r '.error_message // empty')"
  warn "training failed: ${ERROR_MSG}"
  warn "job_id=${JOB_ID} repo_id=${REPO_ID} dataset_id=${DATASET_ID}"
  printf "%s\n" "$FINAL_JOB_JSON" >&2
  warn "try: curl -sS ${BASE_URL}/v1/training/jobs/${JOB_ID} | jq ."
  exit 5
fi

VERSION_ID="$(printf "%s" "$FINAL_JOB_JSON" | jq -r '.produced_version_id // .adapter_version_id // .draft_version_id // empty')"
ADAPTER_ID="$(printf "%s" "$FINAL_JOB_JSON" | jq -r '.adapter_id // empty')"

[[ -n "${VERSION_ID}" ]] || die "training completed but no produced version id found (job_id=${JOB_ID})"
[[ -n "${ADAPTER_ID}" ]] || die "training completed but no adapter_id found (job_id=${JOB_ID}); ensure post_actions.register is supported/enabled"
info "produced_version_id=${VERSION_ID}"
info "adapter_id=${ADAPTER_ID}"

step "Promoting produced version to active (repo/version)"
api_json "POST" "/v1/training/repos/${REPO_ID}/versions/${VERSION_ID}/promote" '{}' >/dev/null

VERSION_RESP="$(api_json "GET" "/v1/adapter-versions/${VERSION_ID}")"
VERSION_LABEL="$(printf "%s" "$VERSION_RESP" | jq -r '.version // empty')"
VERSION_BRANCH="$(printf "%s" "$VERSION_RESP" | jq -r '.branch // empty')"
RELEASE_STATE="$(printf "%s" "$VERSION_RESP" | jq -r '.release_state // empty')"
SERVEABLE="$(printf "%s" "$VERSION_RESP" | jq -r '.serveable // empty')"
SERVEABLE_REASON="$(printf "%s" "$VERSION_RESP" | jq -r '.serveable_reason // empty')"
info "repo_id=${REPO_ID} version=${VERSION_LABEL} branch=${VERSION_BRANCH} release_state=${RELEASE_STATE} serveable=${SERVEABLE}"
if [[ -n "${SERVEABLE_REASON}" && "${SERVEABLE_REASON}" != "null" ]]; then
  info "serveable_reason=${SERVEABLE_REASON}"
fi

step "Loading adapter (${ADAPTER_ID})"
if ! api_json_soft "POST" "/v1/adapters/${ADAPTER_ID}/load" '{}'; then
  warn "adapter load failed; continuing (inference may still succeed via lazy-load)"
fi

step "Running inference with trained adapter"
INFER_REQ="$(jq -n \
  --arg prompt "${INFER_PROMPT}" \
  --arg adapter_id "${ADAPTER_ID}" \
  --argjson max_tokens "${INFER_MAX_TOKENS}" \
  '{
    prompt: $prompt,
    adapters: [$adapter_id],
    max_tokens: $max_tokens
  }')"

INFER_RESP="$(api_json "POST" "/v1/infer" "${INFER_REQ}")"
INFER_TEXT="$(printf "%s" "$INFER_RESP" | jq -r '.text // empty')"
if [[ -z "${INFER_TEXT}" ]]; then
  printf "%s\n" "$INFER_RESP" >&2
  die "inference succeeded but response contained no .text"
fi

printf "\n---\n"
printf "repo_id: %s\n" "${REPO_ID}"
printf "version_id: %s\n" "${VERSION_ID}"
printf "adapter_id: %s\n" "${ADAPTER_ID}"
printf "prompt: %s\n" "${INFER_PROMPT}"
printf "\nresponse:\n%s\n" "${INFER_TEXT}"
