#!/usr/bin/env bash
# Migration check wrapper for tests/CI.
# Canonical role in migration checks:
# - scripts/db/check_migrations.sh: CI/test orchestration wrapper
# - scripts/check_migrations.sh: numbering/gap/collision checks
# - scripts/check-migrations.sh: signatures + duplicate-number gate
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LEGACY_CHECK="${ROOT_DIR}/scripts/check_migrations.sh"
MIGRATIONS_DIR="${ROOT_DIR}/migrations"
SIGNATURES_FILE="${MIGRATIONS_DIR}/signatures.json"

cd "${ROOT_DIR}"

if [[ ! -d "${MIGRATIONS_DIR}" ]]; then
  echo "❌ migrations directory not found at ${MIGRATIONS_DIR}" >&2
  exit 1
fi

if [[ ! -f "${SIGNATURES_FILE}" ]]; then
  echo "❌ signatures.json missing at ${SIGNATURES_FILE}" >&2
  exit 1
fi

echo "[db/check_migrations] running numbering/collision checks..."
bash "${LEGACY_CHECK}"

echo "[db/check_migrations] verifying migration signatures..."
if command -v python3 >/dev/null 2>&1 && python3 - <<'PY' >/dev/null 2>&1
import importlib.util, sys
sys.exit(0 if importlib.util.find_spec("blake3") else 1)
PY
then
  python3 "${ROOT_DIR}/scripts/verify_migration_signatures.py"
else
  echo "[db/check_migrations] python3 blake3 not available; falling back to cargo verification" >&2
  cargo run -p adapteros-cli -- db migrate --verify-only
fi

echo "[db/check_migrations] ✅ migration numbering and signatures are consistent"
