#!/usr/bin/env bash
# adapterOS database quick checker
#
# Checks:
# - Required tables exist
# - Required seed rows exist
# - Foreign keys not broken (best-effort; sqlite only)
#
# Usage:
#   bash scripts/check-db.sh [--db <path|sqlite-url>] [--no-dotenv] [--integrity-check]
#
# Exit codes:
#   0  PASS (no failures)
#   1  FAIL (one or more failures)
#   2  Usage error

set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR" || exit 1

usage() {
  cat <<'EOF'
Usage: bash scripts/check-db.sh [--db <path|sqlite-url>] [--no-dotenv] [--integrity-check]

Options:
  --db <path|sqlite-url>   Database path or sqlite URL (default: AOS_DATABASE_URL/DATABASE_URL/DB_PATH/./var/aos-cp.sqlite3)
  --no-dotenv              Do not source .env/.env.local
  --integrity-check        Also run PRAGMA integrity_check (can be slow)
  -h, --help               Show help
EOF
}

DOTENV=1
DB_ARG=""
RUN_INTEGRITY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-dotenv) DOTENV=0; shift ;;
    --integrity-check) RUN_INTEGRITY=1; shift ;;
    --db|--db-path|--db-url)
      if [[ $# -lt 2 || -z "${2:-}" ]]; then
        echo "Missing value for $1" >&2
        usage >&2
        exit 2
      fi
      DB_ARG="$2"
      shift 2
      ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
  RED=$'\033[0;31m'
  GREEN=$'\033[0;32m'
  YELLOW=$'\033[1;33m'
  BLUE=$'\033[0;34m'
  NC=$'\033[0m'
else
  RED=""
  GREEN=""
  YELLOW=""
  BLUE=""
  NC=""
fi

CHECKS=0
PASSES=0
WARNS=0
FAILS=0

check() { printf "%b\n" "${BLUE}[CHECK]${NC} $*"; CHECKS=$((CHECKS + 1)); }
info() { printf "%b\n" "${BLUE}[INFO]${NC}  $*"; }
pass() { printf "%b\n" "${GREEN}[PASS]${NC}  $*"; PASSES=$((PASSES + 1)); }
warn() { printf "%b\n" "${YELLOW}[WARN]${NC}  $*"; WARNS=$((WARNS + 1)); }
fail() { printf "%b\n" "${RED}[FAIL]${NC}  $*"; FAILS=$((FAILS + 1)); }

have_cmd() { command -v "$1" >/dev/null 2>&1; }

resolve_sqlite_path() {
  local url="$1"
  local path="$url"

  if [[ "$path" == sqlite://* ]]; then
    path="${path#sqlite://}"
  elif [[ "$path" == sqlite:* ]]; then
    path="${path#sqlite:}"
  else
    return 1
  fi

  path="${path%%\?*}"
  path="${path%%#*}"

  if [[ "$path" == ":memory:" ]]; then
    printf "%s" "$path"
    return 0
  fi

  if [[ "$path" != /* ]]; then
    path="$ROOT_DIR/$path"
  fi
  printf "%s" "$path"
  return 0
}

normalize_path() {
  local path="$1"
  if [[ "$path" == ":memory:" ]]; then
    printf "%s" "$path"
    return 0
  fi
  if [[ "$path" != /* ]]; then
    path="$ROOT_DIR/$path"
  fi
  printf "%s" "$path"
  return 0
}

echo "adapterOS DB quick check"
echo "Repo: $ROOT_DIR"
echo ""

if (( DOTENV )); then
  check "Load .env/.env.local (if present)"
  if [[ -f "$ROOT_DIR/.env" ]]; then
    set -a
    # shellcheck disable=SC1091
    source "$ROOT_DIR/.env"
    if [[ -f "$ROOT_DIR/.env.local" ]]; then
      # shellcheck disable=SC1091
      source "$ROOT_DIR/.env.local"
    fi
    set +a
    pass "Loaded .env/.env.local"
  else
    warn "No .env found (using current environment + defaults)"
  fi
else
  info "Skipping .env loading (--no-dotenv)"
fi

DB_SPEC=""
if [[ -n "$DB_ARG" ]]; then
  DB_SPEC="$DB_ARG"
elif [[ -n "${AOS_DATABASE_URL:-}" ]]; then
  DB_SPEC="$AOS_DATABASE_URL"
elif [[ -n "${DATABASE_URL:-}" ]]; then
  DB_SPEC="$DATABASE_URL"
elif [[ -n "${DB_PATH:-}" ]]; then
  DB_SPEC="$DB_PATH"
else
  DB_SPEC="./var/aos-cp.sqlite3"
fi

DB_SCHEME="path"
case "$DB_SPEC" in
  sqlite:* ) DB_SCHEME="sqlite" ;;
  postgres:*|postgresql:* ) DB_SCHEME="postgres" ;;
esac

DB_PATH=""
case "$DB_SCHEME" in
  sqlite)
    DB_PATH="$(resolve_sqlite_path "$DB_SPEC" 2>/dev/null || true)"
    ;;
  path)
    DB_PATH="$(normalize_path "$DB_SPEC" 2>/dev/null || true)"
    ;;
  postgres)
    DB_PATH=""
    ;;
esac

echo ""
echo "Database"
echo "--------"
check "DB spec"
pass "DB_SPEC=$DB_SPEC"

if [[ "$DB_SCHEME" != "sqlite" && "$DB_SCHEME" != "path" ]]; then
  fail "Unsupported DB scheme for this script: $DB_SCHEME (sqlite only)"
fi

if [[ -n "$DB_PATH" ]]; then
  check "Resolved DB path"
  pass "DB_PATH=$DB_PATH"
fi

DB_OK=1
check "sqlite3 available"
if have_cmd sqlite3; then
  pass "sqlite3: $(command -v sqlite3)"
else
  fail "Missing 'sqlite3' (required for sqlite DB checks)"
  DB_OK=0
fi

if [[ "$DB_SCHEME" == "postgres" ]]; then
  DB_OK=0
fi

if (( DB_OK )); then
  if [[ "$DB_PATH" == ":memory:" ]]; then
    fail "Refusing to check :memory: database (provide a file path)"
    DB_OK=0
  fi
fi

if (( DB_OK )); then
  check "DB file exists"
  if [[ -f "$DB_PATH" ]]; then
    pass "DB file present"
  else
    fail "DB file not found: $DB_PATH"
    DB_OK=0
  fi
fi

sqlite_cmd() {
  local sql="$1"
  sqlite3 -batch -noheader \
    -cmd ".timeout 5000" \
    -cmd "PRAGMA foreign_keys=ON;" \
    "$DB_PATH" \
    "$sql"
}

if (( DB_OK )); then
  check "Can query database"
  if sqlite_cmd "SELECT 1;" >/dev/null 2>&1; then
    pass "sqlite query ok"
  else
    fail "Failed to query DB (is it locked/corrupt?): $DB_PATH"
    DB_OK=0
  fi
fi

if (( DB_OK )); then
  check "Foreign keys enabled for this connection"
  fk_enabled="$(sqlite_cmd "PRAGMA foreign_keys;" 2>/dev/null | tr -d '\r' | tail -n 1 || true)"
  if [[ "$fk_enabled" == "1" ]]; then
    pass "PRAGMA foreign_keys=1"
  else
    fail "PRAGMA foreign_keys is not enabled (got: '${fk_enabled:-<empty>}')"
  fi
fi

if (( DB_OK )); then
  echo ""
  echo "Tables"
  echo "------"

  REQUIRED_TABLES=(
    "_sqlx_migrations"
    "tenants"
    "users"
    "auth_sessions"
    "tenant_policy_bindings"
    "system_metrics_config"
    "models"
    "adapters"
    "adapter_stacks"
    "workers"
    "chat_sessions"
    "chat_messages"
    "inference_traces"
    "inference_trace_tokens"
    "inference_trace_receipts"
    "policy_audit_decisions"
    "prefix_templates"
  )

  for table in "${REQUIRED_TABLES[@]}"; do
    check "Table exists: $table"
    exists="$(sqlite_cmd "SELECT 1 FROM sqlite_master WHERE type='table' AND name='$table' LIMIT 1;" 2>/dev/null | tr -d '\r' | head -n 1 || true)"
    if [[ "$exists" == "1" ]]; then
      pass "$table"
    else
      fail "Missing table: $table"
    fi
  done
fi

if (( DB_OK )); then
  echo ""
  echo "Seed rows"
  echo "---------"

  check "System tenant exists (tenants.id='system')"
  sys_tenant="$(sqlite_cmd "SELECT 1 FROM tenants WHERE id='system' LIMIT 1;" 2>/dev/null | tr -d '\r' | head -n 1 || true)"
  SYSTEM_TENANT_PRESENT=0
  if [[ "$sys_tenant" == "1" ]]; then
    pass "System tenant present"
    SYSTEM_TENANT_PRESENT=1
  else
    fail "System tenant missing (try: cargo run -p adapteros-cli -- db seed-fixtures --skip-reset; or start the server once)"
  fi

  SYSTEM_METRICS_KEYS=(
    "collection_interval_secs"
    "sampling_rate"
    "enable_gpu_metrics"
    "enable_disk_metrics"
    "enable_network_metrics"
    "retention_days"
    "cpu_warning_threshold"
    "cpu_critical_threshold"
    "memory_warning_threshold"
    "memory_critical_threshold"
    "disk_warning_threshold"
    "disk_critical_threshold"
    "gpu_warning_threshold"
    "gpu_critical_threshold"
    "min_memory_headroom"
  )

  for k in "${SYSTEM_METRICS_KEYS[@]}"; do
    check "system_metrics_config has key: $k"
    cnt="$(sqlite_cmd "SELECT COUNT(*) FROM system_metrics_config WHERE config_key='$k';" 2>/dev/null | tr -d '\r' | head -n 1 || true)"
    if [[ "$cnt" == "1" ]]; then
      pass "$k"
    else
      fail "Missing system_metrics_config key '$k' (count=$cnt)"
    fi
  done

  CORE_POLICIES=("egress" "determinism" "isolation" "evidence")
  for p in "${CORE_POLICIES[@]}"; do
    check "tenant_policy_bindings core policy enabled: system/$p"
    if (( ! SYSTEM_TENANT_PRESENT )); then
      warn "Skipping system/$p (system tenant missing)"
      continue
    fi
    enabled="$(sqlite_cmd "SELECT enabled FROM tenant_policy_bindings WHERE tenant_id='system' AND policy_pack_id='$p' AND scope='global' LIMIT 1;" 2>/dev/null | tr -d '\r' | head -n 1 || true)"
    if [[ "$enabled" == "1" ]]; then
      pass "$p enabled"
    elif [[ -z "$enabled" ]]; then
      fail "Missing tenant_policy_bindings row for system/$p (initialize_tenant_policy_bindings)"
    else
      fail "Policy binding system/$p not enabled (enabled=$enabled)"
    fi
  done

  check "Seed base model exists: Qwen2.5-7B-Instruct-4bit"
  qwen_cnt="$(sqlite_cmd "SELECT COUNT(*) FROM models WHERE id='Qwen2.5-7B-Instruct-4bit';" 2>/dev/null | tr -d '\r' | head -n 1 || true)"
  if [[ "$qwen_cnt" == "1" ]]; then
    pass "Qwen2.5-7B-Instruct-4bit present"
  else
    fail "Missing seeded model 'Qwen2.5-7B-Instruct-4bit' (run migrations; see migrations/0171_seed_base_model_qwen25.sql)"
  fi
fi

if (( DB_OK )); then
  echo ""
  echo "Foreign keys"
  echo "------------"

  check "PRAGMA foreign_key_check (violations should be empty)"
  if fk_out="$(sqlite_cmd "PRAGMA foreign_key_check;" 2>/dev/null | tr -d '\r')"; then
    if [[ -z "$fk_out" ]]; then
      pass "No FK violations"
    else
      fail "Foreign key violations detected:"
      printf "%s\n" "$fk_out" | sed 's/^/  /' | head -n 200
      if [[ "$(printf "%s\n" "$fk_out" | wc -l | tr -d ' ')" -gt 200 ]]; then
        warn "FK violation output truncated (showing first 200 lines)"
      fi
    fi
  else
    warn "Failed to run PRAGMA foreign_key_check (best-effort check skipped)"
  fi

  if (( RUN_INTEGRITY )); then
    check "PRAGMA integrity_check"
    integrity="$(sqlite_cmd "PRAGMA integrity_check;" 2>/dev/null | tr -d '\r' | head -n 1 || true)"
    if [[ "$integrity" == "ok" ]]; then
      pass "integrity_check=ok"
    else
      fail "integrity_check failed: ${integrity:-<no output>}"
    fi
  else
    info "Skipping PRAGMA integrity_check (pass --integrity-check to run)"
  fi
fi

echo ""
echo "Summary"
echo "-------"
echo "Checks: $CHECKS  Pass: $PASSES  Warn: $WARNS  Fail: $FAILS"

if (( FAILS > 0 )); then
  exit 1
fi
exit 0
