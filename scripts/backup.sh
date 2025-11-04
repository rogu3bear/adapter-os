#!/bin/bash

# AdapterOS Backup Script
# Performs comprehensive backup of AdapterOS system state

set -euo pipefail

# Configuration
BACKUP_ROOT="${BACKUP_ROOT:-/backup}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="${BACKUP_ROOT}/${TIMESTAMP}"
LOG_FILE="${BACKUP_ROOT}/backup.log"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${GREEN}[$(date +%Y-%m-%d\ %H:%M:%S)] INFO: $1${NC}" | tee -a "$LOG_FILE"
}

log_warn() {
    echo -e "${YELLOW}[$(date +%Y-%m-%d\ %H:%M:%S)] WARN: $1${NC}" | tee -a "$LOG_FILE"
}

log_error() {
    echo -e "${RED}[$(date +%Y-%m-%d\ %H:%M:%S)] ERROR: $1${NC}" | tee -a "$LOG_FILE"
}

# Pre-flight checks
preflight_checks() {
    log_info "Performing pre-flight checks..."

    # Check if running as root or adapteros user
    if [[ $EUID -ne 0 ]] && [[ "$USER" != "adapteros" ]]; then
        log_error "This script must be run as root or adapteros user"
        exit 1
    fi

    # Check backup directory
    if [[ ! -w "$BACKUP_ROOT" ]]; then
        log_error "Backup root directory $BACKUP_ROOT is not writable"
        exit 1
    fi

    # Check available disk space (need at least 10GB free)
    local available_space
    available_space=$(df "$BACKUP_ROOT" | tail -1 | awk '{print $4}')
    if [[ $available_space -lt 10485760 ]]; then  # 10GB in KB
        log_error "Insufficient disk space. Need at least 10GB free."
        exit 1
    fi

    # Check if adapteros service is running (optional)
    if systemctl is-active --quiet adapteros; then
        log_warn "AdapterOS service is running. Consider stopping it for consistent backup."
        read -p "Continue with service running? (y/N): " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    fi

    log_info "Pre-flight checks passed"
}

# Create backup directory structure
create_backup_structure() {
    log_info "Creating backup directory structure..."

    mkdir -p "$BACKUP_DIR"/{database,models,config,logs,metrics}

    # Create metadata file
    cat > "$BACKUP_DIR/backup_metadata.json" << EOF
{
    "timestamp": "$TIMESTAMP",
    "hostname": "$(hostname)",
    "adapteros_version": "$(adapteros --version 2>/dev/null || echo 'unknown')",
    "backup_type": "${BACKUP_TYPE:-full}",
    "created_by": "$USER"
}
EOF

    log_info "Backup structure created at $BACKUP_DIR"
}

# Backup database
backup_database() {
    log_info "Backing up database..."

    local db_host="${DB_HOST:-localhost}"
    local db_port="${DB_PORT:-5432}"
    local db_name="${DB_NAME:-adapteros}"
    local db_user="${DB_USER:-adapteros}"

    # Test database connectivity
    if ! pg_isready -h "$db_host" -p "$db_port" -U "$db_user" -d "$db_name" >/dev/null 2>&1; then
        log_error "Cannot connect to database"
        return 1
    fi

    # Create database dump
    local dump_file="$BACKUP_DIR/database/adapteros.sql.gz"
    log_info "Creating database dump to $dump_file"

    pg_dump -h "$db_host" -p "$db_port" -U "$db_user" -d "$db_name" \
        --compress=9 --format=custom --verbose \
        | gzip > "$dump_file"

    # Verify dump
    if [[ ! -s "$dump_file" ]]; then
        log_error "Database dump failed or is empty"
        return 1
    fi

    log_info "Database backup completed ($(du -sh "$dump_file" | cut -f1))"
}

# Backup models and adapters
backup_models() {
    log_info "Backing up models and adapters..."

    local models_dir="${MODELS_DIR:-/var/lib/adapteros/models}"
    local adapters_dir="${ADAPTERS_DIR:-/var/lib/adapteros/adapters}"
    local artifacts_dir="${ARTIFACTS_DIR:-/var/lib/adapteros/artifacts}"

    # Backup models
    if [[ -d "$models_dir" ]]; then
        log_info "Backing up models from $models_dir"
        tar -czf "$BACKUP_DIR/models/models.tar.gz" -C "$models_dir" . \
            --exclude='*.tmp' --exclude='*.lock' 2>/dev/null || true

        log_info "Models backup completed ($(du -sh "$BACKUP_DIR/models/models.tar.gz" | cut -f1))"
    else
        log_warn "Models directory $models_dir not found, skipping"
    fi

    # Backup adapters
    if [[ -d "$adapters_dir" ]]; then
        log_info "Backing up adapters from $adapters_dir"
        tar -czf "$BACKUP_DIR/models/adapters.tar.gz" -C "$adapters_dir" . \
            --exclude='*.tmp' --exclude='*.lock' 2>/dev/null || true

        log_info "Adapters backup completed ($(du -sh "$BACKUP_DIR/models/adapters.tar.gz" | cut -f1))"
    else
        log_warn "Adapters directory $adapters_dir not found, skipping"
    fi

    # Backup artifacts
    if [[ -d "$artifacts_dir" ]]; then
        log_info "Backing up artifacts from $artifacts_dir"
        tar -czf "$BACKUP_DIR/models/artifacts.tar.gz" -C "$artifacts_dir" . \
            --exclude='*.tmp' --exclude='*.lock' 2>/dev/null || true

        log_info "Artifacts backup completed ($(du -sh "$BACKUP_DIR/models/artifacts.tar.gz" | cut -f1))"
    else
        log_warn "Artifacts directory $artifacts_dir not found, skipping"
    fi
}

# Backup configuration
backup_config() {
    log_info "Backing up configuration..."

    local config_files=(
        "/etc/adapteros/config.toml"
        "/etc/adapteros/security/"
        "/etc/adapteros/policies/"
    )

    for config_file in "${config_files[@]}"; do
        if [[ -e "$config_file" ]]; then
            local basename
            basename=$(basename "$config_file")
            if [[ -d "$config_file" ]]; then
                # Directory
                tar -czf "$BACKUP_DIR/config/${basename}.tar.gz" -C "$(dirname "$config_file")" "$basename" 2>/dev/null || true
            else
                # File
                cp "$config_file" "$BACKUP_DIR/config/"
            fi
            log_info "Backed up $config_file"
        else
            log_warn "Config file/directory $config_file not found"
        fi
    done
}

# Backup logs
backup_logs() {
    log_info "Backing up recent logs..."

    local log_dir="${LOG_DIR:-/var/log/adapteros}"

    if [[ -d "$log_dir" ]]; then
        # Only backup logs from last 30 days
        find "$log_dir" -name "*.log" -mtime -30 -exec cp {} "$BACKUP_DIR/logs/" \; 2>/dev/null || true
        find "$log_dir" -name "*.log.gz" -mtime -30 -exec cp {} "$BACKUP_DIR/logs/" \; 2>/dev/null || true

        # Compress logs
        if [[ -n "$(ls -A "$BACKUP_DIR/logs/" 2>/dev/null)" ]]; then
            tar -czf "$BACKUP_DIR/logs.tar.gz" -C "$BACKUP_DIR" logs/ 2>/dev/null || true
            rm -rf "$BACKUP_DIR/logs"
            log_info "Logs backup completed ($(du -sh "$BACKUP_DIR/logs.tar.gz" | cut -f1))"
        else
            log_warn "No recent logs found to backup"
        fi
    else
        log_warn "Log directory $log_dir not found"
    fi
}

# Backup metrics data
backup_metrics() {
    log_info "Backing up metrics data..."

    # Export current metrics if service is running
    if systemctl is-active --quiet adapteros; then
        curl -s http://localhost:9090/metrics > "$BACKUP_DIR/metrics/current_metrics.txt" 2>/dev/null || true
    fi

    # Backup metrics database if it exists
    local metrics_db="${METRICS_DB:-/var/lib/adapteros/metrics.db}"
    if [[ -f "$metrics_db" ]]; then
        cp "$metrics_db" "$BACKUP_DIR/metrics/"
        log_info "Metrics database backed up"
    fi
}

# Create backup manifest
create_manifest() {
    log_info "Creating backup manifest..."

    local manifest_file="$BACKUP_DIR/manifest.txt"

    {
        echo "AdapterOS Backup Manifest"
        echo "========================="
        echo "Created: $(date)"
        echo "Backup ID: $TIMESTAMP"
        echo "Hostname: $(hostname)"
        echo ""
        echo "Contents:"
        find "$BACKUP_DIR" -type f -exec ls -lh {} \; | awk '{print "  " $9 " (" $5 ")"}'
        echo ""
        echo "Checksums:"
        find "$BACKUP_DIR" -type f -exec sha256sum {} \; | sed "s|$BACKUP_DIR/||"
    } > "$manifest_file"

    log_info "Backup manifest created at $manifest_file"
}

# Cleanup old backups
cleanup_old_backups() {
    log_info "Cleaning up old backups..."

    # Keep last 7 daily backups, last 4 weekly backups, last 12 monthly backups
    local daily_backups
    daily_backups=$(find "$BACKUP_ROOT" -maxdepth 1 -type d -name "20??????" | sort | tail -n +8)
    if [[ -n "$daily_backups" ]]; then
        echo "$daily_backups" | xargs rm -rf
        log_info "Cleaned up old daily backups"
    fi

    # Note: Weekly and monthly cleanup would require more complex logic
    # For now, we just keep the last 30 backups
    local old_backups
    old_backups=$(find "$BACKUP_ROOT" -maxdepth 1 -type d -name "20??????_??????" | sort | head -n -30)
    if [[ -n "$old_backups" ]]; then
        echo "$old_backups" | xargs rm -rf 2>/dev/null || true
        log_info "Cleaned up backups older than 30 days"
    fi
}

# Main backup function
main() {
    local backup_type="${1:-full}"

    log_info "Starting AdapterOS $backup_type backup to $BACKUP_DIR"

    # Run backup steps
    preflight_checks
    create_backup_structure

    case "$backup_type" in
        "full")
            backup_database
            backup_models
            backup_config
            backup_logs
            backup_metrics
            ;;
        "database")
            backup_database
            ;;
        "models")
            backup_models
            ;;
        "config")
            backup_config
            ;;
        *)
            log_error "Unknown backup type: $backup_type"
            echo "Usage: $0 [full|database|models|config]"
            exit 1
            ;;
    esac

    create_manifest
    cleanup_old_backups

    # Calculate backup size
    local backup_size
    backup_size=$(du -sh "$BACKUP_DIR" | cut -f1)
    log_info "Backup completed successfully. Total size: $backup_size"

    # Create symlink to latest backup
    ln -sfn "$BACKUP_DIR" "$BACKUP_ROOT/latest"

    echo ""
    echo "Backup Summary:"
    echo "==============="
    echo "Location: $BACKUP_DIR"
    echo "Size: $backup_size"
    echo "Latest symlink: $BACKUP_ROOT/latest"
    echo ""
    echo "To restore, run: $0 --restore $BACKUP_DIR"
}

# Restore function
restore() {
    local backup_dir="$1"

    if [[ ! -d "$backup_dir" ]]; then
        log_error "Backup directory $backup_dir does not exist"
        exit 1
    fi

    log_info "Starting restore from $backup_dir"

    # Verify backup integrity
    if [[ ! -f "$backup_dir/manifest.txt" ]]; then
        log_error "Backup manifest not found. Backup may be corrupted."
        exit 1
    fi

    # Stop services
    log_info "Stopping AdapterOS service..."
    systemctl stop adapteros || true

    # Restore database
    if [[ -f "$backup_dir/database/adapteros.sql.gz" ]]; then
        log_info "Restoring database..."
        gunzip < "$backup_dir/database/adapteros.sql.gz" | psql -d adapteros
    fi

    # Restore models
    if [[ -f "$backup_dir/models/models.tar.gz" ]]; then
        log_info "Restoring models..."
        mkdir -p /var/lib/adapteros/models
        tar -xzf "$backup_dir/models/models.tar.gz" -C /var/lib/adapteros/models/
    fi

    # Restore adapters
    if [[ -f "$backup_dir/models/adapters.tar.gz" ]]; then
        log_info "Restoring adapters..."
        mkdir -p /var/lib/adapteros/adapters
        tar -xzf "$backup_dir/models/adapters.tar.gz" -C /var/lib/adapteros/adapters/
    fi

    # Restore configuration
    if [[ -d "$backup_dir/config" ]]; then
        log_info "Restoring configuration..."
        cp -r "$backup_dir/config"/* /etc/adapteros/ 2>/dev/null || true
    fi

    # Start services
    log_info "Starting AdapterOS service..."
    systemctl start adapteros

    log_info "Restore completed successfully"
}

# Parse command line arguments
case "${1:-}" in
    --restore)
        if [[ -z "${2:-}" ]]; then
            echo "Usage: $0 --restore <backup_directory>"
            exit 1
        fi
        restore "$2"
        ;;
    *)
        main "${1:-full}"
        ;;
esac
