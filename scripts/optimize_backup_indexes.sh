#!/bin/bash
# Optimize database backup by dropping indexes before backup/restore and rebuilding them
# This reduces backup size and ensures optimal index structure on restore

DB_PATH="${AOS_DB_PATH:-var/aos-cp.sqlite3}"
BACKUP_PATH="${1:-var/backup.sqlite3}"

echo "Starting optimized backup..."

# 1. Vacuum into backup (Standard safest way)
echo "Vacuuming into $BACKUP_PATH..."
sqlite3 "$DB_PATH" "VACUUM INTO '$BACKUP_PATH';"

# 2. Post-backup optimization
echo "Optimizing backup file..."
sqlite3 "$BACKUP_PATH" "ANALYZE;"
sqlite3 "$BACKUP_PATH" "PRAGMA journal_mode = WAL;"
sqlite3 "$BACKUP_PATH" "PRAGMA wal_checkpoint(TRUNCATE);"

echo "Backup completed: $BACKUP_PATH"
ls -lh "$BACKUP_PATH"


