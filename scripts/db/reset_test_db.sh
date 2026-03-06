#!/usr/bin/env bash
# Fast reset of the test database (drop + migrate + seed fixtures).
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DB_PATH="${DB_PATH:-${ROOT_DIR}/var/aos-cp.sqlite3}"
AOSCTL_BIN="${ROOT_DIR}/target/debug/aosctl"

cd "${ROOT_DIR}"
mkdir -p "$(dirname "${DB_PATH}")"

echo "[db/reset_test_db] ensuring aosctl is built..."
cargo build -p adapteros-cli --quiet

SECONDS=0

echo "[db/reset_test_db] resetting database at ${DB_PATH}..."
"${AOSCTL_BIN}" db reset --db-path "${DB_PATH}" --force

echo "[db/reset_test_db] seeding deterministic fixtures..."
"${AOSCTL_BIN}" db seed-fixtures --db-path "${DB_PATH}" --skip-reset --chat false

ELAPSED="${SECONDS}"
if (( ELAPSED > 60 )); then
  echo "❌ test DB rebuild exceeded 60 seconds (${ELAPSED}s)" >&2
  exit 1
fi

echo "[db/reset_test_db] ✅ completed in ${ELAPSED}s"
