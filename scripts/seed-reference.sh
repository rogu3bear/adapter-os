#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

extract_sqlite_path() {
  local value="${1}"

  case "${value}" in
    sqlite://*) printf '%s' "${value#sqlite://}" ;;
    sqlite:*) printf '%s' "${value#sqlite:}" ;;
    *://*) return 1 ;;
    *) printf '%s' "${value}" ;;
  esac
}

ensure_sqlx_compile_schema_db() {
  local schema_db="${REPO_ROOT}/target/sqlx-compile-schema.sqlite3"
  local migrations_dir="${REPO_ROOT}/migrations"
  local migration

  if [[ -f "${schema_db}" ]]; then
    shopt -s nullglob
    for migration in "${migrations_dir}"/[0-9][0-9][0-9][0-9]_*.sql; do
      if [[ "${migration}" -nt "${schema_db}" ]]; then
        echo "[seed-reference] sqlx compile-time schema db is stale; rebuilding" >&2
        rm -f "${schema_db}"
        break
      fi
    done
    shopt -u nullglob
  fi

  if [[ -f "${schema_db}" ]]; then
    echo "${schema_db}"
    return 0
  fi

  mkdir -p "$(dirname "${schema_db}")"
  rm -f "${schema_db}"

  echo "[seed-reference] creating sqlx compile-time schema db: ${schema_db}" >&2

  shopt -s nullglob
  for migration in "${migrations_dir}"/[0-9][0-9][0-9][0-9]_*.sql; do
    sqlite3 -bail "${schema_db}" < "${migration}"
  done
  shopt -u nullglob

  echo "${schema_db}"
}

usage() {
  cat <<'USAGE'
Usage: seed-reference.sh [--db-path PATH] [--skip-migrate]

Seeds a deterministic pilot reference dataset into the adapterOS SQLite database.

Defaults:
  - DB path: $DB_PATH, else $AOS_DATABASE_URL / $DATABASE_URL (sqlite:.../sqlite://.../path), else ./var/aos-cp.sqlite3

Options:
  --db-path PATH    SQLite DB file path (not a postgres:// URL)
  --skip-migrate    Skip running `aosctl db migrate` before seeding
USAGE
}

DB_PATH="${DB_PATH:-}"
SKIP_MIGRATE=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --db-path)
      DB_PATH="${2:-}"
      shift 2
      ;;
    --skip-migrate)
      SKIP_MIGRATE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "[seed-reference] Unknown arg: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "${DB_PATH}" ]]; then
  if [[ -n "${AOS_DATABASE_URL:-}" ]]; then
    if ! DB_PATH="$(extract_sqlite_path "${AOS_DATABASE_URL}")"; then
      echo "[seed-reference] AOS_DATABASE_URL is not a sqlite URL/path: ${AOS_DATABASE_URL}" >&2
      echo "[seed-reference] Provide --db-path PATH or set AOS_DATABASE_URL to sqlite://..." >&2
      exit 2
    fi
  elif [[ -n "${DATABASE_URL:-}" ]]; then
    if ! DB_PATH="$(extract_sqlite_path "${DATABASE_URL}")"; then
      echo "[seed-reference] DATABASE_URL is not a sqlite URL/path: ${DATABASE_URL}" >&2
      echo "[seed-reference] Provide --db-path PATH or set DATABASE_URL to sqlite://..." >&2
      exit 2
    fi
  fi
else
  DB_PATH_RAW="${DB_PATH}"
  if ! DB_PATH="$(extract_sqlite_path "${DB_PATH_RAW}")"; then
    echo "[seed-reference] --db-path / DB_PATH must be a SQLite path (not a non-sqlite URL): ${DB_PATH_RAW}" >&2
    exit 2
  fi
fi

if [[ -z "${DB_PATH}" ]]; then
  DB_PATH="${REPO_ROOT}/var/aos-reference.sqlite3"
elif [[ "${DB_PATH}" != /* ]]; then
  DB_PATH="${REPO_ROOT}/${DB_PATH}"
fi

SEED_SQL="${REPO_ROOT}/seeds/pilot_reference.sqlite.sql"
if [[ ! -f "${SEED_SQL}" ]]; then
  echo "[seed-reference] Missing seed file: ${SEED_SQL}" >&2
  exit 1
fi

command -v sqlite3 >/dev/null 2>&1 || { echo "[seed-reference] Missing sqlite3" >&2; exit 1; }

mkdir -p "$(dirname "${DB_PATH}")"

cd "${REPO_ROOT}"

echo "[seed-reference] db_path=${DB_PATH}"

if [[ "${SKIP_MIGRATE}" -eq 0 ]]; then
  echo "[seed-reference] migrating schema via adapteros-cli..."

  if [[ -x "${REPO_ROOT}/aosctl" ]]; then
    "${REPO_ROOT}/aosctl" db migrate --db-path "${DB_PATH}"
  elif [[ -x "${REPO_ROOT}/target/release/aosctl" ]]; then
    "${REPO_ROOT}/target/release/aosctl" db migrate --db-path "${DB_PATH}"
  elif [[ -x "${REPO_ROOT}/target/debug/aosctl" ]]; then
    "${REPO_ROOT}/target/debug/aosctl" db migrate --db-path "${DB_PATH}"
  elif command -v aosctl >/dev/null 2>&1; then
    aosctl db migrate --db-path "${DB_PATH}"
  else
    SQLX_SCHEMA_DB="$(ensure_sqlx_compile_schema_db)"
    DATABASE_URL="sqlite://${SQLX_SCHEMA_DB}" cargo run -p adapteros-cli -- db migrate --db-path "${DB_PATH}"
  fi
fi

echo "[seed-reference] applying deterministic seed SQL..."
sqlite3 -bail "${DB_PATH}" < "${SEED_SQL}"

echo "[seed-reference] done"
