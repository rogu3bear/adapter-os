# Tenant & Settings API Guide

Quick reference for PRD-MULTI-01 tenant management and settings APIs.

## Table of Contents
- [Tenant Management](#tenant-management)
- [Settings Management](#settings-management)
- [Plugin Management](#plugin-management)
- [Examples](#examples)

## Tenant Management

### Create Tenant

```bash
POST /v1/tenants
Authorization: Bearer <admin_token>
Content-Type: application/json

{
  "name": "acme-corp",
  "itar_flag": false
}
```

**Response:**
```json
{
  "schema_version": "1.0",
  "id": "01234567-89ab-cdef-0123-456789abcdef",
  "name": "acme-corp",
  "itar_flag": false,
  "created_at": "2025-11-25 10:00:00",
  "status": "active",
  "max_adapters": null,
  "max_training_jobs": null,
  "max_storage_gb": null,
  "rate_limit_rpm": 1000
}
```

### Update Tenant

```bash
PUT /v1/tenants/{tenant_id}
Authorization: Bearer <admin_token>
Content-Type: application/json

{
  "name": "acme-corporation",
  "max_adapters": 50,
  "max_training_jobs": 5,
  "max_storage_gb": 100.0,
  "rate_limit_rpm": 500
}
```

### List Tenants

```bash
GET /v1/tenants
Authorization: Bearer <token>
```

**Response:**
```json
[
  {
    "schema_version": "1.0",
    "id": "tenant-1",
    "name": "acme-corp",
    "itar_flag": false,
    "created_at": "2025-11-25 10:00:00",
    "status": "active",
    "updated_at": "2025-11-25 11:30:00",
    "default_stack_id": "stack-1",
    "max_adapters": 50,
    "max_training_jobs": 5,
    "max_storage_gb": 100.0,
    "rate_limit_rpm": 500
  }
]
```

### Pause Tenant

```bash
POST /v1/tenants/{tenant_id}/pause
Authorization: Bearer <admin_token>
```

**Use Cases:**
- Temporarily suspend operations for maintenance
- Stop billing while preserving tenant data
- Enforce policy violations

### Archive Tenant

```bash
POST /v1/tenants/{tenant_id}/archive
Authorization: Bearer <admin_token>
```

**Use Cases:**
- Long-term deactivation
- Customer cancellation
- Compliance retention

### Get Tenant Usage

```bash
GET /v1/tenants/{tenant_id}/usage
Authorization: Bearer <token>
```

**Response:**
```json
{
  "schema_version": "1.0",
  "tenant_id": "tenant-1",
  "cpu_usage_pct": 0.0,
  "gpu_usage_pct": 0.0,
  "memory_used_gb": 0.0,
  "memory_total_gb": 0.0,
  "inference_count_24h": 1250,
  "active_adapters_count": 12
}
```

### Assign Policies

```bash
POST /v1/tenants/{tenant_id}/policies
Authorization: Bearer <admin_token>
Content-Type: application/json

{
  "policy_ids": ["cp-001", "cp-egress-002", "cp-evidence-004"]
}
```

### Assign Adapters

```bash
POST /v1/tenants/{tenant_id}/adapters
Authorization: Bearer <admin_token>
Content-Type: application/json

{
  "adapter_ids": ["adapter-1", "adapter-2"]
}
```

### Default Stack Management

#### Get Default Stack
```bash
GET /v1/tenants/{tenant_id}/default-stack
Authorization: Bearer <token>
```

#### Set Default Stack
```bash
PUT /v1/tenants/{tenant_id}/default-stack
Authorization: Bearer <token>
Content-Type: application/json

{
  "stack_id": "stack.production-env"
}
```

#### Clear Default Stack
```bash
DELETE /v1/tenants/{tenant_id}/default-stack
Authorization: Bearer <token>
```

## Settings Management

### Get System Settings

```bash
GET /v1/settings
Authorization: Bearer <admin_token>
```

**Response:**
```json
{
  "schema_version": "1.0",
  "general": {
    "system_name": "AdapterOS",
    "environment": "production",
    "api_base_url": "https://api.adapteros.example.com"
  },
  "server": {
    "http_port": 8080,
    "https_port": 8443,
    "uds_socket_path": "/var/run/adapteros.sock",
    "production_mode": true
  },
  "security": {
    "jwt_mode": "eddsa",
    "token_ttl_seconds": 28800,
    "require_mfa": false,
    "egress_enabled": false,
    "require_pf_deny": true
  },
  "performance": {
    "max_adapters": 100,
    "max_workers": 10,
    "memory_threshold_pct": 0.85,
    "cache_size_mb": 1024
  }
}
```

### Update System Settings

```bash
PUT /v1/settings
Authorization: Bearer <admin_token>
Content-Type: application/json

{
  "general": {
    "system_name": "AdapterOS Production",
    "environment": "production",
    "api_base_url": "https://api.adapteros.example.com"
  },
  "security": {
    "jwt_mode": "eddsa",
    "token_ttl_seconds": 14400,
    "require_mfa": true,
    "egress_enabled": false,
    "require_pf_deny": true
  },
  "performance": {
    "max_adapters": 200,
    "max_workers": 20,
    "memory_threshold_pct": 0.80,
    "cache_size_mb": 2048
  }
}
```

**Response:**
```json
{
  "schema_version": "1.0",
  "success": true,
  "restart_required": true,
  "message": "Settings updated: security, performance. Restart required for changes to take effect."
}
```

**Restart Required For:**
- Server settings (ports, UDS socket, production mode)
- Security settings (JWT mode, MFA, egress control)

**Applied Immediately:**
- General settings
- Performance settings

## Plugin Management

### List Plugins

```bash
GET /v1/plugins
Authorization: Bearer <token>
```

**Response:**
```json
{
  "plugins": [
    {
      "plugin": "code-intelligence",
      "tenant": "tenant-1",
      "enabled": true,
      "health": {
        "status": "Running",
        "details": null
      }
    },
    {
      "plugin": "federation",
      "tenant": "tenant-1",
      "enabled": false,
      "health": {
        "status": "Stopped",
        "details": null
      }
    }
  ]
}
```

### Get Plugin Status

```bash
GET /v1/plugins/{name}
Authorization: Bearer <token>
```

### Enable Plugin

```bash
POST /v1/plugins/{name}/enable
Authorization: Bearer <admin_or_operator_token>
```

**Response:**
```json
{
  "status": "enabled",
  "plugin": "code-intelligence",
  "tenant": "tenant-1"
}
```

### Disable Plugin

```bash
POST /v1/plugins/{name}/disable
Authorization: Bearer <admin_or_operator_token>
```

## Examples

### Example 1: Create and Configure New Tenant

```bash
# 1. Create tenant
curl -X POST http://localhost:8080/v1/tenants \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "startup-inc",
    "itar_flag": false
  }'

# Response: { "id": "tenant-123", ... }

# 2. Set resource limits
curl -X PUT http://localhost:8080/v1/tenants/tenant-123 \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "max_adapters": 25,
    "max_training_jobs": 3,
    "max_storage_gb": 50.0,
    "rate_limit_rpm": 300
  }'

# 3. Assign policies
curl -X POST http://localhost:8080/v1/tenants/tenant-123/policies \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "policy_ids": ["cp-001", "cp-egress-002"]
  }'

# 4. Set default stack
curl -X PUT http://localhost:8080/v1/tenants/tenant-123/default-stack \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "stack_id": "stack.general-purpose"
  }'
```

### Example 2: Monitor Tenant Usage

```bash
# Get current usage
curl -X GET http://localhost:8080/v1/tenants/tenant-123/usage \
  -H "Authorization: Bearer $TOKEN"

# Response:
# {
#   "tenant_id": "tenant-123",
#   "inference_count_24h": 1523,
#   "active_adapters_count": 18,
#   ...
# }

# Check against limits
curl -X GET http://localhost:8080/v1/tenants/tenant-123 \
  -H "Authorization: Bearer $TOKEN"

# Compare active_adapters_count (18) against max_adapters (25)
# 18/25 = 72% utilization
```

### Example 3: Pause and Reactivate Tenant

```bash
# Pause tenant for maintenance
curl -X POST http://localhost:8080/v1/tenants/tenant-123/pause \
  -H "Authorization: Bearer $ADMIN_TOKEN"

# Response: { "status": "paused", ... }

# Perform maintenance...

# Reactivate tenant (future: POST /v1/tenants/{id}/activate)
# Currently: Update tenant with status change via database
```

### Example 4: Configure System Settings

```bash
# Get current settings
curl -X GET http://localhost:8080/v1/settings \
  -H "Authorization: Bearer $ADMIN_TOKEN"

# Update performance settings
curl -X PUT http://localhost:8080/v1/settings \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "performance": {
      "max_adapters": 150,
      "max_workers": 15,
      "memory_threshold_pct": 0.90,
      "cache_size_mb": 2048
    }
  }'

# Response indicates if restart needed
# {
#   "success": true,
#   "restart_required": false,
#   "message": "Settings updated: performance. Changes applied immediately."
# }
```

### Example 5: Plugin Management Workflow

```bash
# Enable code intelligence plugin
curl -X POST http://localhost:8080/v1/plugins/code-intelligence/enable \
  -H "Authorization: Bearer $OPERATOR_TOKEN"

# Check status
curl -X GET http://localhost:8080/v1/plugins/code-intelligence \
  -H "Authorization: Bearer $TOKEN"

# List all plugins
curl -X GET http://localhost:8080/v1/plugins \
  -H "Authorization: Bearer $TOKEN"

# Disable if needed
curl -X POST http://localhost:8080/v1/plugins/code-intelligence/disable \
  -H "Authorization: Bearer $OPERATOR_TOKEN"
```

## Authorization Matrix

| Operation | Admin | Operator | SRE | Compliance | Viewer |
|-----------|-------|----------|-----|------------|--------|
| List tenants | ✓ | ✓ | ✓ | ✓ | ✓ |
| Create tenant | ✓ | ✗ | ✗ | ✗ | ✗ |
| Update tenant | ✓ | ✗ | ✗ | ✗ | ✗ |
| Pause/archive tenant | ✓ | ✗ | ✗ | ✗ | ✗ |
| View usage | ✓ | ✓ | ✓ | ✓ | ✓ |
| Assign policies | ✓ | ✗ | ✗ | ✗ | ✗ |
| Assign adapters | ✓ | ✗ | ✗ | ✗ | ✗ |
| Get settings | ✓ | ✗ | ✗ | ✗ | ✗ |
| Update settings | ✓ | ✗ | ✗ | ✗ | ✗ |
| Enable/disable plugins | ✓ | ✓ | ✗ | ✗ | ✗ |
| View plugins | ✓ | ✓ | ✓ | ✓ | ✓ |

## Error Responses

### 400 Bad Request
```json
{
  "schema_version": "1.0",
  "error": "Invalid request",
  "code": "VALIDATION_ERROR",
  "details": "max_adapters must be positive"
}
```

### 401 Unauthorized
```json
{
  "schema_version": "1.0",
  "error": "Authentication required",
  "code": "AUTHENTICATION_ERROR"
}
```

### 403 Forbidden
```json
{
  "schema_version": "1.0",
  "error": "Insufficient permissions",
  "code": "AUTHORIZATION_ERROR",
  "details": "Admin role required"
}
```

### 404 Not Found
```json
{
  "schema_version": "1.0",
  "error": "Tenant not found",
  "code": "NOT_FOUND"
}
```

### 500 Internal Server Error
```json
{
  "schema_version": "1.0",
  "error": "Database connection failed",
  "code": "INTERNAL_SERVER_ERROR"
}
```

## Best Practices

### Tenant Management
1. **Set limits proactively**: Configure resource limits when creating tenants
2. **Monitor usage regularly**: Check usage statistics to prevent quota overruns
3. **Use status transitions**: Pause instead of deleting for temporary suspensions
4. **Document policy assignments**: Track which policies apply to each tenant

### Settings Management
1. **Test in dev first**: Always test settings changes in development
2. **Schedule restarts**: Plan for downtime when restart is required
3. **Backup configs**: Save current settings before making changes
4. **Validate values**: Ensure performance settings match hardware capabilities

### Plugin Management
1. **Enable incrementally**: Start with essential plugins only
2. **Monitor health**: Check plugin status regularly
3. **Test before prod**: Enable plugins in staging first
4. **Document dependencies**: Track which features require which plugins

## Citation

【2025-11-25†documentation†api-tenant-settings-guide】
