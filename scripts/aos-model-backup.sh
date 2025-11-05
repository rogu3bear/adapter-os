#!/bin/bash
# AdapterOS Model Directory Backup Script
# This script creates backups of models and adapters

set -euo pipefail

# Configuration
MODEL_DIR="${AOS_ADAPTERS_ROOT:-/var/lib/adapteros/adapters}"
BACKUP_DIR="${AOS_BACKUP_DIR:-/var/backups/adapteros}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Security and audit logging
LOG_FILE="${AOS_BACKUP_LOG:-/var/log/adapteros/model-backup.log}"

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

echo -e "${GREEN}🔄 Starting model directory backup...${NC}"
log_event "INFO" "Starting AdapterOS model directory backup"

# Check if model directory exists and is readable
if [ ! -d "$MODEL_DIR" ]; then
    echo -e "${RED}❌ Model directory not found: $MODEL_DIR${NC}"
    log_event "ERROR" "Model directory not found: $MODEL_DIR"
    exit 1
fi

if [ ! -r "$MODEL_DIR" ]; then
    echo -e "${RED}❌ Model directory not readable: $MODEL_DIR${NC}"
    log_event "ERROR" "Model directory not readable: $MODEL_DIR"
    exit 1
fi

# Check available disk space (rough estimate: 2x model directory size)
MODEL_SIZE=$(du -sk "$MODEL_DIR" | cut -f1)
BACKUP_DISK=$(df -k "$BACKUP_DIR" | tail -1 | awk '{print $4}')
REQUIRED_SPACE=$((MODEL_SIZE * 2))  # 2x for safety margin

if [ "$BACKUP_DISK" -lt "$REQUIRED_SPACE" ]; then
    echo -e "${RED}❌ Insufficient disk space for backup. Required: ${REQUIRED_SPACE}KB, Available: ${BACKUP_DISK}KB${NC}"
    exit 1
fi

echo "📁 Backing up model directory (${MODEL_SIZE}KB)..."
BACKUP_PATH="$BACKUP_DIR/models-$TIMESTAMP"

# Create backup with hardlinks for efficiency (if possible)
if cp -rl "$MODEL_DIR" "$BACKUP_PATH" 2>/dev/null; then
    echo -e "${GREEN}✅ Using hardlinks for efficient backup${NC}"
elif cp -r "$MODEL_DIR" "$BACKUP_PATH" 2>/dev/null; then
    echo -e "${GREEN}✅ Using regular copy for backup${NC}"
else
    echo -e "${RED}❌ Failed to create model backup${NC}"
    exit 1
fi

# Compress backup
echo "📦 Compressing model backup..."
if tar czf "$BACKUP_DIR/models-$TIMESTAMP.tar.gz" \
    -C "$BACKUP_DIR" \
    "$TIMESTAMP" 2>/dev/null; then

    # Verify compression succeeded and remove uncompressed directory
    if [ -f "$BACKUP_DIR/models-$TIMESTAMP.tar.gz" ]; then
        rm -rf "$BACKUP_PATH"
        MODEL_BACKUP_SIZE=$(du -sh "$BACKUP_DIR/models-$TIMESTAMP.tar.gz" | cut -f1)
        echo -e "${GREEN}✅ Model backup completed: $BACKUP_DIR/models-$TIMESTAMP.tar.gz (size: $MODEL_BACKUP_SIZE)${NC}"
        log_event "SUCCESS" "Model backup completed successfully: $MODEL_BACKUP_SIZE"
    else
        echo -e "${RED}❌ Compression failed - archive not created${NC}"
        log_event "ERROR" "Model backup compression failed - archive not created"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠️  Compression failed, keeping uncompressed backup for manual handling${NC}"
    echo -e "${YELLOW}Manual cleanup required: $BACKUP_PATH${NC}"
    log_event "WARNING" "Model backup compression failed, manual cleanup required: $BACKUP_PATH"
fi

# Cleanup old backups (keep last 30 days)
echo "🧹 Cleaning up old model backups..."
find "$BACKUP_DIR" -name "models-*.tar.gz" -mtime +30 -delete 2>/dev/null || true
find "$BACKUP_DIR" -maxdepth 1 -type d -name "models-backup-*" -mtime +7 -exec rm -rf {} \; 2>/dev/null || true

echo -e "${GREEN}✅ Model backup process completed${NC}"
