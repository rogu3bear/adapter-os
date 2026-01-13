#!/usr/bin/env bash
# AdapterOS one-command test runner
# - Validates minimal environment
# - Checks toolchain versions
# - Runs formatter, lint, unit, and integration suites in order

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

SUITE="${1:-all}"

info() {
  echo "==> $*"
}

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

require_cmd() {
  local cmd="$1"
  local hint="$2"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    fail "Missing required tool '$cmd'. ${hint}"
  fi
}

run_cmd() {
  local desc="$1"
  shift
  local cmd="$*"
  echo ""
  echo "→ $desc"
  echo "   $cmd"
  bash -c "$cmd"
}

info "AdapterOS test runner (suite: ${SUITE})"

require_cmd "cargo" "Install Rust toolchain (rustup recommended)."

info "cargo version: $(cargo -V)"
if command -v rustc >/dev/null 2>&1; then
  info "rustc version: $(rustc -V)"
fi

# Load environment using unified loader
SCRIPT_DIR="$ROOT_DIR"
source "$ROOT_DIR/scripts/lib/env-loader.sh"

if ! check_env_file ".env"; then
  fail "Missing .env. Copy .env.example to .env and fill required values."
fi

load_env_file ".env" --no-override
if [ -f ".env.local" ]; then
  load_env_file ".env.local" --no-override
fi

REQUIRED_ENV_VARS=("AOS_DATABASE_URL")
for var in "${REQUIRED_ENV_VARS[@]}"; do
  if [ -z "${!var:-}" ]; then
    fail "Environment variable '$var' is required for tests. Update .env/.env.local."
  fi
done

check_db() {
  local url="$1"
  info "Checking database configuration: $url"

  if [[ "$url" == sqlite:* ]]; then
    local path="${url#sqlite:}"
    local dir
    dir="$(dirname "$path")"

    if [ ! -d "$dir" ]; then
      info "Creating SQLite directory: $dir"
      mkdir -p "$dir"
    fi

    if [ ! -f "$path" ]; then
      info "SQLite file not found yet at $path (will be created by migrations/tests)."
      return 0
    fi

    if command -v python3 >/dev/null 2>&1; then
      python3 - <<'PY' "$path"
import sqlite3, sys, pathlib

db_path = pathlib.Path(sys.argv[1])
try:
    sqlite3.connect(f"file:{db_path}?mode=ro", uri=True).close()
except Exception as exc:  # pragma: no cover
    print(f"Database check failed for {db_path}: {exc}")
    raise SystemExit(1)
PY
      info "SQLite reachable at $path"
    else
      info "python3 not available; skipping SQLite open check (not silent)."
    fi
  else
    info "Non-SQLite database URL detected; connectivity probe not implemented."
  fi
}

check_db "$AOS_DATABASE_URL"

info "Running migration hygiene gate..."
bash "${ROOT_DIR}/scripts/db/check_migrations.sh"

run_rust_suite() {
  run_cmd "Reset test database" "bash ${ROOT_DIR}/scripts/db/reset_test_db.sh"
  run_cmd "Tracing import check" "bash ${ROOT_DIR}/scripts/check_tracing_imports.sh"
  run_cmd "Rust fmt check" "cargo fmt --all -- --check"
  run_cmd "Rust lint (clippy)" "cargo clippy --workspace --all-features --all-targets -- -D warnings"
  run_cmd "Rust unit tests" "cargo test --workspace --exclude adapteros-lora-mlx-ffi --lib --bins --examples"
  run_cmd "Rust integration tests" "cargo test --workspace --exclude adapteros-lora-mlx-ffi --tests"
  run_cmd "Rust Miri (aos_worker library)" "cargo miri test --lib adapteros_lora_worker"
}

run_ui_suite() {
  run_cmd "Leptos UI unit tests" "cargo test -p adapteros-ui --lib"
}



case "$SUITE" in
  all)
    run_rust_suite
    run_ui_suite
    ;;
  rust)
    run_rust_suite
    ;;
  ui)
    run_ui_suite
    ;;
  *)
    fail "Unknown suite '$SUITE'. Use: all | rust | ui"
    ;;
esac
