#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🚀 AdapterOS Server Startup Script${NC}"
echo "======================================"

# Check if .env exists, create from template if not
if [ ! -f .env ]; then
    if [ -f .env.example ]; then
        echo -e "${YELLOW}⚠️  No .env file found. Creating from template...${NC}"
        cp .env.example .env
        echo -e "${GREEN}✅ Created .env from .env.example${NC}"
        echo -e "${YELLOW}📝 Please review and update .env with your settings${NC}"
    else
        echo -e "${RED}❌ Error: No .env or .env.example found!${NC}"
        echo "Please create a .env file with required configuration."
        exit 1
    fi
fi

# Load environment variables
echo -e "${BLUE}📋 Loading environment variables...${NC}"
export $(cat .env | grep -v '^#' | xargs)

# Extract database path from DATABASE_URL
if [[ "$DATABASE_URL" =~ ^sqlite://(.*)$ ]]; then
    DB_FILE="${BASH_REMATCH[1]}"
else
    DB_FILE="var/aos-cp.sqlite3"
fi

# Check if database file exists
if [ ! -f "$DB_FILE" ]; then
    echo -e "${YELLOW}📦 Database not found at $DB_FILE${NC}"
    echo -e "${BLUE}📦 Initializing database...${NC}"

    # Create directory if needed
    DB_DIR=$(dirname "$DB_FILE")
    mkdir -p "$DB_DIR"

    # Touch the database file so SQLite can connect
    touch "$DB_FILE"
    echo -e "${GREEN}✅ Created database file at $DB_FILE${NC}"
fi

# Check if migrations need to be run
echo -e "${BLUE}🔄 Checking database migrations...${NC}"
if command -v sqlite3 &> /dev/null; then
    # Check if schema_version table exists
    TABLE_EXISTS=$(sqlite3 "$DB_FILE" "SELECT name FROM sqlite_master WHERE type='table' AND name='schema_version';" 2>/dev/null || echo "")

    if [ -z "$TABLE_EXISTS" ]; then
        echo -e "${YELLOW}⚠️  Database schema not initialized. Migrations will run on server start.${NC}"
    else
        VERSION=$(sqlite3 "$DB_FILE" "SELECT MAX(version) FROM schema_version;" 2>/dev/null || echo "0")
        echo -e "${GREEN}✅ Database at version: $VERSION${NC}"
    fi
else
    echo -e "${YELLOW}⚠️  sqlite3 not found. Cannot check migration status.${NC}"
fi

# Check if the server binary exists
SERVER_BIN="target/release/adapteros-server"
if [ ! -f "$SERVER_BIN" ]; then
    echo -e "${YELLOW}⚠️  Server binary not found. Building...${NC}"
    echo -e "${BLUE}🔨 Running: cargo build --release -p adapteros-server${NC}"

    # Try to build the server
    if cargo build --release -p adapteros-server 2>&1 | tee build.log; then
        echo -e "${GREEN}✅ Build successful${NC}"
    else
        echo -e "${RED}❌ Build failed. Check build.log for details.${NC}"
        echo -e "${YELLOW}📝 Note: Some components may have compilation errors.${NC}"
        echo -e "${YELLOW}   The core server should still work for basic operations.${NC}"

        # Check if we can at least run in dev mode
        echo -e "${BLUE}🔍 Attempting development build...${NC}"
        if cargo build -p adapteros-server 2>&1 | tee build-dev.log; then
            SERVER_BIN="target/debug/adapteros-server"
            echo -e "${GREEN}✅ Development build successful${NC}"
        else
            echo -e "${RED}❌ Could not build server. Please fix compilation errors first.${NC}"
            exit 1
        fi
    fi
fi

# Display startup information
echo ""
echo -e "${BLUE}📊 Server Configuration:${NC}"
echo "   Database:    $DB_FILE"
echo "   Host:        ${AOS_SERVER_HOST:-127.0.0.1}"
echo "   Port:        ${AOS_SERVER_PORT:-8080}"
echo "   Environment: ${AOS_ENV:-development}"
echo "   Log Level:   ${RUST_LOG:-info}"
echo ""

# Start the server
echo -e "${GREEN}🚀 Starting AdapterOS server...${NC}"
echo "======================================"
echo -e "${BLUE}📡 Endpoints:${NC}"
echo "   Health:  http://${AOS_SERVER_HOST:-127.0.0.1}:${AOS_SERVER_PORT:-8080}/healthz"
echo "   Ready:   http://${AOS_SERVER_HOST:-127.0.0.1}:${AOS_SERVER_PORT:-8080}/ready"
echo "   API:     http://${AOS_SERVER_HOST:-127.0.0.1}:${AOS_SERVER_PORT:-8080}/v1"
echo "   Swagger: http://${AOS_SERVER_HOST:-127.0.0.1}:${AOS_SERVER_PORT:-8080}/api/docs"
echo ""
echo -e "${YELLOW}💡 Press Ctrl+C to stop the server${NC}"
echo "======================================"
echo ""

# Run the server
if [ -f "$SERVER_BIN" ]; then
    exec "$SERVER_BIN" --config "${AOS_CONFIG_PATH:-configs/cp.toml}"
else
    echo -e "${RED}❌ Server binary not found at $SERVER_BIN${NC}"
    echo "Please build the server first with: cargo build --release -p adapteros-server"
    exit 1
fi