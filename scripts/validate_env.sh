#!/bin/bash
# AdapterOS Environment Validation Script
# Validates .env configuration and environment setup

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Track validation status
ERRORS=0
WARNINGS=0
CHECKS=0

# Helper functions
log_check() {
    echo -e "${BLUE}[CHECK]${NC} $1"
    ((CHECKS++))
}

log_pass() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
    ((WARNINGS++))
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((ERRORS++))
}

# Check if .env exists
if [ ! -f .env ]; then
    log_check ".env file exists"
    log_fail ".env file not found. Run: cp .env.example .env"
    exit 1
fi

log_check ".env file exists"
log_pass ".env file found"

# Load .env (safely)
set -a
source .env
set +a

# ═══════════════════════════════════════════════════════════════════════════════
# MODEL CONFIGURATION CHECKS
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "${BLUE}=== MODEL CONFIGURATION ===${NC}"

log_check "AOS_MODEL_PATH is set"
if [ -z "$AOS_MODEL_PATH" ]; then
    log_fail "AOS_MODEL_PATH is not set"
else
    log_pass "AOS_MODEL_PATH=$AOS_MODEL_PATH"
    if [ ! -d "$AOS_MODEL_PATH" ]; then
        log_warn "Model directory not found: $AOS_MODEL_PATH"
        log_warn "Run: ./scripts/download_model.sh"
    else
        if [ -f "$AOS_MODEL_PATH/config.json" ]; then
            log_pass "Model config.json found"
        else
            log_fail "Model config.json missing"
        fi
    fi
fi

log_check "AOS_MODEL_BACKEND is set"
if [ -z "$AOS_MODEL_BACKEND" ]; then
    log_warn "AOS_MODEL_BACKEND not set (will use 'auto')"
else
    case "$AOS_MODEL_BACKEND" in
        auto|mlx|coreml|metal)
            log_pass "AOS_MODEL_BACKEND=$AOS_MODEL_BACKEND"
            ;;
        *)
            log_fail "Invalid AOS_MODEL_BACKEND: $AOS_MODEL_BACKEND (valid: auto, mlx, coreml, metal)"
            ;;
    esac
fi

# ═══════════════════════════════════════════════════════════════════════════════
# SERVER CONFIGURATION CHECKS
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "${BLUE}=== SERVER CONFIGURATION ===${NC}"

log_check "AOS_SERVER_PORT is set"
if [ -z "$AOS_SERVER_PORT" ]; then
    log_warn "AOS_SERVER_PORT not set (will use 8080)"
    PORT=8080
else
    PORT=$AOS_SERVER_PORT
    log_pass "AOS_SERVER_PORT=$PORT"
fi

# Check if port is available
log_check "Port $PORT is available"
if lsof -Pi :$PORT -sTCP:LISTEN -t >/dev/null 2>&1; then
    log_warn "Port $PORT is already in use"
    PID=$(lsof -Pi :$PORT -sTCP:LISTEN -t 2>/dev/null | head -1)
    log_warn "Process using port: $(ps -p $PID -o comm=)"
else
    log_pass "Port $PORT is available"
fi

log_check "AOS_SERVER_PRODUCTION_MODE setting"
if [ -z "$AOS_SERVER_PRODUCTION_MODE" ] || [ "$AOS_SERVER_PRODUCTION_MODE" = "false" ]; then
    log_pass "Development mode (AOS_SERVER_PRODUCTION_MODE=false or unset)"
else
    log_pass "Production mode enabled (AOS_SERVER_PRODUCTION_MODE=true)"

    # Production mode requires additional checks
    log_check "Production mode security requirements"

    if [ -z "$AOS_SERVER_UDS_SOCKET" ]; then
        log_fail "Production mode requires AOS_SERVER_UDS_SOCKET to be set"
    else
        log_pass "AOS_SERVER_UDS_SOCKET is set"
    fi

    if [ -z "$AOS_SECURITY_JWT_MODE" ] || [ "$AOS_SECURITY_JWT_MODE" != "eddsa" ]; then
        log_fail "Production mode requires AOS_SECURITY_JWT_MODE=eddsa"
    else
        log_pass "AOS_SECURITY_JWT_MODE=eddsa"
    fi

    if [ "$AOS_SECURITY_PF_DENY" != "true" ]; then
        log_fail "Production mode requires AOS_SECURITY_PF_DENY=true"
    else
        log_pass "AOS_SECURITY_PF_DENY=true"
    fi
fi

# ═══════════════════════════════════════════════════════════════════════════════
# DATABASE CONFIGURATION CHECKS
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "${BLUE}=== DATABASE CONFIGURATION ===${NC}"

log_check "AOS_DATABASE_URL is set"
if [ -z "$AOS_DATABASE_URL" ]; then
    log_fail "AOS_DATABASE_URL is not set"
else
    log_pass "AOS_DATABASE_URL=$AOS_DATABASE_URL"

    # Extract database path from SQLite URL
    if [[ "$AOS_DATABASE_URL" == sqlite:* ]]; then
        DB_PATH="${AOS_DATABASE_URL#sqlite:}"
        DB_DIR=$(dirname "$DB_PATH")

        log_check "Database directory exists"
        if [ ! -d "$DB_DIR" ]; then
            log_warn "Database directory not found: $DB_DIR"
            log_warn "Will be created by migrations"
        else
            log_pass "Database directory found: $DB_DIR"
        fi
    fi
fi

# ═══════════════════════════════════════════════════════════════════════════════
# LOGGING CONFIGURATION CHECKS
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "${BLUE}=== LOGGING CONFIGURATION ===${NC}"

log_check "RUST_LOG is set"
if [ -z "$RUST_LOG" ]; then
    log_warn "RUST_LOG not set (will use default)"
else
    log_pass "RUST_LOG=$RUST_LOG"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# SECURITY CONFIGURATION CHECKS
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "${BLUE}=== SECURITY CONFIGURATION ===${NC}"

log_check "AOS_SECURITY_JWT_MODE is set"
if [ -z "$AOS_SECURITY_JWT_MODE" ]; then
    log_warn "AOS_SECURITY_JWT_MODE not set (will use default)"
elif [ "$AOS_SECURITY_JWT_MODE" = "eddsa" ]; then
    log_pass "AOS_SECURITY_JWT_MODE=eddsa (production-ready)"
elif [ "$AOS_SECURITY_JWT_MODE" = "hs256" ]; then
    log_warn "AOS_SECURITY_JWT_MODE=hs256 (development-only, not secure)"
else
    log_fail "Invalid AOS_SECURITY_JWT_MODE: $AOS_SECURITY_JWT_MODE"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# TOOL AVAILABILITY CHECKS
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "${BLUE}=== TOOL AVAILABILITY ===${NC}"

log_check "Rust compiler is available"
if command -v rustc &> /dev/null; then
    RUST_VERSION=$(rustc --version)
    log_pass "Rust: $RUST_VERSION"
else
    log_fail "Rust compiler not found. Install from: https://rustup.rs/"
fi

log_check "Cargo is available"
if command -v cargo &> /dev/null; then
    CARGO_VERSION=$(cargo --version)
    log_pass "$CARGO_VERSION"
else
    log_fail "Cargo not found"
fi

log_check "SQLite is available"
if command -v sqlite3 &> /dev/null; then
    SQLITE_VERSION=$(sqlite3 --version | head -1)
    log_pass "SQLite: $SQLITE_VERSION"
else
    log_warn "SQLite not found (optional, needed for manual DB inspection)"
fi

log_check "Node.js is available"
if command -v node &> /dev/null; then
    NODE_VERSION=$(node --version)
    log_pass "Node.js: $NODE_VERSION"
else
    log_warn "Node.js not found (needed for UI)"
fi

log_check "pnpm is available"
if command -v pnpm &> /dev/null; then
    PNPM_VERSION=$(pnpm --version)
    log_pass "pnpm: $PNPM_VERSION"
else
    log_warn "pnpm not found (needed for UI)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# DIRECTORY STRUCTURE CHECKS
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "${BLUE}=== DIRECTORY STRUCTURE ===${NC}"

REQUIRED_DIRS=("var" "crates" "ui" "docs" "scripts")

for dir in "${REQUIRED_DIRS[@]}"; do
    log_check "Directory $dir exists"
    if [ -d "$dir" ]; then
        log_pass "$dir found"
    else
        log_fail "$dir not found"
    fi
done

# ═══════════════════════════════════════════════════════════════════════════════
# SUMMARY
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo -e "${BLUE}═══════════════════════════════════════════════════════════════════════════════${NC}"
echo "VALIDATION SUMMARY"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════════════════════${NC}"
echo "Checks run: $CHECKS"
echo -e "${GREEN}Passed: $(($CHECKS - $WARNINGS - $ERRORS))${NC}"
echo -e "${YELLOW}Warnings: $WARNINGS${NC}"
echo -e "${RED}Errors: $ERRORS${NC}"

if [ $ERRORS -gt 0 ]; then
    echo ""
    echo -e "${RED}❌ Environment validation FAILED${NC}"
    echo "Fix the errors above and re-run this script"
    exit 1
elif [ $WARNINGS -gt 0 ]; then
    echo ""
    echo -e "${YELLOW}⚠️  Environment validation passed with warnings${NC}"
    echo "Some optional dependencies or configurations are missing"
    exit 0
else
    echo ""
    echo -e "${GREEN}✅ Environment validation passed${NC}"
    echo "Ready to start AdapterOS!"
    exit 0
fi
