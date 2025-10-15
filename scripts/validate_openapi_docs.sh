#!/bin/bash
set -euo pipefail

# Validate OpenAPI documentation for AdapterOS Server API
# This script validates the OpenAPI spec in docs/api.md

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

OPENAPI_DOC="$PROJECT_ROOT/docs/api.md"
SERVER_URL="${SERVER_URL:-http://localhost:8080}"

echo "🔍 Validating OpenAPI documentation for AdapterOS Server API..."
echo "📍 API docs file: $OPENAPI_DOC"
echo "📍 Server URL: $SERVER_URL"

# Check if API docs file exists
if [ ! -f "$OPENAPI_DOC" ]; then
    echo "❌ API documentation file not found: $OPENAPI_DOC"
    echo "💡 Generate it first:"
    echo "   make openapi-docs"
    exit 1
fi

echo "✅ API documentation file exists"

# Check if swagger-cli is available
if ! command -v swagger-codegen-cli &> /dev/null && ! command -v swagger-codegen &> /dev/null; then
    if command -v swagger-cli &> /dev/null; then
        SWAGGER_CMD="swagger-cli"
    else
        echo "❌ swagger-cli not found"
        echo "💡 Install it:"
        echo "   pnpm add -g swagger-cli"
        exit 1
    fi
else
    # Fallback to other swagger tools if available
    if command -v swagger-codegen-cli &> /dev/null; then
        SWAGGER_CMD="swagger-codegen-cli"
    elif command -v swagger-codegen &> /dev/null; then
        SWAGGER_CMD="swagger-codegen"
    else
        echo "❌ No swagger validation tools found"
        exit 1
    fi
fi

echo "✅ Using $SWAGGER_CMD for validation"

# Extract JSON from markdown file for validation
echo "📥 Extracting OpenAPI JSON for validation..."

# Find the JSON part in the markdown file (between ```json and ```)
JSON_START_LINE=$(grep -n "\`\`\`json" "$OPENAPI_DOC" | head -1 | cut -d: -f1)
# Find the first closing ``` after the JSON start
JSON_END_LINE=$(sed -n "$((JSON_START_LINE + 1)),\$p" "$OPENAPI_DOC" | grep -n "^\\\`\\\`\\\`$" | head -1 | cut -d: -f1)
JSON_END_LINE=$((JSON_START_LINE + JSON_END_LINE))

if [ -z "$JSON_START_LINE" ] || [ -z "$JSON_END_LINE" ]; then
    echo "❌ Could not find JSON specification in $OPENAPI_DOC"
    exit 1
fi

# Extract JSON content (skip the first line which is ```json)
JSON_SPEC=$(sed -n "$((JSON_START_LINE + 1)),$((JSON_END_LINE - 1))p" "$OPENAPI_DOC")

if [ -z "$JSON_SPEC" ]; then
    echo "❌ Could not extract JSON specification from $OPENAPI_DOC"
    exit 1
fi

# Create temporary file for validation
TEMP_JSON=$(mktemp)
echo "$JSON_SPEC" > "$TEMP_JSON"

echo "📋 Validating OpenAPI specification..."

# Validate the JSON
if ! $SWAGGER_CMD validate "$TEMP_JSON" 2>/dev/null; then
    echo "❌ OpenAPI specification validation failed"
    echo ""
    echo "🔧 Validation errors:"
    $SWAGGER_CMD validate "$TEMP_JSON" 2>&1 || true
    rm -f "$TEMP_JSON"
    exit 1
fi

echo "✅ OpenAPI specification is valid"

# Optional: Generate fresh spec for comparison (disabled for now)
# echo "🔄 Regenerating OpenAPI spec for validation..."

# Clean up
rm -f "$TEMP_JSON"

echo ""
echo "🎉 OpenAPI documentation validation completed successfully!"
