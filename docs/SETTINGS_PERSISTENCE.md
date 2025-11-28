# Settings Persistence

**Status:** Implemented (PRD-MULTI-01)
**Last Updated:** 2025-11-25

## Overview

The settings persistence system allows administrators to update system settings via the REST API. Settings are persisted to a file and automatically loaded on server startup, providing a bridge between the deterministic configuration system and runtime configuration updates.

## Architecture

### File-Based Persistence

Settings updates are persisted to `var/settings_override.json`. This file contains only the settings that have been explicitly updated via the API, allowing them to override the base configuration.

```
Base Config (TOML) + Settings Override (JSON) = Effective Configuration
```

### Precedence Order

1. **CLI arguments** (highest priority)
2. **Environment variables** (medium priority)
3. **Settings override file** (`var/settings_override.json`)
4. **Base configuration file** (TOML)
5. **Defaults** (lowest priority)

## API Endpoints

### GET /v1/settings

Retrieve current system settings.

**Authentication:** Admin role required

**Response:**
```json
{
  "schema_version": "0.3.0",
  "general": {
    "system_name": "AdapterOS",
    "environment": "production",
    "api_base_url": "http://localhost:8080"
  },
  "server": {
    "http_port": 8080,
    "https_port": null,
    "uds_socket_path": null,
    "production_mode": false
  },
  "security": {
    "jwt_mode": "eddsa",
    "token_ttl_seconds": 28800,
    "require_mfa": false,
    "egress_enabled": true,
    "require_pf_deny": false
  },
  "performance": {
    "max_adapters": 100,
    "max_workers": 10,
    "memory_threshold_pct": 0.85,
    "cache_size_mb": 1024
  }
}
```

### PUT /v1/settings

Update system settings.

**Authentication:** Admin role required

**Request Body:**
```json
{
  "performance": {
    "max_adapters": 200,
    "max_workers": 20,
    "memory_threshold_pct": 0.90,
    "cache_size_mb": 2048
  }
}
```

**Response:**
```json
{
  "schema_version": "0.3.0",
  "success": true,
  "restart_required": false,
  "message": "Settings updated: performance. Changes applied immediately."
}
```

## Settings Categories

### General Settings

- **system_name**: Display name for the system
- **environment**: Environment identifier (development, staging, production)
- **api_base_url**: Base URL for API endpoints

**Restart Required:** No

### Server Settings

- **http_port**: HTTP port number (1-65535)
- **https_port**: HTTPS port number (optional, 1-65535)
- **uds_socket_path**: Unix domain socket path (optional)
- **production_mode**: Enable production mode constraints

**Restart Required:** Yes

### Security Settings

- **jwt_mode**: JWT signing mode (`eddsa` or `hmac`)
- **token_ttl_seconds**: Token time-to-live (1-86400 seconds)
- **require_mfa**: Require multi-factor authentication
- **egress_enabled**: Allow network egress
- **require_pf_deny**: Require packet filter deny rules

**Restart Required:** Yes

### Performance Settings

- **max_adapters**: Maximum number of adapters (> 0)
- **max_workers**: Maximum number of workers (> 0)
- **memory_threshold_pct**: Memory threshold percentage (0.0-1.0)
- **cache_size_mb**: Cache size in megabytes (> 0)

**Restart Required:** No (applied immediately)

## Validation

All settings are validated before persistence:

1. **Port numbers**: Must be between 1 and 65535
2. **JWT mode**: Must be `eddsa` or `hmac`
3. **Token TTL**: Must be between 1 and 86400 seconds (24 hours)
4. **Max values**: Must be greater than 0
5. **Memory threshold**: Must be between 0.0 and 1.0

Invalid settings return a 400 Bad Request error.

## Audit Logging

All settings updates are logged to the audit trail:

- **Action**: `settings.update`
- **Resource Type**: `settings`
- **Resource ID**: Comma-separated list of updated sections
- **User**: Admin who made the change
- **Status**: `success` or `failure`
- **Error Message**: If status is `failure`

### Query Audit Logs

```bash
curl http://localhost:8080/v1/audit/logs?action=settings.update
```

## File Format

The override file (`var/settings_override.json`) uses the same structure as the API request:

```json
{
  "general": {
    "system_name": "Production AdapterOS",
    "environment": "production",
    "api_base_url": "https://api.example.com"
  },
  "performance": {
    "max_adapters": 200,
    "max_workers": 20,
    "memory_threshold_pct": 0.90,
    "cache_size_mb": 2048
  }
}
```

## Loading Overrides on Startup

The server automatically loads settings overrides during initialization:

```rust
use adapteros_server_api::settings_loader::load_and_apply_overrides;

// After initializing ApiConfig
if let Err(e) = load_and_apply_overrides(&api_config) {
    tracing::warn!("Failed to load settings overrides: {}", e);
}
```

## Error Handling

### File Write Failures

If the settings override file cannot be written:
- Returns HTTP 500 Internal Server Error
- Logs error to audit trail with `failure` status
- Original settings remain unchanged

### File Read Failures

If the settings override file exists but cannot be read on startup:
- Logs warning via tracing
- Server continues with base configuration
- Admin can fix the file or delete it

### Validation Failures

If settings fail validation:
- Returns HTTP 400 Bad Request
- Error message describes the validation failure
- No changes are persisted

## Migration Notes

### From Previous TODO Implementation

The placeholder implementation at line 134 in `settings.rs` has been replaced with:

1. Settings validation before persistence
2. File-based persistence to `var/settings_override.json`
3. Automatic merging with existing overrides
4. Comprehensive audit logging
5. Error handling with proper HTTP status codes

### Settings Override Loading

To enable settings override loading in existing server code, add this after `ApiConfig` initialization:

```rust
// Load settings overrides if present
if let Err(e) = adapteros_server_api::settings_loader::load_and_apply_overrides(&api_config) {
    tracing::warn!("Failed to load settings overrides: {}", e);
}
```

## Testing

### Test Settings Update

```bash
# Get current settings
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
  http://localhost:8080/v1/settings

# Update performance settings
curl -X PUT \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "performance": {
      "max_adapters": 200,
      "max_workers": 20,
      "memory_threshold_pct": 0.90,
      "cache_size_mb": 2048
    }
  }' \
  http://localhost:8080/v1/settings

# Verify override file was created
cat var/settings_override.json
```

### Test Validation

```bash
# Try to set invalid port (should fail)
curl -X PUT \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "server": {
      "http_port": 99999
    }
  }' \
  http://localhost:8080/v1/settings
# Expected: 400 Bad Request
```

### Test Audit Logging

```bash
# Update settings
curl -X PUT \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"performance": {"max_adapters": 150}}' \
  http://localhost:8080/v1/settings

# Check audit logs
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
  "http://localhost:8080/v1/audit/logs?action=settings.update&limit=10"
```

## Security Considerations

1. **Admin-Only Access**: Only users with Admin role can view or update settings
2. **Validation**: All settings are validated before persistence
3. **Audit Trail**: All changes are logged to the immutable audit trail
4. **File Permissions**: The `var/` directory should have restricted permissions (0700)
5. **Restart Required**: Critical settings (security, server) require restart to take effect

## Future Enhancements

Potential improvements for future versions:

1. **Hot Reload**: Support for reloading settings without restart for more setting types
2. **Rollback**: Ability to rollback to previous settings versions
3. **Validation Hooks**: Plugin system for custom validation rules
4. **Settings History**: Track all historical settings changes
5. **Settings Profiles**: Support for different configuration profiles (dev, staging, prod)

## References

- [CLAUDE.md - REST API Reference](../CLAUDE.md#rest-api-reference)
- [RBAC.md - Permission Matrix](./RBAC.md)
- [AUDIT_LOGGING_IMPLEMENTATION.md](./AUDIT_LOGGING_IMPLEMENTATION.md)
- [crates/adapteros-server-api/src/handlers/settings.rs](../crates/adapteros-server-api/src/handlers/settings.rs)
- [crates/adapteros-server-api/src/settings_loader.rs](../crates/adapteros-server-api/src/settings_loader.rs)
