#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

die() {
  local msg="$1"
  local hint="${2:-}"
  echo "ERROR: ${msg}" >&2
  if [ -n "$hint" ]; then
    echo "HINT: ${hint}" >&2
  fi
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "Missing required command: $1"
}

require_cmd python3
require_cmd b3sum

AOSCTL="${ROOT_DIR}/aosctl"
if [ ! -x "$AOSCTL" ]; then
  cargo build -p adapteros-cli >/dev/null
  AOSCTL="${ROOT_DIR}/target/debug/aosctl"
fi

MODEL_CACHE_ROOT="${AOS_MODEL_CACHE_DIR:-${ROOT_DIR}/var/model-cache/models}"
if [ ! -d "$MODEL_CACHE_ROOT" ]; then
  die "Model cache root not found: ${MODEL_CACHE_ROOT}"
fi

MODEL_DIR="$(find "$MODEL_CACHE_ROOT" -mindepth 1 -maxdepth 1 -type d -print0 | \
  while IFS= read -r -d '' dir; do
    if [ -f "$dir/tokenizer.json" ] && [ -f "$dir/config.json" ]; then
      size_kb=$(du -sk "$dir" | awk '{print $1}')
      printf '%s\t%s\n' "$size_kb" "$dir"
    fi
  done | sort -n | head -n 1 | cut -f2-)"

if [ -z "$MODEL_DIR" ]; then
  die "No model directory with tokenizer.json found under ${MODEL_CACHE_ROOT}"
fi

export AOS_MODEL_CACHE_DIR="$MODEL_CACHE_ROOT"

TOKENIZER_PATH="${MODEL_DIR}/tokenizer.json"
MODEL_CONFIG_PATH="${MODEL_DIR}/config.json"
TOKENIZER_CONFIG_PATH="${MODEL_DIR}/tokenizer_config.json"

BASE_DIR="${ROOT_DIR}/var/tmp/plan4"
SUP_DIR="${BASE_DIR}/supervised"
RAW_DIR="${BASE_DIR}/raw"
TRAIN_DIR="${BASE_DIR}/train"
mkdir -p "$SUP_DIR" "$RAW_DIR" "$TRAIN_DIR"

SUP_JSONL="${SUP_DIR}/dataset.jsonl"
RAW_JSONL="${RAW_DIR}/dataset.jsonl"

cat > "$SUP_JSONL" <<EOF
{"prompt":"Hello","completion":"World"}
EOF

python3 - <<PY
import pathlib

base = pathlib.Path("${RAW_DIR}")
base.mkdir(parents=True, exist_ok=True)
payload = "hello " * 800
path = base / "dataset.jsonl"
path.write_text('{"text":"%s"}\n' % payload.strip())
PY

"$AOSCTL" dataset build "$SUP_JSONL" --tokenizer "$TOKENIZER_PATH" --output "$SUP_DIR/build" >/dev/null
echo "dataset accepted: supervised"

"$AOSCTL" dataset build "$RAW_JSONL" --tokenizer "$TOKENIZER_PATH" --output "$RAW_DIR/build" >/dev/null
echo "dataset accepted: raw"

SUP_EXAMPLES_JSONL="$SUP_DIR/build/examples.jsonl"

TRAIN_CONFIG="$TRAIN_DIR/train_config.json"

python3 - <<PY
import json
from pathlib import Path

model_cfg = json.loads(Path("${MODEL_CONFIG_PATH}").read_text())
tok_cfg_path = Path("${TOKENIZER_CONFIG_PATH}")
tok_cfg = json.loads(tok_cfg_path.read_text()) if tok_cfg_path.exists() else {}

hidden_dim = model_cfg.get("hidden_size") or model_cfg.get("hidden_dim") or model_cfg.get("n_embd") or 896
vocab_size = model_cfg.get("vocab_size") or tok_cfg.get("vocab_size") or 200000
pad_token_id = tok_cfg.get("pad_token_id", 0) or 0

config = {
    "rank": 2,
    "alpha": 4.0,
    "learning_rate": 0.001,
    "batch_size": 1,
    "epochs": 1,
    "hidden_dim": int(hidden_dim),
    "vocab_size": int(vocab_size),
    "training_contract_version": "1.0",
    "pad_token_id": int(pad_token_id),
    "ignore_index": 0,
    "use_gpu_backward": False,
    "validation_split": 0.0
}

Path("${TRAIN_CONFIG}").write_text(json.dumps(config))
PY

echo "training running: supervised"
"$AOSCTL" train local \
  --config "$TRAIN_CONFIG" \
  --data "$SUP_DIR/build" \
  --output "$TRAIN_DIR/out" \
  --base-model "$MODEL_DIR" >/dev/null

ADAPTER_ID="plan4-reference-$(date +%s)"
ADAPTER_PATH="${TRAIN_DIR}/${ADAPTER_ID}.aos"

"$AOSCTL" aos create \
  --source "$TRAIN_DIR/out/lora_weights.json" \
  --output "$ADAPTER_PATH" \
  --adapter-id "$ADAPTER_ID" \
  --training-data "$SUP_EXAMPLES_JSONL" >/dev/null

ADAPTER_HASH="$(b3sum "$ADAPTER_PATH" | awk '{print $1}')"
REGISTRY_DB="${ROOT_DIR}/var/adapters/registry.db"
mkdir -p "$(dirname "$REGISTRY_DB")"

python3 - <<PY
import json
import sqlite3

db = sqlite3.connect("${REGISTRY_DB}")
db.execute(
    """CREATE TABLE IF NOT EXISTS adapters (
        id TEXT PRIMARY KEY,
        hash TEXT NOT NULL,
        tier TEXT NOT NULL,
        rank INTEGER NOT NULL,
        acl TEXT NOT NULL,
        activation_pct REAL DEFAULT 0.0,
        registered_at TEXT NOT NULL,
        adapter_name TEXT,
        tenant_namespace TEXT,
        domain TEXT,
        purpose TEXT,
        revision INTEGER,
        parent_id TEXT,
        fork_type TEXT,
        fork_reason TEXT
    )"""
)

db.execute(
    "INSERT OR REPLACE INTO adapters (id, hash, tier, rank, acl, registered_at) VALUES (?, ?, ?, ?, ?, datetime('now'))",
    ("${ADAPTER_ID}", "${ADAPTER_HASH}", "default", 2, json.dumps([])),
)
db.commit()
db.close()
PY

echo "adapter registered: ${ADAPTER_ID}"
echo "adapter artifact: ${ADAPTER_PATH}"
