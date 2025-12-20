#!/bin/bash
# Real-time monitoring of tenant-scoped query performance
# Usage: ./monitor_tenant_query_performance.sh [tenant_id]

TENANT_ID="${1:-test-tenant}"
DB_PATH="${AOS_DB_PATH:-var/aos-cp.sqlite3}"
LOG_FILE="var/query_performance.log"

mkdir -p var

echo "Monitoring query performance for tenant: $TENANT_ID"
echo "Press Ctrl+C to stop"

# Create a temporary SQL script to measure timing
cat <<EOF > var/bench_query.sql
.timer on
.output /dev/null
SELECT * FROM adapters WHERE tenant_id = '$TENANT_ID' AND active = 1 ORDER BY tier ASC, created_at DESC;
SELECT * FROM documents WHERE tenant_id = '$TENANT_ID' ORDER BY created_at DESC LIMIT 50;
.output stdout
EOF

while true; do
    TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    
    # Run benchmark queries and capture timing output
    # sqlite3 outputs timing to stderr usually, redirect to verify
    OUTPUT=$(sqlite3 "$DB_PATH" < var/bench_query.sql 2>&1)
    
    # Extract times (Run Time: real 0.000 user 0.000 sys 0.000)
    REAL_TIME=$(echo "$OUTPUT" | grep "Run Time: real" | awk '{print $4}')
    
    if [ ! -z "$REAL_TIME" ]; then
        echo "$TIMESTAMP - Tenant: $TENANT_ID - Execution Time: ${REAL_TIME}s" >> "$LOG_FILE"
        
        # Alert if slow (> 0.050s = 50ms)
        IS_SLOW=$(echo "$REAL_TIME > 0.050" | bc -l)
        if [ "$IS_SLOW" -eq 1 ]; then
            echo "[ALERT] Slow query detected! Time: ${REAL_TIME}s"
        fi
    fi
    
    sleep 5
done


