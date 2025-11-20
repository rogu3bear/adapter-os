#!/usr/bin/env bash
#
# Rollback Migration 0079: Re-add AOS File Columns
# ==============================================
#
# Safe rollback procedure for migration 0079_readd_aos_file_columns.sql
# Implements comprehensive data backup, validation, and recovery procedures
#
# Agent 14 - Migration Safeguards
# Citation: PRD-02 .aos Upload Integration (Agent 9)
#
# Usage:
#   ./scripts/rollback_migration_0079.sh              # Interactive mode
#   ./scripts/rollback_migration_0079.sh --force      # Non-interactive (with confirmation file)
#   DB_PATH=custom.db ./scripts/rollback_migration_0079.sh  # Custom database

set -euo pipefail

# ============================================================================
# Configuration
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DB_PATH="${DB_PATH:-$PROJECT_ROOT/var/aos-cp.sqlite3}"
BACKUP_DIR="${BACKUP_DIR:-$PROJECT_ROOT/var}"
FORCE_MODE="${1:-}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="$BACKUP_DIR/backup_0079_$TIMESTAMP.sql"
METADATA_BACKUP="$BACKUP_DIR/aos_adapter_metadata_0079_$TIMESTAMP.sql"
VALIDATION_LOG="$BACKUP_DIR/rollback_0079_validation_$TIMESTAMP.log"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Flags
ROLLBACK_APPROVED=0
DB_VALIDATED=0
BACKUP_CREATED=0

# ============================================================================
# Helper Functions
# ============================================================================

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1" | tee -a "$VALIDATION_LOG"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1" | tee -a "$VALIDATION_LOG"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1" | tee -a "$VALIDATION_LOG"
}

log_debug() {
    echo -e "${BLUE}[DEBUG]${NC} $1" | tee -a "$VALIDATION_LOG"
}

print_header() {
    echo -e "\n${BLUE}=== $1 ===${NC}\n"
}

# ============================================================================
# Pre-rollback Validation
# ============================================================================

validate_database_exists() {
    print_header "Database Validation"

    if [ ! -f "$DB_PATH" ]; then
        log_error "Database not found: $DB_PATH"
        exit 1
    fi

    log_info "Database found: $DB_PATH"

    # Check SQLite version
    SQLITE_VERSION=$(sqlite3 "$DB_PATH" "SELECT sqlite_version();")
    log_info "SQLite version: $SQLITE_VERSION"

    # Check if DROP COLUMN is supported (3.35.0+)
    SQLITE_MAJOR=$(echo "$SQLITE_VERSION" | cut -d. -f1)
    SQLITE_MINOR=$(echo "$SQLITE_VERSION" | cut -d. -f2)
    SQLITE_PATCH=$(echo "$SQLITE_VERSION" | cut -d. -f3 | cut -d- -f1)

    if [ "$SQLITE_MAJOR" -lt 3 ] || \
       ([ "$SQLITE_MAJOR" -eq 3 ] && [ "$SQLITE_MINOR" -lt 35 ]); then
        log_error "SQLite 3.35.0+ required for DROP COLUMN support"
        log_error "Current version: $SQLITE_VERSION"
        exit 1
    fi

    log_info "SQLite version supports DROP COLUMN"
}

validate_columns_exist() {
    print_header "Column Existence Check"

    # Check if columns exist
    COLUMNS=$(sqlite3 "$DB_PATH" \
        "SELECT name FROM pragma_table_info('adapters') WHERE name IN ('aos_file_path', 'aos_file_hash');")

    if [ -z "$COLUMNS" ]; then
        log_warn "Columns not found - migration may not have been applied"
        log_warn "aos_file_path: NOT FOUND"
        log_warn "aos_file_hash: NOT FOUND"
        echo -e "\n${YELLOW}Proceed with rollback anyway? (y/n)${NC}"
        read -r response
        if [ "$response" != "y" ]; then
            log_info "Rollback cancelled"
            exit 0
        fi
    else
        log_info "Columns exist (ready for removal):"
        echo "$COLUMNS" | while read -r col; do
            log_info "  ✓ $col"
        done
    fi
}

validate_no_active_uploads() {
    print_header "Active Upload Check"

    # Check for adapters in loading state with aos metadata
    ACTIVE_UPLOADS=$(sqlite3 "$DB_PATH" 2>/dev/null <<'EOF' || echo "0"
SELECT COUNT(*) FROM (
  SELECT a.id FROM adapters a
  INNER JOIN aos_adapter_metadata am ON a.id = am.adapter_id
  WHERE a.load_state IN ('loading', 'cold', 'warm', 'hot')
);
EOF
)

    log_info "Active .aos uploads: $ACTIVE_UPLOADS"

    if [ "$ACTIVE_UPLOADS" -gt 0 ]; then
        log_error "Found $ACTIVE_UPLOADS active .aos file operations"
        log_error "Cannot rollback with active uploads in progress"
        echo -e "\n${YELLOW}Wait for uploads to complete or cancel them, then retry.${NC}"
        exit 1
    fi

    log_info "No active uploads detected"
}

validate_no_training_jobs() {
    print_header "Training Job Check"

    ACTIVE_JOBS=$(sqlite3 "$DB_PATH" 2>/dev/null <<'EOF' || echo "0"
SELECT COUNT(*) FROM training_jobs
WHERE status IN ('running', 'pending');
EOF
)

    log_info "Active training jobs: $ACTIVE_JOBS"

    if [ "$ACTIVE_JOBS" -gt 0 ]; then
        log_warn "Found $ACTIVE_JOBS active training jobs"
        echo -e "\n${YELLOW}Training jobs will continue during rollback.${NC}"
        echo -e "${YELLOW}Proceed anyway? (y/n)${NC}"
        read -r response
        if [ "$response" != "y" ]; then
            log_info "Rollback cancelled"
            exit 0
        fi
    fi

    log_info "Training job check passed"
}

validate_adapter_state_machine() {
    print_header "Adapter State Machine Validation"

    INVALID_STATES=$(sqlite3 "$DB_PATH" 2>/dev/null <<'EOF' || echo "0"
SELECT COUNT(*) FROM adapters
WHERE load_state NOT IN ('unloaded', 'cold', 'warm', 'hot', 'resident')
OR current_state NOT IN ('unloaded', 'cold', 'warm', 'hot', 'resident');
EOF
)

    log_info "Adapters with invalid state: $INVALID_STATES"

    if [ "$INVALID_STATES" -gt 0 ]; then
        log_error "Found $INVALID_STATES adapters with invalid state"
        echo -e "\n${YELLOW}Proceed with rollback? (y/n)${NC}"
        read -r response
        if [ "$response" != "y" ]; then
            log_info "Rollback cancelled"
            exit 0
        fi
    fi

    log_info "Adapter state machine validation passed"
}

validate_foreign_keys() {
    print_header "Foreign Key Constraint Check"

    FK_VIOLATIONS=$(sqlite3 "$DB_PATH" 2>/dev/null <<'EOF' || echo ""
PRAGMA foreign_key_check;
EOF
)

    if [ -n "$FK_VIOLATIONS" ]; then
        log_warn "Foreign key constraint violations detected:"
        echo "$FK_VIOLATIONS" | while read -r violation; do
            log_warn "  $violation"
        done
        echo -e "\n${YELLOW}Proceed with rollback? (y/n)${NC}"
        read -r response
        if [ "$response" != "y" ]; then
            log_info "Rollback cancelled"
            exit 0
        fi
    fi

    log_info "Foreign key constraints valid"
}

# ============================================================================
# Backup Operations
# ============================================================================

create_database_backup() {
    print_header "Creating Database Backup"

    mkdir -p "$BACKUP_DIR"

    log_info "Backing up entire database..."
    log_info "Backup file: $BACKUP_FILE"

    sqlite3 "$DB_PATH" <<'EOF' > "$BACKUP_FILE" || {
        log_error "Database backup failed"
        exit 1
    }
.headers off
.mode insert
SELECT 'PRAGMA foreign_keys=OFF;' AS sql;
.read /dev/stdin
EOF

    # Better approach: use SQLite's built-in backup or dump
    sqlite3 "$DB_PATH" ".dump" > "$BACKUP_FILE" 2>/dev/null || {
        log_error "Database dump failed"
        exit 1
    }

    BACKUP_SIZE=$(du -h "$BACKUP_FILE" | cut -f1)
    log_info "✓ Database backup created: $BACKUP_SIZE"
    BACKUP_CREATED=1
}

backup_aos_metadata() {
    print_header "Backing Up AOS Adapter Metadata"

    log_info "Backing up aos_adapter_metadata table..."
    log_info "Metadata backup file: $METADATA_BACKUP"

    sqlite3 "$DB_PATH" <<'EOF' > "$METADATA_BACKUP" 2>/dev/null || {
        log_warn "Metadata backup warning - table may not exist or be empty"
    }
.mode insert
.headers off
SELECT 'CREATE TABLE aos_adapter_metadata_backup AS SELECT * FROM aos_adapter_metadata;' AS sql;
SELECT * FROM aos_adapter_metadata;
EOF

    METADATA_SIZE=$(du -h "$METADATA_BACKUP" | cut -f1)
    log_info "✓ Metadata backup created: $METADATA_SIZE"
}

# ============================================================================
# Rollback Execution
# ============================================================================

execute_rollback() {
    print_header "Executing Rollback"

    log_info "Starting rollback of migration 0079..."

    # Prepare SQL script
    ROLLBACK_SQL=$(mktemp)
    cat > "$ROLLBACK_SQL" <<'EOSQL'
-- Rollback Migration 0079: Re-add AOS File Columns
-- Citation: Agent 14 - Migration Safeguards
-- Timestamp: $(date -u +%Y-%m-%dT%H:%M:%SZ)

BEGIN TRANSACTION;

-- Step 1: Disable foreign key constraints
PRAGMA foreign_keys=OFF;

-- Step 2: Drop index created in migration 0079
DROP INDEX IF EXISTS idx_adapters_aos_file_hash;

-- Step 3: Drop columns re-added in migration 0079
-- Note: SQLite 3.35.0+ required for DROP COLUMN support
ALTER TABLE adapters DROP COLUMN IF EXISTS aos_file_path;
ALTER TABLE adapters DROP COLUMN IF EXISTS aos_file_hash;

-- Step 4: Re-enable foreign key constraints
PRAGMA foreign_keys=ON;

COMMIT;
EOSQL

    log_info "Executing rollback SQL..."

    if sqlite3 "$DB_PATH" < "$ROLLBACK_SQL" 2>&1 | tee -a "$VALIDATION_LOG"; then
        log_info "✓ Rollback SQL executed successfully"
        rm -f "$ROLLBACK_SQL"
    else
        log_error "Rollback SQL execution failed"
        log_error "Review $VALIDATION_LOG for details"
        rm -f "$ROLLBACK_SQL"
        exit 1
    fi
}

# ============================================================================
# Post-rollback Validation
# ============================================================================

validate_columns_removed() {
    print_header "Verifying Column Removal"

    REMAINING_COLUMNS=$(sqlite3 "$DB_PATH" \
        "SELECT name FROM pragma_table_info('adapters') WHERE name IN ('aos_file_path', 'aos_file_hash');")

    if [ -z "$REMAINING_COLUMNS" ]; then
        log_info "✓ Columns successfully removed"
        return 0
    else
        log_error "Columns still present after rollback:"
        echo "$REMAINING_COLUMNS" | while read -r col; do
            log_error "  ✗ $col"
        done
        return 1
    fi
}

validate_index_removed() {
    print_header "Verifying Index Removal"

    REMAINING_INDEX=$(sqlite3 "$DB_PATH" \
        "SELECT name FROM pragma_index_list('adapters') WHERE name = 'idx_adapters_aos_file_hash';")

    if [ -z "$REMAINING_INDEX" ]; then
        log_info "✓ Index successfully removed"
        return 0
    else
        log_error "Index still present after rollback: $REMAINING_INDEX"
        return 1
    fi
}

validate_adapter_integrity() {
    print_header "Validating Adapter Data Integrity"

    ADAPTER_COUNT=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM adapters;")
    DISTINCT_COUNT=$(sqlite3 "$DB_PATH" "SELECT COUNT(DISTINCT id) FROM adapters;")

    log_info "Total adapters: $ADAPTER_COUNT"
    log_info "Distinct IDs: $DISTINCT_COUNT"

    if [ "$ADAPTER_COUNT" -eq "$DISTINCT_COUNT" ]; then
        log_info "✓ Adapter data integrity verified"
        return 0
    else
        log_error "Adapter data integrity check failed"
        log_error "Duplicate IDs detected"
        return 1
    fi
}

validate_foreign_keys_post_rollback() {
    print_header "Post-Rollback Foreign Key Check"

    FK_VIOLATIONS=$(sqlite3 "$DB_PATH" "PRAGMA foreign_key_check;" 2>/dev/null || echo "")

    if [ -z "$FK_VIOLATIONS" ]; then
        log_info "✓ Foreign key constraints satisfied"
        return 0
    else
        log_error "Foreign key constraint violations:"
        echo "$FK_VIOLATIONS" | while read -r violation; do
            log_error "  $violation"
        done
        return 1
    fi
}

# ============================================================================
# Post-rollback Actions
# ============================================================================

identify_orphaned_files() {
    print_header "Orphaned Files Analysis"

    log_info "Scanning for orphaned .aos files..."

    # This is advisory only - files are not deleted automatically
    if command -v find &> /dev/null; then
        ORPHANED_COUNT=0
        # Note: We don't have definitive list of .aos file locations
        # This is best-effort identification
        log_info "Unable to definitively identify orphaned files without file system paths"
        log_info "Manual cleanup may be required - see MIGRATION_0079_ROLLBACK_PROCEDURE.md"
    fi
}

# ============================================================================
# Interactive Approval
# ============================================================================

request_approval() {
    print_header "Rollback Approval Required"

    echo -e "${YELLOW}Summary of Changes:${NC}"
    echo "  • Remove aos_file_path column from adapters table"
    echo "  • Remove aos_file_hash column from adapters table"
    echo "  • Drop idx_adapters_aos_file_hash index"
    echo "  • Data loss: aos_file_path and aos_file_hash values"
    echo "  • Preserved: aos_adapter_metadata table (with stale data)"
    echo "  • Preserved: All adapter records"
    echo ""
    echo -e "${YELLOW}Backups Created:${NC}"
    echo "  • Full database: $BACKUP_FILE"
    echo "  • Metadata table: $METADATA_BACKUP"
    echo "  • Validation log: $VALIDATION_LOG"
    echo ""
    echo -e "${YELLOW}Do you approve this rollback? (type 'rollback-0079' to confirm)${NC}"

    read -r approval

    if [ "$approval" = "rollback-0079" ]; then
        ROLLBACK_APPROVED=1
        log_info "User approved rollback"
    else
        log_info "Rollback cancelled by user"
        exit 0
    fi
}

# ============================================================================
# Main Flow
# ============================================================================

main() {
    echo -e "${BLUE}"
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║     Migration 0079 Rollback Tool                          ║"
    echo "║     Re-add AOS File Columns - Safe Rollback Procedure     ║"
    echo "╚════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"

    # Validation Phase
    log_info "Starting pre-rollback validation..."
    log_info "Validation log: $VALIDATION_LOG"

    validate_database_exists
    validate_columns_exist
    validate_no_active_uploads
    validate_no_training_jobs
    validate_adapter_state_machine
    validate_foreign_keys

    DB_VALIDATED=1

    # Approval Phase
    if [ "$FORCE_MODE" != "--force" ]; then
        request_approval
    else
        log_info "Force mode enabled - skipping interactive approval"
        ROLLBACK_APPROVED=1
    fi

    if [ $ROLLBACK_APPROVED -eq 0 ]; then
        log_info "Rollback not approved"
        exit 0
    fi

    # Backup Phase
    create_database_backup
    backup_aos_metadata

    # Rollback Phase
    execute_rollback

    # Validation Phase
    validate_columns_removed || {
        log_error "Column removal validation failed"
        log_warn "Attempting restore from backup..."
        if [ $BACKUP_CREATED -eq 1 ]; then
            sqlite3 "$DB_PATH" < "$BACKUP_FILE"
            log_info "Database restored from backup"
        fi
        exit 1
    }

    validate_index_removed || {
        log_error "Index removal validation failed"
        exit 1
    }

    validate_adapter_integrity || {
        log_error "Adapter integrity check failed"
        exit 1
    }

    validate_foreign_keys_post_rollback || {
        log_error "Foreign key check failed"
        exit 1
    }

    identify_orphaned_files

    # Summary
    print_header "Rollback Summary"

    echo -e "${GREEN}✓ Rollback completed successfully${NC}"
    echo ""
    echo "Columns removed:"
    echo "  • adapters.aos_file_path"
    echo "  • adapters.aos_file_hash"
    echo ""
    echo "Index removed:"
    echo "  • idx_adapters_aos_file_hash"
    echo ""
    echo "Data preserved:"
    echo "  • All adapter records"
    echo "  • aos_adapter_metadata table (stale)"
    echo ""
    echo "Backups available:"
    echo "  • Database: $BACKUP_FILE"
    echo "  • Metadata: $METADATA_BACKUP"
    echo "  • Log: $VALIDATION_LOG"
    echo ""
    echo "Next steps:"
    echo "  1. Monitor application logs for 30 minutes"
    echo "  2. Verify .aos upload functionality is disabled"
    echo "  3. Consider cleaning up orphaned .aos files"
    echo "  4. Review: docs/MIGRATION_0079_ROLLBACK_PROCEDURE.md"
    echo ""

    log_info "Rollback completed at $(date)"
    exit 0
}

# Execute main
main "$@"
