#!/usr/bin/env bash
# Fast reset of the test database (drop + migrate + seed fixtures).
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DB_PATH="${DB_PATH:-${ROOT_DIR}/var/aos-cp.sqlite3}"

cd "${ROOT_DIR}"
mkdir -p "$(dirname "${DB_PATH}")"

SECONDS=0

echo "[db/reset_test_db] resetting database at ${DB_PATH}..."
cargo run -p adapteros-cli -- db reset --db-path "${DB_PATH}" --force

echo "[db/reset_test_db] seeding deterministic fixtures..."
cargo run -p adapteros-cli -- db seed-fixtures --db-path "${DB_PATH}" --skip-reset --chat false

ELAPSED="${SECONDS}"
if (( ELAPSED > 60 )); then
  echo "❌ test DB rebuild exceeded 60 seconds (${ELAPSED}s)" >&2
  exit 1
fi

echo "[db/reset_test_db] ✅ completed in ${ELAPSED}s"
