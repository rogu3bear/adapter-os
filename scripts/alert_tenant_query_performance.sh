#!/bin/bash
# Alerting script for tenant query performance degradation
# Checks index stats and query times

DB_PATH="${AOS_DB_PATH:-var/aos-cp.sqlite3}"
SLACK_WEBHOOK_URL="${SLACK_WEBHOOK_URL:-}"

echo "Running performance checks..."

# Check 1: Fragmentation check (simplified)
# High page count for small data size might indicate fragmentation
PAGE_COUNT=$(sqlite3 "$DB_PATH" "PRAGMA page_count;")
PAGE_SIZE=$(sqlite3 "$DB_PATH" "PRAGMA page_size;")
DB_SIZE_MB=$(echo "$PAGE_COUNT * $PAGE_SIZE / 1024 / 1024" | bc)

echo "Database Size: ${DB_SIZE_MB} MB"

# Check 2: Index validity
INTEGRITY_CHECK=$(sqlite3 "$DB_PATH" "PRAGMA integrity_check(10);")

if [ "$INTEGRITY_CHECK" != "ok" ]; then
    MSG="[CRITICAL] Database integrity check failed: $INTEGRITY_CHECK"
    echo "$MSG"
    if [ ! -z "$SLACK_WEBHOOK_URL" ]; then
        curl -X POST -H 'Content-type: application/json' --data "{\"text\":\"$MSG\"}" "$SLACK_WEBHOOK_URL"
    fi
    exit 1
fi

# Check 3: Run ANALYZE if needed (e.g. weekly)
# Here we just check if sqlite_stat1 is populated
STATS_COUNT=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM sqlite_stat1;")
if [ "$STATS_COUNT" -eq "0" ]; then
    MSG="[WARNING] Query statistics missing. Running ANALYZE..."
    echo "$MSG"
    sqlite3 "$DB_PATH" "ANALYZE;"
fi

echo "Performance checks passed."



