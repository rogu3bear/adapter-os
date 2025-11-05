#!/bin/bash
# AdapterOS Production Backup Script
# This script creates automated backups of database and configuration

set -euo pipefail

# Configuration
BACKUP_DIR="${AOS_BACKUP_DIR:-/var/backups/adapteros}"
DB_PATH="${AOS_DB_PATH:-/var/lib/adapteros/aos-cp.sqlite3}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Security and audit logging
LOG_FILE="${AOS_BACKUP_LOG:-/var/log/adapteros/backup.log}"

log_event() {
    local level="$1"
    local message="$2"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    local user=$(whoami)
    echo "[$timestamp] [$level] [$user] $message" >> "$LOG_FILE" 2>/dev/null || true
}

# Ensure log directory exists
mkdir -p "$(dirname "$LOG_FILE")" 2>/dev/null || true

# Create backup directory with secure permissions
mkdir -p "$BACKUP_DIR"
chmod 700 "$BACKUP_DIR" 2>/dev/null || true

echo -e "${GREEN}🔄 Starting AdapterOS backup...${NC}"
log_event "INFO" "Starting AdapterOS database backup"

# Database backup with integrity check
echo "📊 Backing up database..."
if [ ! -f "$DB_PATH" ]; then
    echo -e "${RED}❌ Database file not found: $DB_PATH${NC}"
    exit 1
fi

sqlite3 "$DB_PATH" ".backup '$BACKUP_DIR/aos-db-$TIMESTAMP.sqlite3'" 2>/dev/null
if [ $? -ne 0 ]; then
    echo -e "${RED}❌ Database backup failed${NC}"
    exit 1
fi

# Integrity check of the backup file
BACKUP_DB_FILE="$BACKUP_DIR/aos-db-$TIMESTAMP.sqlite3"
if [ ! -f "$BACKUP_DB_FILE" ]; then
    echo -e "${RED}❌ Backup database file not found: $BACKUP_DB_FILE${NC}"
    exit 1
fi

INTEGRITY_CHECK=$(sqlite3 "$BACKUP_DB_FILE" "PRAGMA integrity_check;" 2>/dev/null)
if [[ "$INTEGRITY_CHECK" == "ok" ]]; then
    echo "ok" > "$BACKUP_DIR/integrity-$TIMESTAMP.txt"
    echo -e "${GREEN}✅ Backup database integrity verified${NC}"
    log_event "INFO" "Database integrity check passed"
else
    echo -e "${RED}❌ Backup database integrity check failed: $INTEGRITY_CHECK${NC}"
    echo "$INTEGRITY_CHECK" > "$BACKUP_DIR/integrity-$TIMESTAMP.txt"
    log_event "ERROR" "Database integrity check failed: $INTEGRITY_CHECK"
    exit 1
fi

# Compress backup
echo "📦 Compressing backup..."
if tar czf "$BACKUP_DIR/aos-backup-$TIMESTAMP.tar.gz" \
    -C "$BACKUP_DIR" \
    "aos-db-$TIMESTAMP.sqlite3" \
    "integrity-$TIMESTAMP.txt" 2>/dev/null; then

    BACKUP_SIZE=$(du -sh "$BACKUP_DIR/aos-backup-$TIMESTAMP.tar.gz" | cut -f1)
    echo -e "${GREEN}✅ Database backup completed: $BACKUP_DIR/aos-backup-$TIMESTAMP.tar.gz (size: $BACKUP_SIZE)${NC}"
    log_event "SUCCESS" "Database backup completed successfully: $BACKUP_SIZE"
else
    echo -e "${RED}❌ Backup compression failed${NC}"
    log_event "ERROR" "Backup compression failed"
    exit 1
fi

# Cleanup old backups (keep last 30 days)
echo "🧹 Cleaning up old backups..."
find "$BACKUP_DIR" -name "aos-backup-*.tar.gz" -mtime +30 -delete 2>/dev/null || true
find "$BACKUP_DIR" -name "aos-db-*.sqlite3" -mtime +7 -delete 2>/dev/null || true
find "$BACKUP_DIR" -name "integrity-*.txt" -mtime +7 -delete 2>/dev/null || true

# Report backup size
BACKUP_SIZE=$(du -sh "$BACKUP_DIR/aos-backup-$TIMESTAMP.tar.gz" | cut -f1)
echo -e "${GREEN}✅ Backup completed successfully (size: $BACKUP_SIZE)${NC}"

# Optional: Send notification (uncomment and configure as needed)
# curl -X POST -H 'Content-type: application/json' \
#   --data "{\"text\":\"AdapterOS backup completed: $TIMESTAMP\"}" \
#   YOUR_SLACK_WEBHOOK_URL
