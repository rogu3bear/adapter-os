#!/bin/bash
set -euo pipefail

# Simple OpenAPI documentation generation for AdapterOS Server API

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OPENAPI_OUTPUT="$PROJECT_ROOT/docs/api.md"

echo "🔍 Generating OpenAPI documentation for AdapterOS Server API..."
echo "📝 Output file: $OPENAPI_OUTPUT"

# Fetch the OpenAPI spec from the running server
echo "📥 Fetching OpenAPI specification from live server..."

# Check if server is running
if ! curl -s -f "http://localhost:8080/api/api-docs/openapi.json" > /dev/null 2>&1; then
    echo "❌ Server is not running at http://localhost:8080"
    echo "💡 Start the server first:"
    echo "   cargo run --release -p adapteros-server -- --config configs/cp.toml"
    exit 1
fi

# Fetch the OpenAPI spec and format it
OPENAPI_JSON=$(curl -s "http://localhost:8080/api/api-docs/openapi.json" | jq .)

# Create the documentation file
cat > "$OPENAPI_OUTPUT" << EOF
# AdapterOS Server API Documentation

This document contains the complete OpenAPI specification for the AdapterOS Server API.

## Overview

The AdapterOS Server API provides endpoints for managing tenants, adapters, repositories, training jobs, and more in the AdapterOS system.

## Demo Credentials

The following demo credentials are available for testing:

- **Admin:** admin@aos.local / password
- **Operator:** operator@aos.local / password  
- **SRE:** sre@aos.local / password
- **Viewer:** viewer@aos.local / password

## OpenAPI Specification

\`\`\`json
$OPENAPI_JSON
\`\`\`

## API Endpoints Summary

The API provides comprehensive endpoints for:

- **Authentication** - Login and JWT token management
- **Adapters** - Register, list, and manage adapters
- **Repositories** - Git repository management and scanning
- **Training** - Training job management and monitoring
- **Domain Adapters** - Domain-specific adapter execution
- **Metrics** - System and adapter performance metrics
- **Contacts** - Contact discovery and management
- **Streams** - Real-time SSE event streams
- **Health** - Health and readiness checks

## Development

To interact with the API:

1. **Swagger UI:** http://localhost:8080/api/swagger-ui/
2. **OpenAPI Spec:** http://localhost:8080/api/api-docs/openapi.json
3. **API Base URL:** http://localhost:8080/api/

## Authentication

All protected endpoints require a JWT token obtained from the login endpoint:

\`\`\`bash
# Login
curl -X POST http://localhost:8080/api/v1/auth/login \\
  -H "Content-Type: application/json" \\
  -d '{"email":"admin@aos.local","password":"password"}'

# Use token in subsequent requests
curl -H "Authorization: Bearer <token>" \\
  http://localhost:8080/api/v1/adapters
\`\`\`

Generated on $(date)
EOF

echo "✅ OpenAPI documentation generated successfully!"
echo "📄 File: $OPENAPI_OUTPUT"
