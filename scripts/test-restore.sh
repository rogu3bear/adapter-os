#!/bin/bash
# AdapterOS Backup Restore Testing Script
# Tests backup integrity and restore procedures without affecting production

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
BACKUP_DIR="${AOS_BACKUP_DIR:-/var/backups/adapteros}"
TEST_DIR="${AOS_TEST_RESTORE_DIR:-/tmp/aos-restore-test}"
LOG_FILE="${AOS_BACKUP_LOG:-/var/log/adapteros/restore-test.log}"

# Security and audit logging
log_event() {
    local level="$1"
    local message="$2"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    local user=$(whoami)
    echo "[$timestamp] [$level] [$user] $message" >> "$LOG_FILE" 2>/dev/null || true
}

# Ensure log directory exists
mkdir -p "$(dirname "$LOG_FILE")" 2>/dev/null || true

echo -e "${GREEN}🧪 Starting AdapterOS backup restore test...${NC}"
log_event "INFO" "Starting AdapterOS backup restore test"

# Cleanup function
cleanup() {
    echo "🧹 Cleaning up test directory..."
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

# Create test directory
mkdir -p "$TEST_DIR"

# Test database backup restore
echo "📊 Testing database backup restore..."

# Find latest database backup
LATEST_DB_BACKUP=$(find "$BACKUP_DIR" -name "aos-backup-*.tar.gz" -type f -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f2-)

if [ -z "$LATEST_DB_BACKUP" ]; then
    echo -e "${RED}❌ No database backups found for testing${NC}"
    log_event "ERROR" "No database backups found for testing"
    exit 1
fi

echo "Testing backup: $(basename "$LATEST_DB_BACKUP")"

# Extract backup to test directory
cd "$TEST_DIR"
if ! tar xzf "$LATEST_DB_BACKUP" 2>/dev/null; then
    echo -e "${RED}❌ Failed to extract database backup${NC}"
    log_event "ERROR" "Failed to extract database backup for testing"
    exit 1
fi

# Find and test database file
DB_FILE=$(ls aos-db-*.sqlite3 2>/dev/null)
if [ -z "$DB_FILE" ] || [ ! -f "$DB_FILE" ]; then
    echo -e "${RED}❌ Database file not found in backup${NC}"
    log_event "ERROR" "Database file not found in backup during testing"
    exit 1
fi

# Test database integrity
if ! command -v sqlite3 >/dev/null 2>&1; then
    echo -e "${RED}❌ sqlite3 command not found${NC}"
    exit 1
fi

INTEGRITY=$(sqlite3 "$DB_FILE" "PRAGMA integrity_check;" 2>/dev/null)
if [[ "$INTEGRITY" == "ok" ]]; then
    echo -e "${GREEN}✅ Database backup integrity verified${NC}"
    log_event "SUCCESS" "Database backup integrity test passed"
else
    echo -e "${RED}❌ Database backup integrity check failed: $INTEGRITY${NC}"
    log_event "ERROR" "Database backup integrity test failed: $INTEGRITY"
    exit 1
fi

# Test basic database queries
if sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM sqlite_master WHERE type='table';" >/dev/null 2>&1; then
    TABLE_COUNT=$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM sqlite_master WHERE type='table';")
    echo -e "${GREEN}✅ Database contains $TABLE_COUNT tables${NC}"
    log_event "SUCCESS" "Database query test passed ($TABLE_COUNT tables)"
else
    echo -e "${RED}❌ Database query test failed${NC}"
    log_event "ERROR" "Database query test failed"
    exit 1
fi

# Test model backup restore
echo "📁 Testing model backup restore..."

# Find latest model backup
LATEST_MODEL_BACKUP=$(find "$BACKUP_DIR" -name "models-*.tar.gz" -type f -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f2-)

if [ -n "$LATEST_MODEL_BACKUP" ]; then
    echo "Testing backup: $(basename "$LATEST_MODEL_BACKUP")"

    # Create subdirectory for model test
    mkdir -p "$TEST_DIR/models"

    # Extract model backup to test directory
    if tar xzf "$LATEST_MODEL_BACKUP" -C "$TEST_DIR/models" 2>/dev/null; then
        # Count model files
        MODEL_COUNT=$(find "$TEST_DIR/models" -type f \( -name "*.safetensors" -o -name "*.json" \) 2>/dev/null | wc -l)
        if [ "$MODEL_COUNT" -gt 0 ]; then
            echo -e "${GREEN}✅ Model backup contains $MODEL_COUNT model files${NC}"
            log_event "SUCCESS" "Model backup test passed ($MODEL_COUNT files)"
        else
            echo -e "${YELLOW}⚠️  Model backup appears empty${NC}"
            log_event "WARNING" "Model backup test found no model files"
        fi
    else
        echo -e "${RED}❌ Failed to extract model backup${NC}"
        log_event "ERROR" "Failed to extract model backup for testing"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠️  No model backups found for testing${NC}"
    log_event "WARNING" "No model backups found for testing"
fi

# Test backup script execution (dry run)
echo "🔧 Testing backup script execution..."

# Test database backup script (dry run by checking syntax)
if bash -n scripts/aos-backup.sh 2>/dev/null; then
    echo -e "${GREEN}✅ Database backup script syntax is valid${NC}"
else
    echo -e "${RED}❌ Database backup script has syntax errors${NC}"
    exit 1
fi

# Test model backup script (dry run by checking syntax)
if bash -n scripts/aos-model-backup.sh 2>/dev/null; then
    echo -e "${GREEN}✅ Model backup script syntax is valid${NC}"
else
    echo -e "${RED}❌ Model backup script has syntax errors${NC}"
    exit 1
fi

# Test verification script (dry run by checking syntax)
if bash -n scripts/verify-backups.sh 2>/dev/null; then
    echo -e "${GREEN}✅ Backup verification script syntax is valid${NC}"
else
    echo -e "${RED}❌ Backup verification script has syntax errors${NC}"
    exit 1
fi

echo ""
echo -e "${GREEN}🎉 All backup restore tests completed successfully!${NC}"
echo ""
echo "✅ Database backup integrity verified"
echo "✅ Database queries functional"
if [ -n "$LATEST_MODEL_BACKUP" ]; then
    echo "✅ Model backup extraction works"
fi
echo "✅ Script syntax validation passed"
echo ""
echo -e "${GREEN}📋 Summary:${NC}"
echo "   - Test directory: $TEST_DIR (auto-cleaned up)"
echo "   - Database backup: $(basename "$LATEST_DB_BACKUP")"
if [ -n "$LATEST_MODEL_BACKUP" ]; then
    echo "   - Model backup: $(basename "$LATEST_MODEL_BACKUP")"
fi
echo "   - Log file: $LOG_FILE"

log_event "SUCCESS" "All backup restore tests completed successfully"
