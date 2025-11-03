#!/bin/bash
# AdapterOS Backup Verification Script
# Verifies integrity of backup files

set -euo pipefail

BACKUP_DIR="${AOS_BACKUP_DIR:-/var/backups/adapteros}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}🔍 Verifying AdapterOS backups...${NC}"

# Check if backup directory exists
if [ ! -d "$BACKUP_DIR" ]; then
    echo -e "${RED}❌ Backup directory not found: $BACKUP_DIR${NC}"
    exit 1
fi

# Find latest database backup
LATEST_DB_BACKUP=$(find "$BACKUP_DIR" -name "aos-backup-*.tar.gz" -type f -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f2-)

if [ -z "$LATEST_DB_BACKUP" ]; then
    echo -e "${RED}❌ No database backups found in $BACKUP_DIR${NC}"
    echo -e "${YELLOW}Expected pattern: aos-backup-*.tar.gz${NC}"
    exit 1
fi

echo "📊 Checking latest database backup: $(basename "$LATEST_DB_BACKUP")"

# Check if backup file is readable
if [ ! -r "$LATEST_DB_BACKUP" ]; then
    echo -e "${RED}❌ Database backup file not readable: $LATEST_DB_BACKUP${NC}"
    exit 1
fi

# Extract and verify database backup
TEMP_DIR=$(mktemp -d)
if [ ! -d "$TEMP_DIR" ]; then
    echo -e "${RED}❌ Failed to create temporary directory${NC}"
    exit 1
fi

trap "rm -rf $TEMP_DIR" EXIT

cd "$TEMP_DIR"
if ! tar xzf "$LATEST_DB_BACKUP" 2>/dev/null; then
    echo -e "${RED}❌ Failed to extract database backup${NC}"
    exit 1
fi

# Find the database file (handles timestamp in name)
DB_FILE=$(ls aos-db-*.sqlite3 2>/dev/null)
if [ -z "$DB_FILE" ] || [ ! -f "$DB_FILE" ]; then
    echo -e "${RED}❌ Database file not found in backup archive${NC}"
    exit 1
fi

# Test database integrity
if ! command -v sqlite3 >/dev/null 2>&1; then
    echo -e "${RED}❌ sqlite3 command not found${NC}"
    exit 1
fi

INTEGRITY=$(sqlite3 "$DB_FILE" "PRAGMA integrity_check;" 2>/dev/null)
if [[ "$INTEGRITY" == "ok" ]]; then
    echo -e "${GREEN}✅ Database integrity verified${NC}"
else
    echo -e "${RED}❌ Database integrity check failed: $INTEGRITY${NC}"
    exit 1
fi

# Check model backups
LATEST_MODEL_BACKUP=$(find "$BACKUP_DIR" -name "models-*.tar.gz" -type f -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f2-)

if [ -n "$LATEST_MODEL_BACKUP" ]; then
    echo "📁 Checking latest model backup: $(basename "$LATEST_MODEL_BACKUP")"

    # Check if model backup is readable
    if [ ! -r "$LATEST_MODEL_BACKUP" ]; then
        echo -e "${RED}❌ Model backup file not readable: $LATEST_MODEL_BACKUP${NC}"
        exit 1
    fi

    # Quick check - count files in model backup
    if MODEL_FILES=$(tar tzf "$LATEST_MODEL_BACKUP" 2>/dev/null | grep -E "\.(safetensors|json)$" | wc -l 2>/dev/null); then
        if [ "$MODEL_FILES" -gt 0 ]; then
            echo -e "${GREEN}✅ Model backup contains $MODEL_FILES model files${NC}"
        else
            echo -e "${YELLOW}⚠️  No model files found in backup${NC}"
        fi
    else
        echo -e "${RED}❌ Failed to read model backup archive${NC}"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠️  No model backups found${NC}"
fi

# Check backup freshness
BACKUP_AGE=$(find "$BACKUP_DIR" -name "aos-backup-*.tar.gz" -type f -mmin +1440 | wc -l)  # Older than 24 hours
if [ "$BACKUP_AGE" -gt 0 ]; then
    echo -e "${YELLOW}⚠️  Some backups are older than 24 hours${NC}"
else
    echo -e "${GREEN}✅ All backups are fresh (less than 24 hours old)${NC}"
fi

echo -e "${GREEN}✅ Backup verification completed${NC}"
