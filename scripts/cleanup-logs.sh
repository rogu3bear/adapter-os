#!/usr/bin/env bash
# Standalone log cleanup script for AdapterOS
#
# This script deletes log files older than the specified retention period.
# Can be run manually or via cron for additional safety.

set -euo pipefail

# Default values
LOG_DIR="${AOS_LOG_DIR:-./var/logs}"
RETENTION_DAYS="${AOS_LOG_RETENTION_DAYS:-14}"
DRY_RUN=false

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Cleanup old log files based on retention policy.

OPTIONS:
    -d, --dir DIR           Log directory (default: $LOG_DIR)
    -r, --retention DAYS    Retention period in days (default: $RETENTION_DAYS)
    -n, --dry-run          Show what would be deleted without deleting
    -h, --help             Show this help message

ENVIRONMENT VARIABLES:
    AOS_LOG_DIR                 Log directory path
    AOS_LOG_RETENTION_DAYS      Retention period in days

EXAMPLES:
    # Use defaults from environment or fallback to ./var/logs and 14 days
    $0

    # Cleanup logs older than 7 days
    $0 --retention 7

    # Dry run to see what would be deleted
    $0 --dry-run

    # Custom log directory
    $0 --dir /var/log/aos --retention 30
EOF
}

info() {
    echo -e "${BLUE}INFO: $1${NC}"
}

success() {
    echo -e "${GREEN}SUCCESS: $1${NC}"
}

warning() {
    echo -e "${YELLOW}WARNING: $1${NC}"
}

error() {
    echo -e "${RED}ERROR: $1${NC}" >&2
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -d|--dir)
            LOG_DIR="$2"
            shift 2
            ;;
        -r|--retention)
            RETENTION_DAYS="$2"
            shift 2
            ;;
        -n|--dry-run)
            DRY_RUN=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Validate inputs
if [[ ! -d "$LOG_DIR" ]]; then
    error "Log directory does not exist: $LOG_DIR"
    exit 1
fi

if ! [[ "$RETENTION_DAYS" =~ ^[0-9]+$ ]]; then
    error "Retention days must be a positive integer: $RETENTION_DAYS"
    exit 1
fi

if [[ "$RETENTION_DAYS" -eq 0 ]]; then
    warning "Retention period is 0 days, no cleanup will be performed"
    exit 0
fi

info "Starting log cleanup"
info "  Log directory: $LOG_DIR"
info "  Retention period: $RETENTION_DAYS days"
if [[ "$DRY_RUN" == "true" ]]; then
    warning "DRY RUN MODE - No files will be deleted"
fi
echo ""

# Calculate cutoff date (files older than this will be deleted)
if [[ "$(uname)" == "Darwin" ]]; then
    # macOS date command
    CUTOFF_DATE=$(date -v-${RETENTION_DAYS}d +%s)
else
    # Linux date command
    CUTOFF_DATE=$(date -d "$RETENTION_DAYS days ago" +%s)
fi

DELETED_COUNT=0
TOTAL_SIZE=0

# Find and process log files
while IFS= read -r -d '' file; do
    # Get file modification time
    if [[ "$(uname)" == "Darwin" ]]; then
        FILE_MTIME=$(stat -f %m "$file")
    else
        FILE_MTIME=$(stat -c %Y "$file")
    fi

    # Check if file is older than retention period
    if [[ "$FILE_MTIME" -lt "$CUTOFF_DATE" ]]; then
        FILE_SIZE=$(du -k "$file" | cut -f1)
        AGE_DAYS=$(( ($(date +%s) - FILE_MTIME) / 86400 ))

        if [[ "$DRY_RUN" == "true" ]]; then
            info "[DRY RUN] Would delete: $file (${FILE_SIZE}KB, ${AGE_DAYS} days old)"
        else
            info "Deleting: $file (${FILE_SIZE}KB, ${AGE_DAYS} days old)"
            if rm -f "$file"; then
                DELETED_COUNT=$((DELETED_COUNT + 1))
                TOTAL_SIZE=$((TOTAL_SIZE + FILE_SIZE))
            else
                error "Failed to delete: $file"
            fi
        fi
    fi
done < <(find "$LOG_DIR" -type f -print0)

# Summary
echo ""
if [[ "$DRY_RUN" == "true" ]]; then
    info "Dry run complete"
    info "  Files that would be deleted: $DELETED_COUNT"
    info "  Space that would be freed: ${TOTAL_SIZE}KB"
else
    if [[ "$DELETED_COUNT" -gt 0 ]]; then
        success "Cleanup complete"
        success "  Files deleted: $DELETED_COUNT"
        success "  Space freed: ${TOTAL_SIZE}KB"
    else
        info "No files to cleanup (all files within retention period)"
    fi
fi
