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
TMP_DIR="$(mktemp -d "${TMP_ROOT}/aos-verify.XXXXXX")"
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

cleanup() {
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "Required command missing: $1"
}

for bin in openssl tar sqlite3 shasum; do
  require_cmd "${bin}"
done

if [ ! -s "${KEY_PATH}" ]; then
  fail "Backup key missing at ${KEY_PATH}"
fi

LATEST="$(ls -1t "${BACKUP_ROOT}"/aos-backup-*.tar.gz.enc 2>/dev/null | head -n 1 || true)"
if [ -z "${LATEST}" ]; then
  fail "No backup files found under ${BACKUP_ROOT}"
fi

SIG_PATH="${LATEST}.sig"

log_json "info" "Verifying latest backup: ${LATEST}"

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

if openssl enc -d -aes-256-gcm -pbkdf2 -iter 100000 -md sha256 \
  -in "${LATEST}" \
  -out "${DECRYPTED}" \
  -pass "file:${KEY_PATH}" 2>/dev/null; then
  :
elif openssl enc -d -aes-256-cbc -pbkdf2 -iter 100000 -md sha256 \
  -in "${LATEST}" \
  -out "${DECRYPTED}" \
  -pass "file:${KEY_PATH}" 2>/dev/null; then
  log_json "warn" "Decrypted backup using aes-256-cbc fallback"
else
  fail "Unable to decrypt backup with supported ciphers"
fi

mkdir -p "${EXTRACT_DIR}"
tar -xzf "${DECRYPTED}" -C "${EXTRACT_DIR}"

if [ ! -f "${EXTRACT_DIR}/metadata.json" ]; then
  fail "metadata.json missing inside backup"
fi

if [ ! -f "${EXTRACT_DIR}/checksums.txt" ]; then
  fail "checksums.txt missing inside backup"
fi

(
  cd "${EXTRACT_DIR}"
  LC_ALL=C find . -type f \
    ! -path "./metadata.json" \
    ! -path "./checksums.txt" \
    ! -path "./checksums.current" \
    -print | LC_ALL=C sort | while IFS= read -r file; do
      shasum -a 256 "${file}"
    done
) > "${EXTRACT_DIR}/checksums.current"

diff -u "${EXTRACT_DIR}/checksums.txt" "${EXTRACT_DIR}/checksums.current" >/dev/null

DB_PATH="${EXTRACT_DIR}/db/aos-cp.sqlite3"
if [ ! -f "${DB_PATH}" ]; then
  fail "SQLite DB missing in backup payload"
fi

INTEGRITY_RESULT="$(sqlite3 "${DB_PATH}" "PRAGMA integrity_check;")"
if [ "${INTEGRITY_RESULT}" != "ok" ]; then
  fail "SQLite integrity check failed: ${INTEGRITY_RESULT}"
fi

log_json "info" "Backup verified successfully: ${LATEST}"
