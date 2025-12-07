#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

umask 077

DATA_ROOT="${AOS_DATA_ROOT:-${REPO_ROOT}/var}"
CONFIG_PATH="${AOS_CONFIG_PATH:-${REPO_ROOT}/configs/cp.toml}"
BACKUP_ROOT="${AOS_BACKUP_ROOT:-/var/backups/aos}"
KEY_PATH="${AOS_BACKUP_KEY_PATH:-/etc/aos/backup.key}"
KEY_ID_RAW="${AOS_BACKUP_KEY_ID:-default}"
RETENTION_DAYS="${AOS_BACKUP_RETENTION_DAYS:-7}"
RETENTION_BYTES="${AOS_BACKUP_RETENTION_BYTES:-0}"
OFFSITE_ROOT="${AOS_BACKUP_OFFSITE_ROOT:-}"
SIGN_KEY="${AOS_BACKUP_SIGN_KEY:-}"
SIGN_KEY_ID="${AOS_BACKUP_SIGN_KEY_ID:-}"
PRE_HOOK="${AOS_BACKUP_HOOK_PRE:-}"
POST_HOOK="${AOS_BACKUP_HOOK_POST:-}"
REQUIRE_OFFSITE="${AOS_BACKUP_REQUIRE_OFFSITE:-0}"
REQUIRE_SIGNING="${AOS_BACKUP_REQUIRE_SIGNING:-0}"
REQUIRE_QUIESCE="${AOS_BACKUP_REQUIRE_QUIESCE:-0}"
TIMESTAMP="$(date -u +"%Y%m%dT%H%M%SZ")"

sanitize_id() {
  echo "$1" | tr -cd 'A-Za-z0-9._-'
}
KEY_ID="$(sanitize_id "${KEY_ID_RAW}")"
if [ -z "${KEY_ID}" ]; then
  KEY_ID="default"
fi

BACKUP_NAME="aos-backup-${TIMESTAMP}-${KEY_ID}.tar.gz.enc"
BACKUP_FILE="${BACKUP_ROOT}/${BACKUP_NAME}"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/aos-backup.XXXXXX")"
SNAPSHOT_DIR="${TMP_DIR}/data"

log_json() {
  local level="$1"
  shift
  printf '{"ts":"%s","level":"%s","msg":"%s","backup":"%s"}\n' \
    "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "${level}" "$*" "${BACKUP_NAME}"
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

for bin in openssl sqlite3 tar rsync shasum; do
  require_cmd "${bin}"
done
if [ -n "${SIGN_KEY}" ]; then
  require_cmd openssl
fi

mkdir -p "${SNAPSHOT_DIR}" "${BACKUP_ROOT}"

if [ ! -s "${KEY_PATH}" ]; then
  fail "Backup key missing at ${KEY_PATH}"
fi
if [ "${REQUIRE_OFFSITE}" -eq 1 ] && [ -z "${OFFSITE_ROOT}" ]; then
  fail "Offsite target required (set AOS_BACKUP_OFFSITE_ROOT)"
fi
if [ "${REQUIRE_SIGNING}" -eq 1 ] && [ -z "${SIGN_KEY}" ]; then
  fail "Signing required (set AOS_BACKUP_SIGN_KEY)"
fi
if [ "${REQUIRE_QUIESCE}" -eq 1 ] && [ -z "${PRE_HOOK}" ]; then
  fail "Quiesce hook required (set AOS_BACKUP_HOOK_PRE)"
fi

resolve_path() {
  python3 - "$1" "$REPO_ROOT" <<'PY'
import os, sys
path = sys.argv[1]
root = sys.argv[2]
if path.startswith("~"):
    path = os.path.expanduser(path)
if os.path.isabs(path):
    print(os.path.abspath(path))
else:
    print(os.path.abspath(os.path.join(root, path)))
PY
}

resolve_sqlite_path() {
  local url="$1"
  local trimmed="${url%%\?*}"
  case "${trimmed}" in
    sqlite:///*) echo "${trimmed#sqlite://}";;
    sqlite:/*) echo "${trimmed#sqlite:}";;
    file:///*) echo "${trimmed#file://}";;
    *) echo "${trimmed}";;
  esac
}

copy_path() {
  local src="$1"
  local dest_rel="$2"
  local required="${3:-0}"

  if [ ! -e "${src}" ]; then
    if [ "${required}" -eq 1 ]; then
      fail "Required path missing: ${src}"
    else
      log_json "warn" "Optional path missing, skipping: ${src}"
      return
    fi
  fi

  mkdir -p "$(dirname "${SNAPSHOT_DIR}/${dest_rel}")"
  if [ -d "${src}" ]; then
    rsync -a "${src}/" "${SNAPSHOT_DIR}/${dest_rel}/"
  else
    rsync -a "${src}" "${SNAPSHOT_DIR}/${dest_rel}"
  fi
}

backup_sqlite() {
  local src="$1"
  local dest_rel="$2"
  mkdir -p "$(dirname "${SNAPSHOT_DIR}/${dest_rel}")"
  if [ ! -f "${src}" ]; then
    fail "SQLite source not found: ${src}"
  fi
  sqlite3 "${src}" ".backup '${SNAPSHOT_DIR}/${dest_rel}'"
}

DB_URL="${AOS_DATABASE_URL:-sqlite://${DATA_ROOT}/aos-cp.sqlite3}"
DB_PATH_RAW="$(resolve_sqlite_path "${DB_URL}")"
DB_PATH="$(resolve_path "${DB_PATH_RAW}")"
KV_PATH="$(resolve_path "${AOS_KV_PATH:-${DATA_ROOT}/aos-kv.redb}")"
KV_TANTIVY_PATH="$(resolve_path "${AOS_KV_TANTIVY_PATH:-${DATA_ROOT}/aos-kv-index}")"
TANTIVY_PATH="$(resolve_path "${AOS_TANTIVY_PATH:-${DATA_ROOT}/aos-search}")"
ADAPTERS_DIR="$(resolve_path "${AOS_ADAPTERS_DIR:-${DATA_ROOT}/adapters}")"
ARTIFACTS_DIR="$(resolve_path "${AOS_ARTIFACTS_DIR:-${DATA_ROOT}/artifacts}")"
MODEL_CACHE_DIR="$(resolve_path "${AOS_MODEL_CACHE_DIR:-${DATA_ROOT}/model-cache}")"
CONFIG_ABS="$(resolve_path "${CONFIG_PATH}")"

log_json "info" "Starting backup to ${BACKUP_FILE}"

backup_sqlite "${DB_PATH}" "db/aos-cp.sqlite3"
copy_path "${KV_PATH}" "kv/aos-kv.redb" 0
copy_path "${KV_TANTIVY_PATH}" "kv-index" 0
copy_path "${TANTIVY_PATH}" "tantivy" 0
copy_path "${ADAPTERS_DIR}" "adapters" 0
copy_path "${ARTIFACTS_DIR}" "artifacts" 0
copy_path "${MODEL_CACHE_DIR}" "model-cache" 0
copy_path "${CONFIG_ABS}" "configs/cp.toml" 1

if [ -n "${PRE_HOOK}" ]; then
  log_json "info" "Running pre-hook: ${PRE_HOOK}"
  bash -c "${PRE_HOOK}" || fail "Pre-hook failed"
fi

cat > "${TMP_DIR}/metadata.json" <<EOF
{
  "timestamp": "${TIMESTAMP}",
  "retention_days": ${RETENTION_DAYS},
  "retention_bytes": ${RETENTION_BYTES},
  "key_id": "${KEY_ID}",
  "db_path": "${DB_PATH}",
  "kv_path": "${KV_PATH}",
  "kv_tantivy_path": "${KV_TANTIVY_PATH}",
  "tantivy_path": "${TANTIVY_PATH}",
  "adapters_dir": "${ADAPTERS_DIR}",
  "artifacts_dir": "${ARTIFACTS_DIR}",
  "model_cache_dir": "${MODEL_CACHE_DIR}",
  "config_path": "${CONFIG_ABS}",
  "signing_key_id": "${SIGN_KEY_ID}",
  "git_ref": "$(git -C "${REPO_ROOT}" rev-parse --short HEAD 2>/dev/null || echo "unknown")"
}
EOF

(
  cd "${SNAPSHOT_DIR}"
  LC_ALL=C find . -type f -print | LC_ALL=C sort | while IFS= read -r file; do
    shasum -a 256 "${file}"
  done
) > "${TMP_DIR}/checksums.txt"

PAYLOAD="${TMP_DIR}/payload.tar.gz"
tar -czf "${PAYLOAD}" \
  -C "${TMP_DIR}" metadata.json checksums.txt \
  -C "${SNAPSHOT_DIR}" .

shasum -a 256 "${PAYLOAD}" > "${TMP_DIR}/payload.sha256"

openssl enc -aes-256-gcm -pbkdf2 -iter 100000 -md sha256 -salt \
  -in "${PAYLOAD}" \
  -out "${BACKUP_FILE}" \
  -pass "file:${KEY_PATH}"

if [ -n "${SIGN_KEY}" ]; then
  openssl dgst -sha256 -sign "${SIGN_KEY}" -out "${BACKUP_FILE}.sig" "${BACKUP_FILE}" || fail "Signing failed"
fi

find "${BACKUP_ROOT}" -maxdepth 1 -type f -name "aos-backup-*.tar.gz.enc" -mtime "+${RETENTION_DAYS}" -print -delete

if [ "${RETENTION_BYTES}" -gt 0 ]; then
  while :; do
    TOTAL_BYTES=$(find "${BACKUP_ROOT}" -maxdepth 1 -type f -name "aos-backup-*.tar.gz.enc" -printf "%s\n" 2>/dev/null | awk '{s+=$1} END{print s+0}')
    if [ "${TOTAL_BYTES}" -le "${RETENTION_BYTES}" ]; then
      break
    fi
    OLDEST=$(find "${BACKUP_ROOT}" -maxdepth 1 -type f -name "aos-backup-*.tar.gz.enc" -printf "%T@ %p\n" | sort -n | head -n1 | awk '{print $2}')
    if [ -z "${OLDEST}" ]; then
      break
    fi
    log_json "warn" "Pruning for size cap: ${OLDEST}"
    rm -f "${OLDEST}"
  done
fi

if [ -n "${OFFSITE_ROOT}" ]; then
  mkdir -p "${OFFSITE_ROOT}"
  rsync -a "${BACKUP_FILE}" "${OFFSITE_ROOT}/" || fail "Offsite copy failed"
  if [ -f "${BACKUP_FILE}.sig" ]; then
    rsync -a "${BACKUP_FILE}.sig" "${OFFSITE_ROOT}/" || fail "Offsite signature copy failed"
  fi
  log_json "info" "Offsite copy stored at ${OFFSITE_ROOT}/$(basename "${BACKUP_FILE}")"
fi

if [ -n "${POST_HOOK}" ]; then
  log_json "info" "Running post-hook: ${POST_HOOK}"
  bash -c "${POST_HOOK}" || fail "Post-hook failed"
fi

log_json "info" "Backup complete at ${BACKUP_FILE}"

