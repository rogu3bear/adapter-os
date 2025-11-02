# RBAC Coverage Audit

This document captures RBAC implementation and coverage across API, UI, and CLI layers.

## Role Definitions

### Available Roles
- **Admin**: Full system access including tenant management, CPID promotion, policy updates
- **Operator**: Can manage adapters, workers, inference operations, training jobs
- **Compliance**: Can view audit logs, telemetry bundles, compliance reports
- **Viewer**: Read-only access to system status and metrics

### Role Hierarchy
Admin > Operator > Compliance > Viewer

## API Layer RBAC

### Middleware Protection
- All protected routes use `auth_middleware` to validate JWT tokens
- Claims extracted from JWT include `role` field for authorization
- Role checks performed via `require_role()` and `require_any_role()` helpers

### Endpoint Coverage
- **Admin-only endpoints**: Tenant creation/modification, CPID promotion, policy updates, user management
- **Operator endpoints**: Adapter management, worker operations, inference, training
- **Compliance endpoints**: Audit log viewing, telemetry bundle export, compliance reports
- **Viewer endpoints**: System metrics, health checks, adapter listing (read-only)

### Role Check Distribution
- `require_role(Role::Admin)`: 9 endpoints
- `require_any_role([Admin, Operator])`: 15+ endpoints
- `require_any_role([Admin, Operator, Compliance])`: 5 endpoints
- `require_any_role([Operator])`: 8 endpoints

### Public Endpoints
- `/healthz` - Health check (no auth)
- `/readyz` - Readiness check (no auth)
- `/v1/auth/login` - Authentication (no auth)
- `/v1/meta` - System metadata (no auth)
- `/metrics` - Prometheus metrics (bearer token auth, not JWT)

### Dual Auth Endpoints
- OpenAI-compatible endpoints (`/v1/chat/completions`, `/v1/models`) support API key or JWT

## UI Layer RBAC

### Centralized RBAC Module
- Created `ui/src/lib/rbac.ts` with helper functions for role checks
- Functions: `hasRole()`, `hasAnyRole()`, `hasRoleLevel()`, `canAdmin()`, `canOperate()`, etc.
- Role names and descriptions provided for UI display

### Current UI Checks
- Tenant management page checks for Admin role before allowing modifications
- Ad-hoc role checks exist but should be migrated to centralized helpers

### Recommendations
1. Migrate all UI role checks to use `ui/src/lib/rbac.ts` helpers
2. Add role-based route guards for protected pages
3. Show role-specific navigation items based on user permissions
4. Display role badges in user profile/components

## CLI Layer RBAC

### Current Implementation
- CLI uses API client with JWT authentication
- Bootstrap admin command creates Admin users
- No explicit role checks in CLI commands (relies on server-side enforcement)

### Recommendations
1. Add role checks to CLI commands that modify system state
2. Display user role in CLI status output
3. Validate role requirements before attempting operations

## Server-Side Enforcement

### Middleware Stack
1. `auth_middleware` - Validates JWT and extracts claims
2. `per_tenant_rate_limit_middleware` - Rate limiting per tenant
3. Handler-level role checks via `require_role()` / `require_any_role()`

### Error Handling
- Unauthorized requests return `401 Unauthorized`
- Forbidden requests return `403 Forbidden` with role requirement details
- Error messages include required role information

## Validation Status

✅ API endpoints: All protected routes use auth middleware  
✅ Role checks: Centralized helpers (`require_role`, `require_any_role`)  
✅ Role definitions: Four roles with clear hierarchy  
✅ UI helpers: Centralized RBAC module created  
⚠️ UI migration: Some ad-hoc checks need migration to helpers  
⚠️ CLI coverage: CLI relies on server-side enforcement (acceptable)  

## Recommendations

1. **UI Migration**: Update all UI components to use `ui/src/lib/rbac.ts` helpers
2. **Route Guards**: Add React route guards for role-based page access
3. **Role Display**: Show user role consistently across UI components
4. **CLI Enhancement**: Add role validation messages in CLI commands
5. **Audit Logging**: Log all role-check failures for security monitoring
6. **Role Permissions Matrix**: Document complete permissions matrix for each role

