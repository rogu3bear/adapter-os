#!/bin/bash
# Automated maintenance for tenant-scoped database optimization
# Recommended schedule: Daily at low-traffic time

DB_PATH="${AOS_DB_PATH:-var/aos-cp.sqlite3}"

echo "Starting DB Maintenance: $(date)"

# 1. Incremental Vacuum to reclaim space without locking too long
echo "Running Incremental Vacuum..."
sqlite3 "$DB_PATH" "PRAGMA incremental_vacuum(500);"

# 2. Optimize Indexes (ANALYZE)
echo "Updating Query Statistics..."
sqlite3 "$DB_PATH" "ANALYZE adapters;"
sqlite3 "$DB_PATH" "ANALYZE documents;"
sqlite3 "$DB_PATH" "ANALYZE chat_messages;"

# 3. Checkpoint WAL
echo "Checkpointing WAL..."
sqlite3 "$DB_PATH" "PRAGMA wal_checkpoint(PASSIVE);"

echo "Maintenance completed: $(date)"


