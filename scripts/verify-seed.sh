#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
  cat <<'USAGE'
Usage: verify-seed.sh [--db-path PATH]

Verifies the deterministic pilot reference seed exists in the SQLite DB.

Options:
  --db-path PATH  SQLite DB file path (not a sqlite:// URL)
USAGE
}

DB_PATH="${DB_PATH:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --db-path)
      DB_PATH="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "[verify-seed] Unknown arg: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "${DB_PATH}" ]]; then
  if [[ -n "${DATABASE_URL:-}" ]]; then
    case "${DATABASE_URL}" in
      sqlite://*) DB_PATH="${DATABASE_URL#sqlite://}" ;;
      sqlite:*) DB_PATH="${DATABASE_URL#sqlite:}" ;;
    esac
  fi
fi

if [[ -z "${DB_PATH}" ]]; then
  DB_PATH="${REPO_ROOT}/var/aos-reference.sqlite3"
elif [[ "${DB_PATH}" != /* ]]; then
  DB_PATH="${REPO_ROOT}/${DB_PATH}"
fi

command -v sqlite3 >/dev/null 2>&1 || { echo "[verify-seed] Missing sqlite3" >&2; exit 1; }

if [[ ! -f "${DB_PATH}" ]]; then
  echo "[verify-seed] DB file not found: ${DB_PATH}" >&2
  echo "[verify-seed] Run: bash scripts/reset-reference.sh --force" >&2
  exit 1
fi

echo "[verify-seed] db_path=${DB_PATH}"

tenant_id="00000000-0000-4000-8000-000000000001"
admin_user_id="00000000-0000-4000-8000-000000000002"
base_model_id="00000000-0000-4000-8000-000000000003"
repo_id="00000000-0000-4000-8000-000000000004"
adapter_version_id="00000000-0000-4000-8000-000000000005"
training_job_id="00000000-0000-4000-8000-000000000006"
stack_id="00000000-0000-4000-8000-000000000007"

expect_one() {
  local label="$1"
  local sql="$2"
  local got
  got="$(sqlite3 "${DB_PATH}" "${sql}" | tr -d '\r' | head -n 1)"
  if [[ "${got}" != "1" ]]; then
    echo "[verify-seed] FAIL: ${label} (expected 1, got '${got:-<empty>}')" >&2
    echo "[verify-seed] Hint: bash scripts/reset-reference.sh --force" >&2
    exit 1
  fi
  echo "[verify-seed] OK: ${label}"
}

expect_min() {
  local label="$1"
  local sql="$2"
  local min="$3"
  local got
  got="$(sqlite3 "${DB_PATH}" "${sql}" | tr -d '\r' | head -n 1)"
  if [[ -z "${got}" ]] || ! [[ "${got}" =~ ^[0-9]+$ ]] || [[ "${got}" -lt "${min}" ]]; then
    echo "[verify-seed] FAIL: ${label} (expected >=${min}, got '${got:-<empty>}')" >&2
    echo "[verify-seed] Hint: bash scripts/reset-reference.sh --force" >&2
    exit 1
  fi
  echo "[verify-seed] OK: ${label} (count=${got})"
}

# UI expects these entity categories to be non-empty for the seeded tenant.
expect_min ">=1 model" \
  "SELECT COUNT(1) FROM models WHERE tenant_id='${tenant_id}';" \
  1
expect_min ">=1 repo" \
  "SELECT COUNT(1) FROM adapter_repositories WHERE tenant_id='${tenant_id}';" \
  1
expect_min ">=1 adapter version" \
  "SELECT COUNT(1) FROM adapter_versions WHERE tenant_id='${tenant_id}';" \
  1
expect_min ">=1 training job" \
  "SELECT COUNT(1) FROM repository_training_jobs WHERE tenant_id='${tenant_id}';" \
  1
expect_min ">=1 stack" \
  "SELECT COUNT(1) FROM adapter_stacks WHERE tenant_id='${tenant_id}';" \
  1

expect_one "tenant exists (${tenant_id})" \
  "SELECT COUNT(1) FROM tenants WHERE id='${tenant_id}' AND name='pilot-reference';"
expect_one "admin user exists (${admin_user_id})" \
  "SELECT COUNT(1) FROM users WHERE id='${admin_user_id}' AND tenant_id='${tenant_id}' AND email='reference@example.com' AND role='admin';"
expect_one "base model exists (${base_model_id})" \
  "SELECT COUNT(1) FROM models WHERE id='${base_model_id}' AND tenant_id='${tenant_id}';"
expect_one "adapter repository exists (${repo_id})" \
  "SELECT COUNT(1) FROM adapter_repositories WHERE id='${repo_id}' AND tenant_id='${tenant_id}';"
expect_one "adapter version exists (${adapter_version_id})" \
  "SELECT COUNT(1) FROM adapter_versions WHERE id='${adapter_version_id}' AND tenant_id='${tenant_id}';"
expect_one "training job exists (${training_job_id})" \
  "SELECT COUNT(1) FROM repository_training_jobs WHERE id='${training_job_id}' AND tenant_id='${tenant_id}';"
expect_one "default stack exists (${stack_id})" \
  "SELECT COUNT(1) FROM adapter_stacks WHERE id='${stack_id}' AND tenant_id='${tenant_id}';"

echo "[verify-seed] PASS"
