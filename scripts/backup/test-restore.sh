#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
TMP_ROOT="${REPO_ROOT}/var/tmp"
mkdir -p "${TMP_ROOT}"

BACKUP_ROOT="${AOS_BACKUP_ROOT:-/var/backups/aos}"
KEY_PATH="${AOS_BACKUP_KEY_PATH:-/etc/aos/backup.key}"
VERIFY_PUBKEY="${AOS_BACKUP_VERIFY_PUBKEY:-}"
REQUIRE_SIGNATURE="${AOS_BACKUP_REQUIRE_SIGNATURE:-0}"
TARGET_ROOT="${AOS_TARGET_ROOT:-$(mktemp -d "${TMP_ROOT}/aos-restore.XXXXXX")}"
TMP_DIR="$(mktemp -d "${TMP_ROOT}/aos-restore-work.XXXXXX")"
EXTRACT_DIR="${TMP_DIR}/extract"

log_json() {
  local level="$1"
  shift
  printf '{"ts":"%s","level":"%s","msg":"%s"}\n' \
    "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "${level}" "$*"
}

fail() {
  log_json "error" "$*"
  exit 1
}

SERVER_PID=""
cleanup() {
  if [ -n "${SERVER_PID}" ] && kill -0 "${SERVER_PID}" 2>/dev/null; then
    kill "${SERVER_PID}" 2>/dev/null || true
    wait "${SERVER_PID}" 2>/dev/null || true
  fi
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "Required command missing: $1"
}

for bin in openssl tar sqlite3 rsync shasum; do
  require_cmd "${bin}"
done

if command -v curl >/dev/null 2>&1; then
  HAS_CURL=1
else
  HAS_CURL=0
fi

if [ ! -s "${KEY_PATH}" ]; then
  fail "Backup key missing at ${KEY_PATH}"
fi

LATEST="$(ls -1t "${BACKUP_ROOT}"/aos-backup-*.tar.gz.enc 2>/dev/null | head -n 1 || true)"
if [ -z "${LATEST}" ]; then
  fail "No backup files found under ${BACKUP_ROOT}"
fi
SIG_PATH="${LATEST}.sig"

log_json "info" "Restoring from backup: ${LATEST}"

if [ -n "${VERIFY_PUBKEY}" ] && [ -f "${SIG_PATH}" ]; then
  if ! openssl dgst -sha256 -verify "${VERIFY_PUBKEY}" -signature "${SIG_PATH}" "${LATEST}" >/dev/null 2>&1; then
    fail "Signature verification failed for ${LATEST}"
  fi
  log_json "info" "Signature verified for ${LATEST}"
elif [ -f "${SIG_PATH}" ]; then
  log_json "warn" "Signature present but AOS_BACKUP_VERIFY_PUBKEY not set; skipping signature verification"
elif [ "${REQUIRE_SIGNATURE}" -eq 1 ]; then
  fail "Signature required but none present and no pubkey configured"
fi

DECRYPTED="${TMP_DIR}/bundle.tar.gz"

openssl enc -d -aes-256-gcm -pbkdf2 -iter 100000 -md sha256 \
  -in "${LATEST}" \
  -out "${DECRYPTED}" \
  -pass "file:${KEY_PATH}"

mkdir -p "${EXTRACT_DIR}"
tar -xzf "${DECRYPTED}" -C "${EXTRACT_DIR}"

DATA_DIR="${EXTRACT_DIR}"
DB_PATH="${DATA_DIR}/db/aos-cp.sqlite3"
if [ ! -f "${DB_PATH}" ]; then
  fail "SQLite DB missing in backup payload"
fi

INTEGRITY_RESULT="$(sqlite3 "${DB_PATH}" "PRAGMA integrity_check;")"
if [ "${INTEGRITY_RESULT}" != "ok" ]; then
  fail "SQLite integrity check failed: ${INTEGRITY_RESULT}"
fi

TARGET_VAR="${TARGET_ROOT}/var"
mkdir -p "${TARGET_VAR}" "${TARGET_ROOT}/configs"

rsync -a "${DATA_DIR}/db/" "${TARGET_VAR}/"
[ -e "${DATA_DIR}/kv" ] && rsync -a "${DATA_DIR}/kv/" "${TARGET_VAR}/"
[ -e "${DATA_DIR}/kv-index" ] && rsync -a "${DATA_DIR}/kv-index/" "${TARGET_VAR}/"
[ -e "${DATA_DIR}/tantivy" ] && rsync -a "${DATA_DIR}/tantivy/" "${TARGET_VAR}/"
[ -e "${DATA_DIR}/adapters" ] && rsync -a "${DATA_DIR}/adapters/" "${TARGET_VAR}/adapters/"
[ -e "${DATA_DIR}/artifacts" ] && rsync -a "${DATA_DIR}/artifacts/" "${TARGET_VAR}/artifacts/"
[ -e "${DATA_DIR}/model-cache" ] && rsync -a "${DATA_DIR}/model-cache/" "${TARGET_VAR}/model-cache/"
[ -e "${DATA_DIR}/configs" ] && rsync -a "${DATA_DIR}/configs/" "${TARGET_ROOT}/configs/"

log_json "info" "Restore files staged under ${TARGET_ROOT}"

SERVER_BIN="${AOS_SERVER_BIN:-adapteros-server}"
HEALTH_PORT="${AOS_RESTORE_HEALTH_PORT:-18080}"

if command -v "${SERVER_BIN}" >/dev/null 2>&1 && [ "${HAS_CURL}" -eq 1 ]; then
  log_json "info" "Starting health check server on port ${HEALTH_PORT}"
  AOS_DATABASE_URL="sqlite://${TARGET_VAR}/aos-cp.sqlite3" \
  AOS_KV_PATH="${TARGET_VAR}/aos-kv.redb" \
  AOS_KV_TANTIVY_PATH="${TARGET_VAR}/aos-kv-index" \
  AOS_TANTIVY_PATH="${TARGET_VAR}/aos-search" \
  AOS_ADAPTERS_DIR="${TARGET_VAR}/adapters" \
  AOS_ARTIFACTS_DIR="${TARGET_VAR}/artifacts" \
  AOS_MODEL_CACHE_DIR="${TARGET_VAR}/model-cache" \
  AOS_DEV_NO_AUTH=1 \
    "${SERVER_BIN}" --config "${TARGET_ROOT}/configs/cp.toml" --port "${HEALTH_PORT}" \
    >/dev/null 2>"${TMP_DIR}/server.log" &
  SERVER_PID=$!

  for _ in $(seq 1 45); do
    if curl -fs "http://127.0.0.1:${HEALTH_PORT}/health" >/dev/null 2>&1; then
      HEALTH_OK=1
      break
    fi
    sleep 1
  done

  if [ "${HEALTH_OK:-0}" -ne 1 ]; then
    fail "Health check did not succeed; see ${TMP_DIR}/server.log"
  fi

  log_json "info" "Health check passed on restored data"
else
  log_json "warn" "adapteros-server or curl not available; skipping live health check"
fi

log_json "info" "Restore test completed; data available at ${TARGET_ROOT}"
