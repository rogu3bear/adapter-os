#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_ROOT"

DATABASE_URL="${DATABASE_URL:-sqlite://./var/sqlx-dev.sqlite3}"
SQLX_OFFLINE_DIR="${SQLX_OFFLINE_DIR:-crates/adapteros-db/.sqlx}"
SQLX_CHECK_CRATES="${SQLX_CHECK_CRATES:-adapteros-db adapteros-server-api adapteros-server}"
SQLX_PREPARE_CRATES="${SQLX_PREPARE_CRATES:-$SQLX_CHECK_CRATES}"

if ! cargo sqlx --version >/dev/null 2>&1; then
  echo "Error: sqlx-cli is required. Install with:" >&2
  echo "  cargo install sqlx-cli --version 0.8.2 --no-default-features --features sqlite" >&2
  exit 1
fi

mkdir -p "$REPO_ROOT/var"
mkdir -p "$SQLX_OFFLINE_DIR"

echo "[sqlx] DATABASE_URL=${DATABASE_URL}"
echo "[sqlx] SQLX_OFFLINE_DIR=${SQLX_OFFLINE_DIR}"
echo "[sqlx] SQLX_PREPARE_CRATES=${SQLX_PREPARE_CRATES}"

echo "[sqlx] Running migrations..."
DATABASE_URL="$DATABASE_URL" cargo sqlx migrate run

echo "[sqlx] Preparing offline cache..."
unset SQLX_OFFLINE
prepare_args=()
for crate in $SQLX_PREPARE_CRATES; do
  prepare_args+=("--package" "$crate")
done
SQLX_OFFLINE_DIR="$SQLX_OFFLINE_DIR" DATABASE_URL="$DATABASE_URL" \
  cargo sqlx prepare --workspace -- "${prepare_args[@]}"

echo "[sqlx] Verifying offline builds..."
for crate in $SQLX_CHECK_CRATES; do
  SQLX_OFFLINE=1 SQLX_OFFLINE_DIR="$SQLX_OFFLINE_DIR" \
    cargo check -p "$crate"
done

echo "[sqlx] Done. Commit updates in $SQLX_OFFLINE_DIR if they changed."
