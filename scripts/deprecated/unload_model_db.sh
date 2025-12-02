#!/bin/bash
# Unload a model directly via database
# Usage: ./scripts/unload_model_db.sh [model_id] [tenant_id]

set -e

MODEL_ID="${1}"
TENANT_ID="${2:-default}"
DB_PATH="${AOS_DB_PATH:-/Users/star/Dev/adapter-os/var/aos.db}"

if [ -z "$MODEL_ID" ]; then
    echo "Usage: $0 <model_id> [tenant_id]"
    echo ""
    echo "To list available models:"
    sqlite3 "$DB_PATH" "SELECT DISTINCT model_id, tenant_id, status FROM base_model_status ORDER BY updated_at DESC;" 2>/dev/null || echo "No models found in database"
    exit 1
fi

# Check if model exists
EXISTS=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM base_model_status WHERE model_id = '$MODEL_ID' AND tenant_id = '$TENANT_ID';" 2>/dev/null || echo "0")

if [ "$EXISTS" = "0" ]; then
    echo "Error: Model '$MODEL_ID' not found for tenant '$TENANT_ID'"
    exit 1
fi

# Get current status
CURRENT_STATUS=$(sqlite3 "$DB_PATH" "SELECT status FROM base_model_status WHERE model_id = '$MODEL_ID' AND tenant_id = '$TENANT_ID' LIMIT 1;" 2>/dev/null || echo "")

if [ "$CURRENT_STATUS" = "unloaded" ]; then
    echo "Model '$MODEL_ID' is already unloaded"
    exit 0
fi

# Update status to unloading
sqlite3 "$DB_PATH" <<EOF
UPDATE base_model_status 
SET status = 'unloading', updated_at = datetime('now')
WHERE model_id = '$MODEL_ID' AND tenant_id = '$TENANT_ID';
EOF

# Update to unloaded
UNLOADED_AT=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
sqlite3 "$DB_PATH" <<EOF
UPDATE base_model_status 
SET status = 'unloaded', 
    unloaded_at = '$UNLOADED_AT',
    loaded_at = NULL,
    memory_usage_mb = NULL,
    updated_at = datetime('now')
WHERE model_id = '$MODEL_ID' AND tenant_id = '$TENANT_ID';
EOF

echo "Model '$MODEL_ID' unloaded successfully for tenant '$TENANT_ID'"

