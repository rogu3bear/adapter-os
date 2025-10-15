#!/bin/bash
set -euo pipefail

# Generate OpenAPI documentation for AdapterOS Server API
# This script uses a standalone Rust script to generate the OpenAPI spec

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

OPENAPI_OUTPUT="$PROJECT_ROOT/docs/api.md"

echo "🔍 Generating OpenAPI documentation for AdapterOS Server API..."
echo "📝 Output file: $OPENAPI_OUTPUT"

# Generate the OpenAPI spec by compiling and running the standalone binary
echo "📥 Generating OpenAPI specification..."

# Check if binary exists, build if not
if [ ! -f "target/debug/generate-openapi" ]; then
    echo "🔨 Building OpenAPI generator..."
    (cd scripts && cargo build --manifest-path Cargo-generate-openapi.toml)
fi

# Run the generator
OPENAPI_JSON=$(scripts/target/debug/generate-openapi)

if [ -z "$OPENAPI_JSON" ]; then
    echo "❌ Failed to generate OpenAPI specification"
    exit 1
fi

# Create the API documentation markdown file
cat > "$OPENAPI_OUTPUT" << 'EOF'
# AdapterOS Server API Documentation

This document contains the complete OpenAPI specification for the AdapterOS Server API.

## Overview

The AdapterOS Server API provides endpoints for managing tenants, adapters, repositories, training jobs, and more in the AdapterOS system.

## OpenAPI Specification

```json
EOF

echo "$OPENAPI_JSON" >> "$OPENAPI_OUTPUT"

cat >> "$OPENAPI_OUTPUT" << 'EOF'
```

## API Endpoints Summary

EOF

# Extract endpoint summary using jq if available
if command -v jq &> /dev/null; then
    echo "📊 Generating endpoint summary..."

    # Extract paths and methods
    echo "$OPENAPI_JSON" | jq -r '
        .paths | to_entries[] | "- **\(.key)** (\(.value | keys | join(", ")))"
    ' >> "$OPENAPI_OUTPUT" 2>/dev/null || echo "  (Could not generate summary - jq not available or JSON parsing failed)" >> "$OPENAPI_OUTPUT"
else
    echo "  (Install jq to generate endpoint summary)" >> "$OPENAPI_OUTPUT"
fi

echo "" >> "$OPENAPI_OUTPUT"
echo "Generated on $(date)" >> "$OPENAPI_OUTPUT"
echo "Server URL: $SERVER_URL" >> "$OPENAPI_OUTPUT"

echo "✅ OpenAPI documentation generated successfully!"
echo "📄 File: $OPENAPI_OUTPUT"
