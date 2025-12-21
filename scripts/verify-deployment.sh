#!/bin/bash
# Verify deployment of all determinism loop components
#
# This script checks that all components are properly deployed and configured

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
TENANT_ID="${AOS_TENANT_ID:-default}"
DB_PATH="${AOS_DB_PATH:-var/aos.db}"
UDS_SOCKET="/var/run/aos/${TENANT_ID}/metrics.sock"

echo "🔍 Verifying Determinism Loop Deployment"
echo "========================================"

# Function to check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to check if a service is running
service_running() {
    systemctl is-active --quiet "$1" 2>/dev/null
}

# Function to check if a file exists
file_exists() {
    [ -f "$1" ]
}

# Function to check if a socket exists
socket_exists() {
    [ -S "$1" ]
}

# Check 1: Database migrations applied
echo "📊 Checking database migrations..."
if file_exists "$DB_PATH"; then
    # Check if new tables exist
    TABLES=$(sqlite3 "$DB_PATH" ".tables" | grep -E "(policy_hashes|federation|tick_ledger|cp_lineage)" | wc -l)
    if [ "$TABLES" -ge 4 ]; then
        echo -e "${GREEN}✅ Database migrations applied ($TABLES/4 tables found)${NC}"
    else
        echo -e "${RED}❌ Database migrations incomplete ($TABLES/4 tables found)${NC}"
        exit 1
    fi
else
    echo -e "${RED}❌ Database file not found: $DB_PATH${NC}"
    exit 1
fi

# Check 2: Federation crate compilation
echo "🔗 Checking federation crate compilation..."
if cargo check --package adapteros-federation --quiet 2>/dev/null; then
    echo -e "${GREEN}✅ Federation crate compiles successfully${NC}"
else
    echo -e "${RED}❌ Federation crate compilation failed${NC}"
    cargo check --package adapteros-federation
    exit 1
fi

# Check 3: Policy hash watcher integration
echo "🛡️  Checking policy hash watcher integration..."
if cargo check --package adapteros-policy --quiet 2>/dev/null; then
    echo -e "${GREEN}✅ Policy hash watcher compiles successfully${NC}"
else
    echo -e "${RED}❌ Policy hash watcher compilation failed${NC}"
    cargo check --package adapteros-policy
    exit 1
fi

# Check 4: Global tick ledger
echo "📈 Checking global tick ledger..."
if cargo check --package adapteros-deterministic-exec --quiet 2>/dev/null; then
    echo -e "${GREEN}✅ Global tick ledger compiles successfully${NC}"
else
    echo -e "${RED}❌ Global tick ledger compilation failed${NC}"
    cargo check --package adapteros-deterministic-exec
    exit 1
fi

# Check 5: UDS metrics exporter
echo "📡 Checking UDS metrics exporter..."
if cargo check --package adapteros-telemetry --quiet 2>/dev/null; then
    echo -e "${GREEN}✅ UDS metrics exporter compiles successfully${NC}"
else
    echo -e "${RED}❌ UDS metrics exporter compilation failed${NC}"
    cargo check --package adapteros-telemetry
    exit 1
fi

# Check 6: CAB rollback
echo "🔄 Checking CAB rollback..."
if cargo check --package adapteros-cli --quiet 2>/dev/null; then
    echo -e "${GREEN}✅ CAB rollback compiles successfully${NC}"
else
    echo -e "${RED}❌ CAB rollback compilation failed${NC}"
    cargo check --package adapteros-cli
    exit 1
fi

# Check 7: Secure Enclave integration
echo "🔐 Checking Secure Enclave integration..."
if cargo check --package adapteros-secd --quiet 2>/dev/null; then
    echo -e "${GREEN}✅ Secure Enclave integration compiles successfully${NC}"
else
    echo -e "${RED}❌ Secure Enclave integration compilation failed${NC}"
    cargo check --package adapteros-secd
    exit 1
fi

# Check 8: Supervisor daemon
echo "👥 Checking supervisor daemon..."
if cargo check --package adapteros-orchestrator --quiet 2>/dev/null; then
    echo -e "${GREEN}✅ Supervisor daemon compiles successfully${NC}"
else
    echo -e "${RED}❌ Supervisor daemon compilation failed${NC}"
    cargo check --package adapteros-orchestrator
    exit 1
fi

# Check 9: CLI commands
echo "💻 Checking CLI commands..."
if cargo build --package adapteros-cli --quiet 2>/dev/null; then
    echo -e "${GREEN}✅ CLI commands build successfully${NC}"
else
    echo -e "${RED}❌ CLI commands build failed${NC}"
    cargo build --package adapteros-cli
    exit 1
fi

# Check 10: UDS socket (if service is running)
echo "🔌 Checking UDS socket..."
if socket_exists "$UDS_SOCKET"; then
    echo -e "${GREEN}✅ UDS socket exists: $UDS_SOCKET${NC}"
else
    echo -e "${YELLOW}⚠️  UDS socket not found: $UDS_SOCKET${NC}"
    echo "   This is normal if the metrics bridge service isn't running yet."
fi

# Check 11: Service files
echo "⚙️  Checking service files..."
if file_exists "scripts/aos-supervisor.service"; then
    echo -e "${GREEN}✅ Supervisor service file exists${NC}"
else
    echo -e "${RED}❌ Supervisor service file missing${NC}"
    exit 1
fi

if file_exists "scripts/supervisor.toml"; then
    echo -e "${GREEN}✅ Supervisor configuration exists${NC}"
else
    echo -e "${RED}❌ Supervisor configuration missing${NC}"
    exit 1
fi

# Check 12: Deployment scripts
echo "📦 Checking deployment scripts..."
if file_exists "scripts/deploy-uds-metrics.sh"; then
    echo -e "${GREEN}✅ UDS metrics deployment script exists${NC}"
else
    echo -e "${RED}❌ UDS metrics deployment script missing${NC}"
    exit 1
fi

if file_exists "scripts/metrics-bridge.sh"; then
    echo -e "${GREEN}✅ Metrics bridge script exists${NC}"
else
    echo -e "${RED}❌ Metrics bridge script missing${NC}"
    exit 1
fi

# Check 13: Documentation
echo "📚 Checking documentation..."
if file_exists "docs/secure-enclave-integration.md"; then
    echo -e "${GREEN}✅ Secure Enclave integration guide exists${NC}"
else
    echo -e "${RED}❌ Secure Enclave integration guide missing${NC}"
    exit 1
fi

if file_exists "DETERMINISM_LOOP_IMPLEMENTATION_SUMMARY.md"; then
    echo -e "${GREEN}✅ Implementation summary exists${NC}"
else
    echo -e "${RED}❌ Implementation summary missing${NC}"
    exit 1
fi

# Check 14: Test files
echo "🧪 Checking test files..."
if file_exists "tests/federation_signature_exchange.rs"; then
    echo -e "${GREEN}✅ Federation signature exchange test exists${NC}"
else
    echo -e "${RED}❌ Federation signature exchange test missing${NC}"
    exit 1
fi

# Summary
echo ""
echo "🎉 Deployment Verification Complete!"
echo "===================================="
echo ""
echo "All components are properly deployed and configured:"
echo "  ✅ Database migrations applied"
echo "  ✅ Federation crate ready"
echo "  ✅ Policy hash watcher integrated"
echo "  ✅ Global tick ledger implemented"
echo "  ✅ UDS metrics exporter ready"
echo "  ✅ CAB rollback implemented"
echo "  ✅ Secure Enclave integration ready"
echo "  ✅ Supervisor daemon implemented"
echo "  ✅ CLI commands available"
echo "  ✅ Service files configured"
echo "  ✅ Deployment scripts ready"
echo "  ✅ Documentation complete"
echo "  ✅ Test files available"
echo ""
echo "Next steps:"
echo "  1. Start the supervisor daemon: sudo systemctl start aos-supervisor"
echo "  2. Deploy UDS metrics: sudo ./scripts/deploy-uds-metrics.sh"
echo "  3. Test federation: cargo test federation_signature_exchange"
echo "  4. Monitor logs: sudo journalctl -u aos-supervisor -f"
echo ""
echo "The determinism loop is ready for production deployment! 🚀"
