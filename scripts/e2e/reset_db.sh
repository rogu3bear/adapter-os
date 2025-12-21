#!/usr/bin/env bash
set -euo pipefail

# Pattern A: drop + recreate DB, then seed deterministic fixtures
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

cd "${REPO_ROOT}"

echo "[reset_db] resetting and seeding deterministic fixtures..."
cargo run -p adapteros-cli -- db seed-fixtures
echo "[reset_db] done"
