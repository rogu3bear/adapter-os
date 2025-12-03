# AdapterOS API Versioning

## Overview

AdapterOS follows semantic versioning for its HTTP API with a commitment to backward compatibility and clear deprecation policies. This document outlines versioning guarantees, breaking change definitions, and the deprecation process.

## Current Version

**API Version:** `v1` (stable)
**Schema Version:** `1.0.0`

The v1 API is stable and production-ready. No breaking changes will be introduced to v1 endpoints without a deprecation period and migration path.

## Version Identification

### Response Headers

All API responses include version information:

```http
X-API-Version: v1
Content-Type: application/vnd.aos.v1+json
```

### Schema Version

All structured responses include a `schema_version` field:

```json
{
  "schema_version": "1.0.0",
  "data": { ... }
}
```

## Version Negotiation

The API supports multiple methods for specifying the desired API version. Precedence order:

1. **Path-based version** (highest priority)
2. **Accept-Version header**
3. **Accept header**
4. **Default to v1** (lowest priority)

### 1. Path-Based Version (Recommended)

Specify version directly in the URL path:

```bash
# v1 API
curl https://api.adapteros.local/v1/adapters

# v2 API (future)
curl https://api.adapteros.local/v2/adapters
```

**Recommendation:** Use path-based versioning for explicit control and clarity.

### 2. Accept-Version Header

Specify version using the `Accept-Version` header. Supports multiple formats:

```bash
# All these are equivalent for v1:
curl -H "Accept-Version: v1" https://api.adapteros.local/adapters
curl -H "Accept-Version: 1" https://api.adapteros.local/adapters
curl -H "Accept-Version: 1.0" https://api.adapteros.local/adapters
curl -H "Accept-Version: 1.0.0" https://api.adapteros.local/adapters
```

**Use case:** Client libraries that want to set a default version across all requests without modifying URLs.

### 3. Accept Header (Content Negotiation)

Specify version using vendor-specific MIME types:

```bash
# v1 API
curl -H "Accept: application/vnd.aos.v1+json" https://api.adapteros.local/adapters

# v2 API (future)
curl -H "Accept: application/vnd.aos.v2+json" https://api.adapteros.local/adapters
```

**Use case:** REST purist clients that prefer content negotiation.

### 4. Default Version

If no version is specified, the API defaults to **v1**:

```bash
# Defaults to v1
curl https://api.adapteros.local/adapters
```

**Warning:** Relying on the default is not recommended for production applications. Always specify a version explicitly.

## Stability Guarantees

### v1 Stability Promise

AdapterOS commits to the following guarantees for v1 endpoints:

**No Breaking Changes:**
- Existing fields will NOT be removed
- Existing fields will NOT change type
- Existing fields will NOT change semantics
- Existing endpoints will NOT be removed without deprecation
- Existing query parameters will NOT be removed or change behavior

**Allowed Changes (Non-Breaking):**
- Adding new optional fields to responses
- Adding new endpoints
- Adding new optional query parameters
- Improving error messages (keeping error codes stable)
- Performance improvements

### Breaking Change Definition

A change is considered **breaking** if it requires client code modifications:

**Breaking Changes:**
- Removing a field from a response
- Renaming a field
- Changing a field's data type (e.g., `string` to `number`)
- Changing field semantics (e.g., timestamp format change)
- Removing an endpoint
- Changing HTTP status codes for existing scenarios
- Making an optional field required
- Changing authentication/authorization behavior
- Removing or renaming query parameters
- Changing error codes for existing error conditions

**Non-Breaking Changes:**
- Adding new optional fields to responses
- Adding new optional query parameters
- Adding new endpoints
- Adding new enum values (if clients handle unknown values gracefully)
- Deprecating fields (with continued support)
- Improving performance
- Fixing bugs that resulted in incorrect behavior

## Deprecation Process

When functionality needs to be phased out, AdapterOS follows a structured deprecation process:

### Phase 1: Deprecation Announcement

Deprecated endpoints receive deprecation headers:

```http
X-API-Deprecation: deprecated_at="2025-01-01T00:00:00Z"; sunset_at="2025-07-01T00:00:00Z"; replacement="/v1/new-endpoint"; migration_url="https://docs.adapteros.com/migrations/endpoint-name"
Sunset: 2025-07-01T00:00:00Z
```

**Timeline:** Minimum 6 months between deprecation and sunset.

### Phase 2: Deprecation Period

During the deprecation period (6+ months):
- Deprecated functionality continues to work
- Warning headers are present
- Documentation is updated with migration guides
- New clients should use replacement endpoints

### Phase 3: Sunset

After the sunset date:
- Deprecated endpoint returns `410 Gone` status
- Error response includes migration instructions
- Replacement endpoint is fully operational

### Example Deprecation Response

```json
{
  "schema_version": "1.0.0",
  "error": "This endpoint has been sunset. Please use /v1/code/repositories instead.",
  "code": "ENDPOINT_SUNSET",
  "details": {
    "sunset_at": "2025-07-01T00:00:00Z",
    "replacement": "/v1/code/repositories",
    "migration_guide": "https://docs.adapteros.com/migrations/repositories"
  }
}
```

## Version History

### v1.0.0 (Current - Stable)

**Released:** 2025-01-15
**Status:** Stable

**Endpoints:** 300+ endpoints across:
- Adapter management (`/v1/adapters/*`)
- Inference (`/v1/infer/*`, `/v1/chat/*`)
- Training (`/v1/training/*`)
- RAG (`/v1/collections/*`, `/v1/documents/*`)
- Stacks (`/v1/stacks/*`)
- Policies (`/v1/policies/*`)
- Workers (`/v1/workers/*`)
- System (`/v1/health`, `/v1/metrics/*`)

### v2 (Future - Not Yet Released)

**Status:** Planned for future release

Potential v2 improvements under consideration:
- Enhanced RAG vector retrieval
- Unified adapter/stack lifecycle API
- Streaming improvements
- GraphQL support

**Note:** v2 is not yet available. This is a placeholder for future planning.

## Client Best Practices

### 1. Always Specify Version

**Good:**
```typescript
const client = new AdapterOSClient({
  baseURL: 'https://api.adapteros.local/v1',
  // or
  headers: { 'Accept-Version': '1.0.0' }
});
```

**Bad:**
```typescript
const client = new AdapterOSClient({
  baseURL: 'https://api.adapteros.local', // No version!
});
```

### 2. Monitor Deprecation Headers

Check for `X-API-Deprecation` headers in responses:

```typescript
if (response.headers['x-api-deprecation']) {
  console.warn('Deprecated API in use:', response.headers['x-api-deprecation']);
  // Log to monitoring system
}
```

### 3. Handle Unknown Fields Gracefully

Ignore unknown fields in responses to allow for non-breaking additions:

```typescript
// Good: Destructure only known fields
const { id, name, status } = adapter;

// Bad: Strict validation that fails on new fields
const adapter = strictSchema.parse(response); // Breaks when new fields added
```

### 4. Pin to Major Version

Lock to a major version but accept minor/patch updates:

```bash
# In package.json or requirements.txt
adapteros-client: "^1.0.0"  # Accepts 1.x.x updates
```

## Migration Guides

When migrating between API versions, refer to version-specific guides:

- **v1 → v2** (future): `docs/migrations/v1-to-v2.md`

## Backward Compatibility Testing

AdapterOS maintains a compatibility test suite:

```bash
# Run backward compatibility tests
cargo test -p adapteros-server-api -- backward_compat

# Test v1 endpoint stability
cargo test -p adapteros-server-api -- v1_stable
```

## Support Policy

**v1 Support:**
- Active development and bug fixes
- Security updates
- No sunset date planned
- Guaranteed support through at least 2027

**Future Versions:**
- v2: Planned for future release
- Support policy will be announced with v2 release

## Contact

For questions about API versioning:
- Documentation: `https://docs.adapteros.com`
- Issues: GitHub Issues
- Security: `security@adapteros.com`

---

**Last Updated:** 2025-01-15
**Document Version:** 1.0.0
