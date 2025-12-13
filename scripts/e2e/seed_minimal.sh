#!/usr/bin/env bash
set -euo pipefail

# Seed deterministic fixtures without dropping the database (idempotent upsert).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

cd "${REPO_ROOT}"

echo "[seed_minimal] seeding deterministic fixtures (skip reset)..."
cargo run -p adapteros-cli -- db seed-fixtures --skip-reset
echo "[seed_minimal] done"
