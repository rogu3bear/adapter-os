#!/usr/bin/env bash
# adapterOS config quick checker
#
# Checks:
# - Env vars used by dev-up/smoke scripts (ports, DB path)
# - Core commands exist (cargo, sqlite3/psql) and optional tooling checks (node, pnpm)
# - Required ports are free (or shows what is using them)
# - Prints PASS/FAIL summary

set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR" || exit 1
source "$ROOT_DIR/scripts/lib/ports.sh"

usage() {
  cat <<'EOF'
Usage: bash scripts/check-config.sh [--no-dotenv] [--allow-in-use]

Options:
  --no-dotenv       Do not source .env/.env.local
  --allow-in-use    Treat occupied ports as WARN (default: FAIL)
  -h, --help        Show help

Exit codes:
  0  PASS (no failures)
  1  FAIL (one or more failures)
  2  Usage error
EOF
}

DOTENV=1
ALLOW_IN_USE=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-dotenv) DOTENV=0; shift ;;
    --allow-in-use) ALLOW_IN_USE=1; shift ;;
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

is_uint() { [[ "${1:-}" =~ ^[0-9]+$ ]]; }

validate_port() {
  local label="$1"
  local value="${2:-}"
  if [[ -z "$value" ]]; then
    fail "$label is empty"
    return 1
  fi
  if ! is_uint "$value"; then
    fail "$label must be an integer (got: '$value')"
    return 1
  fi
  if (( value < 1 || value > 65535 )); then
    fail "$label must be 1-65535 (got: $value)"
    return 1
  fi
  return 0
}

extract_url_port() {
  local url="${1:-}"
  local rest hostport after port=""

  [[ -z "$url" ]] && return 1

  rest="$url"
  if [[ "$rest" == *"://"* ]]; then
    rest="${rest#*://}"
  fi
  hostport="${rest%%/*}"

  if [[ "$hostport" == \[*\]* ]]; then
    after="${hostport#*\]}"
    if [[ "$after" == :* ]]; then
      port="${after#:}"
    fi
  elif [[ "$hostport" == *:* ]]; then
    port="${hostport##*:}"
  fi

  if [[ -z "$port" ]]; then
    return 1
  fi
  printf "%s" "$port"
  return 0
}

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

  if [[ "$path" != /* && "$path" != ":memory:" ]]; then
    path="$ROOT_DIR/$path"
  fi
  printf "%s" "$path"
  return 0
}

nearest_existing_parent() {
  local p="$1"
  while [[ -n "$p" && "$p" != "/" && ! -d "$p" ]]; do
    p="$(dirname "$p")"
  done
  printf "%s" "$p"
}

port_tool() {
  if have_cmd lsof; then
    printf "lsof"
    return 0
  fi
  if have_cmd ss; then
    printf "ss"
    return 0
  fi
  if have_cmd netstat; then
    printf "netstat"
    return 0
  fi
  return 1
}

port_in_use() {
  local port="$1"
  if have_cmd lsof; then
    lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1
    case $? in
      0) return 0 ;;
      1) return 1 ;;
      *) return 2 ;;
    esac
  fi
  if have_cmd ss; then
    ss -ltn 2>/dev/null | grep -qE "[:.]${port}[[:space:]]"
    return $?
  fi
  if have_cmd netstat; then
    netstat -an 2>/dev/null | grep -qE "[:.]${port}[[:space:]].*LISTEN"
    return $?
  fi
  return 2
}

describe_port_users() {
  local port="$1"

  if have_cmd lsof; then
    info "Listeners (lsof):"
    lsof -nP -iTCP:"$port" -sTCP:LISTEN 2>/dev/null | sed 's/^/  /' || true

    local pids
    pids="$(lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null | tr '\n' ' ' || true)"
    if [[ -n "$pids" ]]; then
      info "Process details (ps):"
      local pid
      for pid in $pids; do
        if ps -p "$pid" -o pid= -o user= -o command= >/dev/null 2>&1; then
          ps -p "$pid" -o pid= -o user= -o command= 2>/dev/null | sed 's/^/  /' || true
        else
          ps -p "$pid" -o pid= -o user= -o args= 2>/dev/null | sed 's/^/  /' || true
        fi
      done
    fi
    return 0
  fi

  if have_cmd ss; then
    info "Listeners (ss):"
    ss -ltnp 2>/dev/null | grep -E "[:.]${port}[[:space:]]" | sed 's/^/  /' || true
    return 0
  fi

  if have_cmd netstat; then
    info "Listeners (netstat):"
    netstat -an 2>/dev/null | grep -E "[:.]${port}[[:space:]].*LISTEN" | sed 's/^/  /' || true
    return 0
  fi

  return 0
}

echo "adapterOS config quick check"
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

aos_apply_port_pane_defaults

echo ""
echo "Env vars (ports, DB)"
echo "--------------------"

check "AOS_SERVER_PORT / API_PORT / API_URL (backend port)"
if [[ -n "${AOS_SERVER_PORT:-}" ]]; then
  if validate_port "AOS_SERVER_PORT" "$AOS_SERVER_PORT"; then
    pass "AOS_SERVER_PORT=$AOS_SERVER_PORT"
  fi
else
  warn "AOS_SERVER_PORT not set (default: 18080)"
fi
if [[ -n "${API_PORT:-}" ]]; then
  if validate_port "API_PORT" "$API_PORT"; then
    pass "API_PORT=$API_PORT"
  fi
  if [[ -n "${AOS_SERVER_PORT:-}" && "$API_PORT" != "$AOS_SERVER_PORT" ]]; then
    warn "API_PORT ($API_PORT) differs from AOS_SERVER_PORT ($AOS_SERVER_PORT)"
  fi
fi
if [[ -n "${API_URL:-}" ]]; then
  api_url_port="$(extract_url_port "$API_URL" 2>/dev/null || true)"
  if [[ -n "$api_url_port" ]]; then
    if validate_port "API_URL port" "$api_url_port"; then
      pass "API_URL=$API_URL (port $api_url_port)"
    fi
  else
    warn "API_URL set but no explicit port detected: $API_URL"
  fi
fi

if [[ -n "${ADAPTEROS_BASE_URL:-}" ]]; then
  check "ADAPTEROS_BASE_URL (used by scripts/ui_smoke.sh)"
  base_url_port="$(extract_url_port "$ADAPTEROS_BASE_URL" 2>/dev/null || true)"
  if [[ -n "$base_url_port" ]]; then
    if validate_port "ADAPTEROS_BASE_URL port" "$base_url_port"; then
      pass "ADAPTEROS_BASE_URL=$ADAPTEROS_BASE_URL (port $base_url_port)"
    fi
  else
    warn "ADAPTEROS_BASE_URL set but no explicit port detected: $ADAPTEROS_BASE_URL"
  fi
fi

check "AOS_UI_PORT / UI_PORT (UI port)"
if [[ -n "${AOS_UI_PORT:-}" ]]; then
  if validate_port "AOS_UI_PORT" "$AOS_UI_PORT"; then
    pass "AOS_UI_PORT=$AOS_UI_PORT"
  fi
else
  warn "AOS_UI_PORT not set (default: 18081)"
fi
if [[ -n "${UI_PORT:-}" ]]; then
  if validate_port "UI_PORT" "$UI_PORT"; then
    pass "UI_PORT=$UI_PORT"
  fi
  if [[ -n "${AOS_UI_PORT:-}" && "$UI_PORT" != "$AOS_UI_PORT" ]]; then
    warn "UI_PORT ($UI_PORT) differs from AOS_UI_PORT ($AOS_UI_PORT)"
  fi
fi

check "AOS_DATABASE_URL (DB URL/path)"
DB_URL="${AOS_DATABASE_URL:-}"
if [[ -n "$DB_URL" ]]; then
  pass "AOS_DATABASE_URL=$DB_URL"
elif [[ -n "${DATABASE_URL:-}" ]]; then
  DB_URL="$DATABASE_URL"
  warn "AOS_DATABASE_URL is not set; falling back to DATABASE_URL=$DATABASE_URL"
else
  fail "AOS_DATABASE_URL is not set (copy .env.example -> .env)"
  DB_URL=""
fi

check "DB_PATH (used by e2e/smoke scripts)"
if [[ -n "${DB_PATH:-}" ]]; then
  pass "DB_PATH=$DB_PATH"
else
  warn "DB_PATH not set (e2e/smoke scripts default to $ROOT_DIR/var/*.sqlite3)"
fi

echo ""
echo "Commands"
echo "--------"

check "cargo"
if have_cmd cargo; then
  pass "cargo: $(command -v cargo)"
else
  fail "Missing 'cargo' (install Rust toolchain via rustup)"
fi

check "node"
if have_cmd node; then
  pass "node: $(command -v node)"
else
  warn "Missing 'node' (optional for runtime; needed for UI/tooling such as Playwright)"
fi

check "pnpm"
if have_cmd pnpm; then
  pass "pnpm: $(command -v pnpm)"
else
  warn "Missing 'pnpm' (optional for runtime; needed for some UI/tooling workflows)"
fi

check "sqlite3/psql (based on AOS_DATABASE_URL)"
DB_SCHEME=""
if [[ -n "$DB_URL" ]]; then
  case "$DB_URL" in
    sqlite:* ) DB_SCHEME="sqlite" ;;
    postgres:*|postgresql:* ) DB_SCHEME="postgres" ;;
    * ) DB_SCHEME="unknown" ;;
  esac
fi

case "$DB_SCHEME" in
  sqlite)
    if have_cmd sqlite3; then
      pass "sqlite3: $(command -v sqlite3)"
    else
      fail "Missing 'sqlite3' (required by scripts/service-manager.sh for dev-up)"
    fi
    ;;
  postgres)
    if have_cmd psql; then
      pass "psql: $(command -v psql)"
    else
      fail "Missing 'psql' (Postgres database URL detected)"
    fi
    ;;
  "")
    warn "Skipping DB client check (AOS_DATABASE_URL missing)"
    ;;
  *)
    fail "Unsupported AOS_DATABASE_URL scheme: $DB_URL"
    ;;
esac

check "port inspection tool (lsof/ss/netstat)"
PORT_TOOL="$(port_tool 2>/dev/null || true)"
if [[ -n "$PORT_TOOL" ]]; then
  pass "Using '$PORT_TOOL' to inspect ports"
else
  fail "Missing port inspection tool (need one of: lsof, ss, netstat)"
fi

echo ""
echo "Ports"
echo "-----"

EFFECTIVE_BACKEND_PORT=""
if [[ -n "${AOS_SERVER_PORT:-}" ]]; then
  EFFECTIVE_BACKEND_PORT="$AOS_SERVER_PORT"
elif [[ -n "${API_PORT:-}" ]]; then
  EFFECTIVE_BACKEND_PORT="$API_PORT"
elif [[ -n "${API_URL:-}" ]]; then
  EFFECTIVE_BACKEND_PORT="$(extract_url_port "$API_URL" 2>/dev/null || true)"
elif [[ -n "${ADAPTEROS_BASE_URL:-}" ]]; then
  EFFECTIVE_BACKEND_PORT="$(extract_url_port "$ADAPTEROS_BASE_URL" 2>/dev/null || true)"
fi
EFFECTIVE_BACKEND_PORT="${EFFECTIVE_BACKEND_PORT:-18080}"

EFFECTIVE_UI_PORT=""
if [[ -n "${AOS_UI_PORT:-}" ]]; then
  EFFECTIVE_UI_PORT="$AOS_UI_PORT"
elif [[ -n "${UI_PORT:-}" ]]; then
  EFFECTIVE_UI_PORT="$UI_PORT"
fi
EFFECTIVE_UI_PORT="${EFFECTIVE_UI_PORT:-18081}"

check "Effective port values"
if validate_port "backend port" "$EFFECTIVE_BACKEND_PORT" && validate_port "ui port" "$EFFECTIVE_UI_PORT"; then
  if [[ "$EFFECTIVE_BACKEND_PORT" == "$EFFECTIVE_UI_PORT" ]]; then
    fail "Backend/UI ports collide: $EFFECTIVE_BACKEND_PORT"
  else
    pass "backend=$EFFECTIVE_BACKEND_PORT ui=$EFFECTIVE_UI_PORT"
  fi
fi

if [[ -n "${API_URL:-}" ]]; then
  api_url_port="$(extract_url_port "$API_URL" 2>/dev/null || true)"
  if [[ -n "$api_url_port" && "$api_url_port" != "$EFFECTIVE_BACKEND_PORT" ]]; then
    warn "API_URL port ($api_url_port) differs from effective backend port ($EFFECTIVE_BACKEND_PORT)"
  fi
fi
if [[ -n "${ADAPTEROS_BASE_URL:-}" ]]; then
  base_url_port="$(extract_url_port "$ADAPTEROS_BASE_URL" 2>/dev/null || true)"
  if [[ -n "$base_url_port" && "$base_url_port" != "$EFFECTIVE_BACKEND_PORT" ]]; then
    warn "ADAPTEROS_BASE_URL port ($base_url_port) differs from effective backend port ($EFFECTIVE_BACKEND_PORT)"
  fi
fi

check "Backend port $EFFECTIVE_BACKEND_PORT is free"
if port_in_use "$EFFECTIVE_BACKEND_PORT"; then
  if (( ALLOW_IN_USE )); then
    warn "Port $EFFECTIVE_BACKEND_PORT is in use"
  else
    fail "Port $EFFECTIVE_BACKEND_PORT is in use"
  fi
  describe_port_users "$EFFECTIVE_BACKEND_PORT"
else
  rc=$?
  if (( rc == 1 )); then
    pass "Port $EFFECTIVE_BACKEND_PORT is free"
  else
    fail "Unable to determine status for port $EFFECTIVE_BACKEND_PORT"
  fi
fi

check "UI port $EFFECTIVE_UI_PORT is free"
if port_in_use "$EFFECTIVE_UI_PORT"; then
  if (( ALLOW_IN_USE )); then
    warn "Port $EFFECTIVE_UI_PORT is in use"
  else
    fail "Port $EFFECTIVE_UI_PORT is in use"
  fi
  describe_port_users "$EFFECTIVE_UI_PORT"
else
  rc=$?
  if (( rc == 1 )); then
    pass "Port $EFFECTIVE_UI_PORT is free"
  else
    fail "Unable to determine status for port $EFFECTIVE_UI_PORT"
  fi
fi

echo ""
echo "Database path"
echo "-------------"

if [[ "$DB_SCHEME" == "sqlite" && -n "$DB_URL" ]]; then
  check "Resolve SQLite DB path"
  SQLITE_PATH="$(resolve_sqlite_path "$DB_URL" 2>/dev/null || true)"
  if [[ -z "$SQLITE_PATH" ]]; then
    fail "Failed to parse SQLite path from AOS_DATABASE_URL=$DB_URL"
  else
    pass "SQLite path: $SQLITE_PATH"

    if [[ "$SQLITE_PATH" == ":memory:" ]]; then
      warn "SQLite is in-memory (:memory:); DB file checks skipped"
    else
      check "SQLite directory exists / is writable"
      SQLITE_DIR="$(dirname "$SQLITE_PATH")"
      if [[ -d "$SQLITE_DIR" ]]; then
        if [[ -w "$SQLITE_DIR" && -x "$SQLITE_DIR" ]]; then
          pass "Directory writable: $SQLITE_DIR"
        else
          fail "Directory not writable: $SQLITE_DIR"
        fi
      else
        EXISTING_PARENT="$(nearest_existing_parent "$SQLITE_DIR")"
        if [[ -n "$EXISTING_PARENT" && -w "$EXISTING_PARENT" && -x "$EXISTING_PARENT" ]]; then
          warn "Directory does not exist yet ($SQLITE_DIR); parent is writable ($EXISTING_PARENT)"
        else
          fail "Directory does not exist and parent is not writable: $SQLITE_DIR"
        fi
      fi

      check "SQLite file sanity"
      if [[ -e "$SQLITE_PATH" && ! -f "$SQLITE_PATH" ]]; then
        fail "DB path exists but is not a file: $SQLITE_PATH"
      elif [[ -f "$SQLITE_PATH" ]]; then
        if [[ -r "$SQLITE_PATH" ]]; then
          pass "DB file readable: $SQLITE_PATH"
        else
          fail "DB file not readable: $SQLITE_PATH"
        fi
      else
        warn "DB file not present yet (will be created by migrations): $SQLITE_PATH"
      fi
    fi
  fi
fi

if [[ -n "${DB_PATH:-}" ]]; then
  check "DB_PATH directory exists / is writable"
  DB_PATH_ABS="$DB_PATH"
  if [[ "$DB_PATH_ABS" != /* ]]; then
    DB_PATH_ABS="$ROOT_DIR/$DB_PATH_ABS"
  fi
  DB_PATH_DIR="$(dirname "$DB_PATH_ABS")"
  if [[ -d "$DB_PATH_DIR" ]]; then
    if [[ -w "$DB_PATH_DIR" && -x "$DB_PATH_DIR" ]]; then
      pass "Directory writable: $DB_PATH_DIR"
    else
      fail "Directory not writable: $DB_PATH_DIR"
    fi
  else
    EXISTING_PARENT="$(nearest_existing_parent "$DB_PATH_DIR")"
    if [[ -n "$EXISTING_PARENT" && -w "$EXISTING_PARENT" && -x "$EXISTING_PARENT" ]]; then
      warn "Directory does not exist yet ($DB_PATH_DIR); parent is writable ($EXISTING_PARENT)"
    else
      fail "Directory does not exist and parent is not writable: $DB_PATH_DIR"
    fi
  fi
fi

echo ""
echo "Summary"
echo "-------"
echo "Checks:   $CHECKS"
echo "Passed:   $PASSES"
echo "Warnings: $WARNS"
echo "Failed:   $FAILS"

if (( FAILS > 0 )); then
  echo ""
  printf "%b\n" "${RED}FAIL${NC} - fix failures above"
  exit 1
fi

echo ""
printf "%b\n" "${GREEN}PASS${NC}"
exit 0
