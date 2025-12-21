#!/bin/bash
# Monitor index performance and utilization for tenant-scoped indexes
# Part of Migration 0210 validation and maintenance

set -e

DB_PATH="${AOS_DB_PATH:-var/aos-cp.sqlite3}"
THRESHOLD_MS=10

echo "Starting Index Performance Monitor..."
echo "Database: $DB_PATH"
echo "Threshold: ${THRESHOLD_MS}ms"

# Function to run query explanation
check_query_plan() {
    local name="$1"
    local query="$2"
    local params="$3"
    
    echo "----------------------------------------"
    echo "Analyzing: $name"
    echo "Query: $query"
    
    # Run EXPLAIN QUERY PLAN
    echo "Plan:"
    sqlite3 "$DB_PATH" "EXPLAIN QUERY PLAN $query" | while read -r line; do
        echo "  $line"
        if [[ "$line" == *"SCAN TABLE"* && "$line" != *"USING INDEX"* ]]; then
            echo "  [WARNING] FULL TABLE SCAN DETECTED!"
        fi
        if [[ "$line" == *"USE TEMP B-TREE"* ]]; then
            echo "  [WARNING] TEMP B-TREE SORT DETECTED!"
        fi
    done
}

# 1. Check Adapter Listing Index
check_query_plan "Tenant Adapter Listing" \
    "SELECT * FROM adapters WHERE tenant_id = 'test-tenant' AND active = 1 ORDER BY tier ASC, created_at DESC"

# 2. Check Adapter Hash Lookup Index
check_query_plan "Adapter Hash Lookup" \
    "SELECT * FROM adapters WHERE tenant_id = 'test-tenant' AND hash_b3 = 'test-hash' AND active = 1"

# 3. Check Document Pagination Index
check_query_plan "Document Pagination" \
    "SELECT * FROM documents WHERE tenant_id = 'test-tenant' ORDER BY created_at DESC LIMIT 20 OFFSET 0"

# 4. Check Training Jobs Index
check_query_plan "Training Jobs Listing" \
    "SELECT * FROM repository_training_jobs WHERE tenant_id = 'test-tenant' AND status = 'running' ORDER BY created_at DESC"

# 5. Check Routing Decisions Index
check_query_plan "Routing Decisions" \
    "SELECT * FROM routing_decisions WHERE tenant_id = 'test-tenant' ORDER BY timestamp DESC LIMIT 10"

echo "----------------------------------------"
echo "Index usage stats (sqlite_stat1):"
sqlite3 -header -column "$DB_PATH" "SELECT tbl, idx, stat FROM sqlite_stat1 WHERE idx LIKE 'idx_%_tenant_%'"

echo "Done."
