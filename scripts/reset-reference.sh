#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
  cat <<'USAGE'
Usage: reset-reference.sh [--db-path PATH] [--force]

Deletes the local SQLite DB (and WAL/SHM), then reseeds deterministic pilot reference data.

Options:
  --db-path PATH  SQLite DB file path (not a sqlite:// URL)
  --force         Skip interactive confirmation
USAGE
}

DB_PATH="${DB_PATH:-}"
FORCE=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --db-path)
      DB_PATH="${2:-}"
      shift 2
      ;;
    --force)
      FORCE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "[reset-reference] Unknown arg: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "${DB_PATH}" ]]; then
  if [[ -n "${AOS_DATABASE_URL:-}" ]]; then
    case "${AOS_DATABASE_URL}" in
      sqlite://*) DB_PATH="${AOS_DATABASE_URL#sqlite://}" ;;
      sqlite:*) DB_PATH="${AOS_DATABASE_URL#sqlite:}" ;;
    esac
  elif [[ -n "${DATABASE_URL:-}" ]]; then
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

if [[ "${DB_PATH}" == *"/prod"* || "${DB_PATH}" == *"production"* ]]; then
  echo "[reset-reference] Refusing to operate on production-like path: ${DB_PATH}" >&2
  exit 1
fi

echo "[reset-reference] db_path=${DB_PATH}"

if [[ "${FORCE}" -ne 1 ]]; then
  echo "[reset-reference] This will DELETE the database file and WAL/SHM:"
  echo "[reset-reference]   ${DB_PATH}"
  echo "[reset-reference] Type 'yes' to continue:"
  read -r confirm
  if [[ "${confirm}" != "yes" ]]; then
    echo "[reset-reference] cancelled"
    exit 0
  fi
fi

rm -f "${DB_PATH}" "${DB_PATH}-wal" "${DB_PATH}-shm"

echo "[reset-reference] reseeding..."
bash "${REPO_ROOT}/scripts/seed-reference.sh" --db-path "${DB_PATH}"

echo "[reset-reference] done"
