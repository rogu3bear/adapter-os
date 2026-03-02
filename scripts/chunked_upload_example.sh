#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <file> [dataset_name]"
  exit 1
fi

FILE="$1"
DATASET_NAME="${2:-$(basename "$FILE")}"
BASE_URL="${AOS_BASE_URL:-http://localhost:8080}"
TOKEN="${AOS_TOKEN:-}"
CHUNK_SIZE="${CHUNK_SIZE:-10485760}"
DATASET_FORMAT="${AOS_DATASET_FORMAT:-jsonl}"

if [[ -z "$TOKEN" ]]; then
  echo "Set AOS_TOKEN for Authorization."
  exit 1
fi

for tool in b3sum curl jq uuidgen; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "Missing dependency: $tool"
    exit 1
  fi
done

if [[ ! -f "$FILE" ]]; then
  echo "File not found: $FILE"
  exit 1
fi

file_size() {
  if stat -f%z "$1" >/dev/null 2>&1; then
    stat -f%z "$1"
  else
    stat -c%s "$1"
  fi
}

FILE_SIZE="$(file_size "$FILE")"
FILE_HASH="$(b3sum "$FILE" | awk '{print $1}')"
IDEMPOTENCY_KEY="$(uuidgen | tr '[:upper:]' '[:lower:]')"
FILE_NAME="$(basename "$FILE")"

init_payload="$(jq -n \
  --arg file_name "$FILE_NAME" \
  --arg expected_file_hash_b3 "$FILE_HASH" \
  --arg idempotency_key "$IDEMPOTENCY_KEY" \
  --arg content_type "application/octet-stream" \
  --argjson total_size "$FILE_SIZE" \
  --argjson chunk_size "$CHUNK_SIZE" \
  '{file_name:$file_name,total_size:$total_size,chunk_size:$chunk_size,content_type:$content_type,idempotency_key:$idempotency_key,expected_file_hash_b3:$expected_file_hash_b3}')"

session_json="$(
  curl -sS "$BASE_URL/v1/datasets/chunked-upload/initiate" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "$init_payload"
)"

SESSION_ID="$(jq -r .session_id <<<"$session_json")"
CHUNK_SIZE="$(jq -r .chunk_size <<<"$session_json")"
EXPECTED_CHUNKS="$(jq -r .expected_chunks <<<"$session_json")"

if [[ -z "$SESSION_ID" || "$SESSION_ID" == "null" ]]; then
  echo "Failed to initiate upload session: $session_json"
  exit 1
fi

upload_chunk() {
  local index="$1"
  dd if="$FILE" bs="$CHUNK_SIZE" skip="$index" count=1 2>/dev/null | \
    curl -sS "$BASE_URL/v1/datasets/chunked-upload/$SESSION_ID/chunk?chunk_index=$index" \
      -H "Authorization: Bearer $TOKEN" \
      -H "Content-Type: application/octet-stream" \
      --data-binary @- >/dev/null
}

for ((i=0; i<EXPECTED_CHUNKS; i++)); do
  for attempt in 1 2 3; do
    if upload_chunk "$i"; then
      break
    fi
    if [[ "$attempt" -eq 3 ]]; then
      echo "Failed to upload chunk $i after $attempt attempts"
      exit 1
    fi
    sleep 1
  done
done

complete_payload="$(jq -n --arg name "$DATASET_NAME" --arg format "$DATASET_FORMAT" \
  '{name:$name,format:$format}')"

complete_json="$(
  curl -sS "$BASE_URL/v1/datasets/chunked-upload/$SESSION_ID/complete" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "$complete_payload"
)"
DATASET_ID="$(jq -r .dataset_id <<<"$complete_json")"

retry_json="$(
  curl -sS "$BASE_URL/v1/datasets/chunked-upload/$SESSION_ID/complete" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "$complete_payload"
)"
RETRY_DATASET_ID="$(jq -r .dataset_id <<<"$retry_json")"

if [[ "$DATASET_ID" == "null" || -z "$DATASET_ID" ]]; then
  echo "Completion failed: $complete_json"
  exit 1
fi

if [[ "$DATASET_ID" != "$RETRY_DATASET_ID" ]]; then
  echo "Completion retry returned a different dataset_id: $RETRY_DATASET_ID"
  exit 1
fi

echo "dataset_id: $DATASET_ID"
