#!/bin/bash
# Unload a model from adapterOS
# Usage: ./scripts/unload_model.sh [model_id] [tenant_id]

set -e

MODEL_ID="${1}"
TENANT_ID="${2:-default}"
API_URL="${AOS_API_URL:-http://localhost:3000/api}"

# Check if model_id is provided
if [ -z "$MODEL_ID" ]; then
    echo "Usage: $0 <model_id> [tenant_id]"
    echo ""
    echo "Example: $0 qwen2.5-7b default"
    exit 1
fi

# Get JWT token (if available)
TOKEN="${AOS_JWT_TOKEN}"

if [ -z "$TOKEN" ]; then
    echo "Warning: AOS_JWT_TOKEN not set. Authentication may fail."
    echo "Set AOS_JWT_TOKEN environment variable with a valid JWT token."
fi

# Unload the model
echo "Unloading model: $MODEL_ID for tenant: $TENANT_ID"
echo "API URL: $API_URL"

if [ -n "$TOKEN" ]; then
    RESPONSE=$(curl -s -w "\n%{http_code}" -X POST \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        "$API_URL/v1/models/$MODEL_ID/unload" 2>&1)
else
    RESPONSE=$(curl -s -w "\n%{http_code}" -X POST \
        -H "Content-Type: application/json" \
        "$API_URL/v1/models/$MODEL_ID/unload" 2>&1)
fi

HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" = "200" ]; then
    echo "Model unloaded successfully"
    exit 0
else
    echo "Error unloading model (HTTP $HTTP_CODE):"
    echo "$BODY"
    exit 1
fi

