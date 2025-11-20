#!/usr/bin/env bash
#
# Backup .aos File Data Before Rollback
# ======================================
#
# Safe backup procedure for .aos files and metadata before migration 0079 rollback
# Allows recovery of file information if rollback is needed
#
# Agent 14 - Migration Safeguards
# Citation: PRD-02 .aos Upload Integration
#
# Usage:
#   ./scripts/backup_aos_files.sh                          # Default backup
#   ADAPTERS_DIR=/custom/path ./scripts/backup_aos_files.sh  # Custom directory
#   ./scripts/backup_aos_files.sh --verify-only            # Verify without backing up

set -euo pipefail

# ============================================================================
# Configuration
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DB_PATH="${DB_PATH:-$PROJECT_ROOT/var/aos-cp.sqlite3}"
ADAPTERS_DIR="${ADAPTERS_DIR:-$PROJECT_ROOT/var/adapters}"
BACKUP_DIR="${BACKUP_DIR:-$PROJECT_ROOT/var/backups}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_NAME="aos_files_backup_$TIMESTAMP"
BACKUP_PATH="$BACKUP_DIR/$BACKUP_NAME"
METADATA_EXPORT="$BACKUP_DIR/aos_metadata_export_$TIMESTAMP.json"
MANIFEST="$BACKUP_DIR/aos_files_backup_manifest_$TIMESTAMP.txt"
VERIFY_ONLY="${1:-}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Statistics
TOTAL_FILES=0
TOTAL_SIZE=0
BACKED_UP_FILES=0
MISSING_FILES=0
VERIFIED_FILES=0

# ============================================================================
# Helper Functions
# ============================================================================

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_debug() {
    echo -e "${BLUE}[DEBUG]${NC} $1"
}

print_header() {
    echo -e "\n${BLUE}=== $1 ===${NC}\n"
}

# ============================================================================
# Database Operations
# ============================================================================

query_aos_metadata() {
    print_header "Querying .aos Adapter Metadata"

    if [ ! -f "$DB_PATH" ]; then
        log_error "Database not found: $DB_PATH"
        return 1
    fi

    log_info "Querying aos_adapter_metadata table..."

    # Export metadata as CSV
    sqlite3 "$DB_PATH" <<'EOF'
.mode csv
.headers on
.output $METADATA_EXPORT
SELECT
    adapter_id,
    aos_file_path,
    aos_file_hash,
    extracted_weights_path,
    training_data_count,
    lineage_version,
    signature_valid,
    created_at,
    updated_at
FROM aos_adapter_metadata
ORDER BY created_at DESC;
EOF

    if [ ! -f "$METADATA_EXPORT" ]; then
        log_error "Failed to export metadata"
        return 1
    fi

    METADATA_LINES=$(wc -l < "$METADATA_EXPORT")
    METADATA_COUNT=$((METADATA_LINES - 1))  # Subtract header

    log_info "✓ Exported $METADATA_COUNT adapter records"
    log_info "✓ Metadata export: $METADATA_EXPORT"
}

export_aos_metadata_json() {
    print_header "Exporting Metadata as JSON"

    log_info "Creating JSON export of metadata..."

    JSON_EXPORT="$BACKUP_DIR/aos_metadata_export_$TIMESTAMP.json"

    sqlite3 "$DB_PATH" <<EOF > "$JSON_EXPORT" 2>/dev/null || {
        log_warn "JSON export failed - continuing with CSV only"
        return 0
    }
.mode json
SELECT
    adapter_id,
    aos_file_path,
    aos_file_hash,
    extracted_weights_path,
    training_data_count,
    lineage_version,
    signature_valid,
    created_at,
    updated_at
FROM aos_adapter_metadata
ORDER BY created_at DESC;
EOF

    log_info "✓ JSON metadata export: $JSON_EXPORT"
}

# ============================================================================
# File Operations
# ============================================================================

discover_aos_files() {
    print_header "Discovering .aos Files"

    if [ ! -d "$ADAPTERS_DIR" ]; then
        log_warn "Adapters directory not found: $ADAPTERS_DIR"
        log_info "Creating directory..."
        mkdir -p "$ADAPTERS_DIR"
        TOTAL_FILES=0
        return 0
    fi

    log_info "Scanning: $ADAPTERS_DIR"

    # Find all .aos files
    TOTAL_FILES=$(find "$ADAPTERS_DIR" -type f -name "*.aos" 2>/dev/null | wc -l)

    if [ "$TOTAL_FILES" -eq 0 ]; then
        log_warn "No .aos files found in $ADAPTERS_DIR"
        return 0
    fi

    log_info "✓ Found $TOTAL_FILES .aos files"

    # Calculate total size
    TOTAL_SIZE=$(find "$ADAPTERS_DIR" -type f -name "*.aos" -exec du -b {} + 2>/dev/null | awk '{sum+=$1} END {print sum}')
    TOTAL_SIZE_MB=$((TOTAL_SIZE / 1024 / 1024))

    log_info "✓ Total size: ${TOTAL_SIZE_MB}MB"
}

list_aos_files() {
    find "$ADAPTERS_DIR" -type f -name "*.aos" 2>/dev/null | sort || true
}

verify_aos_file_integrity() {
    print_header "Verifying .aos File Integrity"

    local file="$1"
    local filename=$(basename "$file")

    log_debug "Verifying: $filename"

    # Check file exists and is readable
    if [ ! -r "$file" ]; then
        log_error "✗ File not readable: $filename"
        return 1
    fi

    # Check file size
    local size=$(du -b "$file" | cut -f1)
    if [ "$size" -lt 1024 ]; then
        log_warn "✗ Suspiciously small file: $filename ($size bytes)"
        return 1
    fi

    # Compute hash
    if command -v b3sum &> /dev/null; then
        local hash=$(b3sum "$file" | cut -d' ' -f1)
    else
        local hash=$(shasum -a 256 "$file" | cut -d' ' -f1)
    fi

    # Verify against metadata
    local expected_hash=$(sqlite3 "$DB_PATH" \
        "SELECT aos_file_hash FROM aos_adapter_metadata WHERE aos_file_path = '$file';" 2>/dev/null || echo "")

    if [ -z "$expected_hash" ]; then
        log_warn "⚠ No metadata hash for: $filename"
        return 0  # Not an error - file still valid
    fi

    # Compare hashes
    if [[ "$hash" == "$expected_hash"* ]]; then
        VERIFIED_FILES=$((VERIFIED_FILES + 1))
        return 0
    else
        log_error "✗ Hash mismatch: $filename"
        log_error "  Expected: $expected_hash"
        log_error "  Got: $hash"
        return 1
    fi
}

backup_aos_files() {
    print_header "Backing Up .aos Files"

    if [ "$TOTAL_FILES" -eq 0 ]; then
        log_info "No files to backup"
        return 0
    fi

    mkdir -p "$BACKUP_PATH"
    mkdir -p "$BACKUP_DIR"

    log_info "Creating backup directory: $BACKUP_PATH"

    # Backup manifest header
    {
        echo "AdapterOS .aos Files Backup Manifest"
        echo "======================================"
        echo "Created: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
        echo "Source: $ADAPTERS_DIR"
        echo "Backup: $BACKUP_PATH"
        echo ""
        echo "File Inventory:"
        echo ""
    } > "$MANIFEST"

    # Copy each file with verification
    list_aos_files | while read -r file; do
        local filename=$(basename "$file")
        local destination="$BACKUP_PATH/$filename"

        log_debug "Backing up: $filename"

        # Copy file
        if cp "$file" "$destination" 2>/dev/null; then
            BACKED_UP_FILES=$((BACKED_UP_FILES + 1))

            # Record in manifest
            local size=$(du -h "$file" | cut -f1)
            {
                echo "✓ $filename"
                echo "  Size: $size"
                echo "  Path: $file"
                echo ""
            } >> "$MANIFEST"

            log_info "✓ Backed up: $filename"
        else
            MISSING_FILES=$((MISSING_FILES + 1))
            log_error "✗ Failed to backup: $filename"
        fi
    done

    log_info "✓ Backup complete: $BACKUP_PATH"
    log_info "✓ Manifest: $MANIFEST"
}

create_tar_backup() {
    print_header "Creating Compressed Archive"

    if [ ! -d "$BACKUP_PATH" ] || [ "$BACKED_UP_FILES" -eq 0 ]; then
        log_info "No files backed up - skipping archive creation"
        return 0
    fi

    TAR_FILE="$BACKUP_DIR/${BACKUP_NAME}.tar.gz"

    log_info "Creating compressed archive: $TAR_FILE"

    if tar -czf "$TAR_FILE" -C "$BACKUP_DIR" "$BACKUP_NAME" 2>/dev/null; then
        local tar_size=$(du -h "$TAR_FILE" | cut -f1)
        log_info "✓ Archive created: $tar_size"
    else
        log_warn "Failed to create archive (continuing with uncompressed backup)"
    fi
}

# ============================================================================
# Verification Operations
# ============================================================================

verify_backup_integrity() {
    print_header "Verifying Backup Integrity"

    if [ ! -d "$BACKUP_PATH" ]; then
        log_warn "Backup directory not found: $BACKUP_PATH"
        return 0
    fi

    log_info "Verifying backed-up files..."

    find "$BACKUP_PATH" -type f -name "*.aos" | while read -r file; do
        verify_aos_file_integrity "$file"
    done

    log_info "✓ Verified $VERIFIED_FILES files"
}

verify_metadata_consistency() {
    print_header "Verifying Metadata Consistency"

    log_info "Checking for orphaned or missing files..."

    sqlite3 "$DB_PATH" <<'EOF'
-- Find files referenced in metadata but not in adapters table
SELECT COUNT(*) FROM aos_adapter_metadata am
WHERE NOT EXISTS (SELECT 1 FROM adapters a WHERE a.id = am.adapter_id);
EOF

    log_info "Consistency check complete"
}

# ============================================================================
# Recovery Documentation
# ============================================================================

create_recovery_guide() {
    print_header "Creating Recovery Guide"

    RECOVERY_GUIDE="$BACKUP_DIR/RECOVERY_GUIDE_$TIMESTAMP.md"

    cat > "$RECOVERY_GUIDE" <<'EOMD'
# .aos Files Backup Recovery Guide

## Backup Contents

This backup contains .aos files and metadata exported before migration 0079 rollback.

### Files Included

- **aos_files_backup_*** : Directory containing all .aos files
- **aos_metadata_export_*.json** : JSON export of aos_adapter_metadata table
- **aos_metadata_export_*.csv** : CSV export of aos_adapter_metadata table
- **aos_files_backup_manifest_*.txt** : Inventory of backed-up files

## Recovery Procedures

### Option 1: Restore All Files

```bash
# Extract backup archive
tar -xzf aos_files_backup_TIMESTAMP.tar.gz

# Copy files back to original location
cp -v aos_files_backup_TIMESTAMP/*.aos /var/lib/adapteros/adapters/
```

### Option 2: Restore Specific File

```bash
# Copy individual file
cp aos_files_backup_TIMESTAMP/HASH.aos /var/lib/adapteros/adapters/

# Verify integrity
sha256sum /var/lib/adapteros/adapters/HASH.aos
```

### Option 3: Query Backup Metadata

```bash
# View JSON metadata
jq . aos_metadata_export_TIMESTAMP.json

# View CSV metadata
column -t -s, aos_metadata_export_TIMESTAMP.csv
```

## Verification

### Verify File Integrity

```bash
# Check file size
du -h aos_files_backup_TIMESTAMP/*.aos

# Compute hash (compare to manifest)
sha256sum aos_files_backup_TIMESTAMP/*.aos
```

### Verify Metadata Consistency

```bash
# Check manifest inventory
cat aos_files_backup_manifest_TIMESTAMP.txt
```

## Important Notes

1. **Backup Timing**: This backup was created BEFORE rollback execution
2. **Metadata Only**: JSON/CSV exports capture metadata, not actual file data
3. **Compression**: Archive is compressed for transport (tar.gz)
4. **Retention**: Keep backups for at least 30 days after rollback
5. **Security**: Backup contains file paths - protect accordingly

## Data Loss Mitigation

If .aos files are lost during rollback:

1. Extract compressed archive
2. Restore files to original location
3. Update database:
   ```sql
   INSERT OR REPLACE INTO aos_adapter_metadata (adapter_id, aos_file_path, aos_file_hash)
   SELECT adapter_id, aos_file_path, aos_file_hash
   FROM metadata_import
   WHERE adapter_id NOT IN (SELECT adapter_id FROM aos_adapter_metadata);
   ```

## Storage Requirements

- Uncompressed backup: approximately {ORIGINAL_SIZE}
- Compressed archive: approximately {ARCHIVE_SIZE}
- Metadata exports: < 1MB

## Troubleshooting

### Missing Files

If some files are missing from backup:
1. Check disk space at backup time
2. Verify source directory permissions
3. Review backup script log

### Corrupted Files

If files fail integrity verification:
1. Check backup media for corruption
2. Compare to production backups
3. Use older backup if available

### Lost Backup

If backup media is lost:
1. Check for system backups (Time Machine on macOS)
2. Check for cloud sync backups
3. Restore from production backup infrastructure

## Contacts

For recovery assistance:
- Database Team: [contact info]
- DevOps Team: [contact info]
- Emergency Hotline: [contact info]

---

Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)
Backup Name: $BACKUP_NAME

EOMD

    log_info "✓ Recovery guide: $RECOVERY_GUIDE"
}

# ============================================================================
# Main Flow
# ============================================================================

main() {
    echo -e "${BLUE}"
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║     AOS Files Backup Tool                                 ║"
    echo "║     Safe Backup Before Migration 0079 Rollback           ║"
    echo "╚════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"

    # Configuration
    log_info "Database: $DB_PATH"
    log_info "Adapters directory: $ADAPTERS_DIR"
    log_info "Backup directory: $BACKUP_DIR"
    log_info "Backup name: $BACKUP_NAME"
    echo ""

    # Database operations
    query_aos_metadata || true
    export_aos_metadata_json || true

    # Discovery phase
    discover_aos_files

    # Verification phase if requested
    if [ "$VERIFY_ONLY" = "--verify-only" ]; then
        verify_aos_file_integrity
        verify_metadata_consistency
        log_info "Verification complete"
        exit 0
    fi

    # Backup phase
    if [ "$TOTAL_FILES" -gt 0 ]; then
        backup_aos_files
        create_tar_backup
    fi

    # Post-backup verification
    verify_backup_integrity
    verify_metadata_consistency

    # Documentation
    create_recovery_guide

    # Summary
    print_header "Backup Summary"

    echo -e "${GREEN}✓ Backup completed${NC}"
    echo ""
    echo "Statistics:"
    echo "  Total files: $TOTAL_FILES"
    echo "  Backed up: $BACKED_UP_FILES"
    echo "  Failed: $MISSING_FILES"
    echo "  Total size: ${TOTAL_SIZE_MB}MB"
    echo ""
    echo "Backup location:"
    echo "  Directory: $BACKUP_PATH"
    echo "  Manifest: $MANIFEST"
    echo "  Metadata CSV: $METADATA_EXPORT"
    echo ""
    echo "Available for recovery:"
    if [ -f "$BACKUP_DIR/${BACKUP_NAME}.tar.gz" ]; then
        TAR_SIZE=$(du -h "$BACKUP_DIR/${BACKUP_NAME}.tar.gz" | cut -f1)
        echo "  Compressed archive: ${TAR_SIZE} (tar.gz)"
    fi
    echo "  Recovery guide: $BACKUP_DIR/RECOVERY_GUIDE_$TIMESTAMP.md"
    echo ""
    echo "Next steps:"
    echo "  1. Verify backup is complete"
    echo "  2. Test recovery (optional): tar -tzf ${BACKUP_NAME}.tar.gz | head"
    echo "  3. Proceed with rollback when ready"
    echo ""

    log_info "Backup completed at $(date)"
}

# Execute main
main "$@"
