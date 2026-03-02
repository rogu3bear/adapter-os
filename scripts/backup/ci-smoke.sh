#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
TMP_ROOT="${REPO_ROOT}/var/tmp"
mkdir -p "${TMP_ROOT}"
ROOT="$(mktemp -d "${TMP_ROOT}/aos-backup-ci.XXXXXX")"
DATA_ROOT="${ROOT}/data"
VAR_DIR="${DATA_ROOT}/var"
CONFIG_DIR="${DATA_ROOT}/configs"
BACKUP_ROOT="${ROOT}/backups"
TARGET_ROOT="${ROOT}/restore"
KEY_PATH="${ROOT}/backup.key"

cleanup() {
  if [ "${AOS_CI_KEEP_ARTIFACTS:-0}" != "1" ]; then
    rm -rf "${ROOT}"
  else
    echo "Keeping artifacts under ${ROOT}"
  fi
}
trap cleanup EXIT

mkdir -p "${VAR_DIR}" "${CONFIG_DIR}" "${BACKUP_ROOT}" "${TARGET_ROOT}" \
  "${VAR_DIR}/adapters" "${VAR_DIR}/artifacts" "${VAR_DIR}/model-cache" \
  "${VAR_DIR}/kv" "${VAR_DIR}/kv-index" "${VAR_DIR}/aos-search"

sqlite3 "${VAR_DIR}/aos-cp.sqlite3" "pragma journal_mode=wal; create table if not exists smoke(id integer primary key, v text); insert into smoke(v) values('ok');"

cat > "${CONFIG_DIR}/cp.toml" <<EOF
[db]
path = "${VAR_DIR}/aos-cp.sqlite3"
[database]
kv_path = "${VAR_DIR}/aos-kv.redb"
kv_tantivy_path = "${VAR_DIR}/kv-index"
[paths]
adapters_dir = "${VAR_DIR}/adapters"
artifacts_dir = "${VAR_DIR}/artifacts"
EOF

openssl rand -hex 64 > "${KEY_PATH}"
chmod 600 "${KEY_PATH}"

echo "Running backup -> verify -> restore smoke in ${ROOT}"

AOS_DATA_ROOT="${VAR_DIR}" \
AOS_CONFIG_PATH="${CONFIG_DIR}/cp.toml" \
AOS_BACKUP_ROOT="${BACKUP_ROOT}" \
AOS_BACKUP_KEY_PATH="${KEY_PATH}" \
AOS_BACKUP_RETENTION_DAYS=1 \
  bash "${SCRIPT_DIR}/backup.sh"

AOS_BACKUP_ROOT="${BACKUP_ROOT}" \
AOS_BACKUP_KEY_PATH="${KEY_PATH}" \
  bash "${SCRIPT_DIR}/verify-backups.sh"

AOS_BACKUP_ROOT="${BACKUP_ROOT}" \
AOS_BACKUP_KEY_PATH="${KEY_PATH}" \
AOS_TARGET_ROOT="${TARGET_ROOT}" \
  bash "${SCRIPT_DIR}/test-restore.sh"

echo "CI smoke completed successfully"
