# Authentication Architecture

## Overview

AdapterOS uses a comprehensive JWT-based authentication system with environment-specific configurations to support secure production deployments while maintaining developer convenience.

**Citations:**
- `crates/adapteros-server-api/src/state.rs` L151-242: AuthMode and AuthConfig structures
- `crates/adapteros-server-api/src/middleware.rs` L1-272: Environment-aware middleware
- `crates/adapteros-server-api/src/errors.rs` L13-88: Authentication error types

## Architecture

### Authentication Modes

The system supports three authentication modes:

#### Development Mode
- **Purpose**: Local development and testing
- **Features**:
  - Accepts development tokens (e.g., `adapteros-local`)
  - Lenient validation with warning logs
  - Auto-login capabilities
- **Configuration**: `auth.mode = "development"` in `configs/cp.toml`
- **Security**: Should NEVER be used in production

**Citation**: `crates/adapteros-server-api/src/state.rs` L154-160

#### Production Mode
- **Purpose**: Live deployment environments
- **Features**:
  - Strict JWT validation only
  - No development tokens accepted
  - Comprehensive security logging
- **Configuration**: `auth.mode = "production"` in `configs/cp.toml`
- **Security**: Full authentication required for all requests

**Citation**: `crates/adapteros-server-api/src/middleware.rs` L48-67

#### Mixed Mode
- **Purpose**: Staging and testing environments
- **Features**:
  - Supports both development and production tokens
  - Flexible validation strategies
- **Configuration**: `auth.mode = "mixed"` in `configs/cp.toml`
- **Use Case**: Pre-production validation

**Citation**: `crates/adapteros-server-api/src/middleware.rs` L104-133

## JWT Token Management

### Token Structure

JWT tokens contain the following claims:
- `sub`: User ID
- `email`: User email address
- `role`: User role (Admin, Operator, User, etc.)
- `tenant_id`: Tenant identifier
- `exp`: Expiration timestamp
- `iat`: Issued at timestamp
- `jti`: JWT ID for tracking and revocation
- `nbf`: Not before timestamp

**Citation**: `crates/adapteros-server-api/src/auth.rs` L12-23

### Token Lifecycle

1. **Login**: User authenticates with credentials
   - Endpoint: `POST /v1/auth/login`
   - Returns: JWT token + user information
   - **Citation**: `crates/adapteros-server-api/src/handlers.rs` L481-568

2. **Token Usage**: Token sent in Authorization header
   - Format: `Authorization: Bearer <token>`
   - Validated on each request
   - **Citation**: `crates/adapteros-server-api/src/middleware.rs` L135-154

3. **Token Refresh**: Automatically refresh before expiry
   - Endpoint: `POST /v1/auth/refresh`
   - Triggers: Less than 1 hour until expiry
   - **Citation**: `crates/adapteros-server-api/src/handlers.rs` L2247-2310
   - **Citation**: `ui/src/api/client.ts` L125-137

4. **Logout**: Client-side token removal
   - Endpoint: `POST /v1/auth/logout`
   - Stateless JWT (no server tracking)
   - **Citation**: `crates/adapteros-server-api/src/handlers.rs` L2219-2225

### Token Expiry

- **Default Expiry**: 8 hours
- **Configurable**: `auth.token_expiry_hours` in configuration
- **Auto-Refresh**: Triggers when < 1 hour remaining
- **Grace Period**: 1 hour refresh window

**Citations**:
- `crates/adapteros-server-api/src/state.rs` L189-190
- `crates/adapteros-server-api/src/auth.rs` L175-179

## Frontend Integration

### API Client

The frontend API client (`ui/src/api/client.ts`) provides:

1. **Secure Token Storage**
   - localStorage with validation
   - Development fallback for local testing
   - **Citation**: `ui/src/api/client.ts` L38-60

2. **Automatic Token Refresh**
   - Checks every 5 minutes
   - Refreshes when < 1 hour remaining
   - **Citation**: `ui/src/api/client.ts` L87-102

3. **Request Retry Logic**
   - Intercepts 401 errors
   - Attempts token refresh
   - Retries failed request
   - **Citation**: `ui/src/api/client.ts` L278-298

4. **Token Validation**
   - Checks token structure
   - Validates expiration
   - **Citation**: `ui/src/api/client.ts` L62-85

### Authentication Flow

```
┌─────────┐      ┌───────────┐      ┌─────────┐
│ Browser │      │  API      │      │  Server │
└────┬────┘      └─────┬─────┘      └────┬────┘
     │                 │                  │
     │  1. Login       │                  │
     ├────────────────>│                  │
     │                 │  Validate        │
     │                 ├─────────────────>│
     │                 │                  │
     │                 │  JWT + User      │
     │  JWT + User     │<─────────────────┤
     │<────────────────┤                  │
     │                 │                  │
     │  2. API Request │                  │
     │  (with token)   │                  │
     ├────────────────>│                  │
     │                 │  Validate Token  │
     │                 ├─────────────────>│
     │                 │                  │
     │                 │  Response        │
     │  Response       │<─────────────────┤
     │<────────────────┤                  │
     │                 │                  │
     │  3. 401 Error   │                  │
     │<────────────────┤                  │
     │                 │                  │
     │  4. Refresh     │                  │
     ├────────────────>│                  │
     │                 │  New JWT         │
     │  New JWT        │<─────────────────┤
     │<────────────────┤                  │
     │                 │                  │
     │  5. Retry       │                  │
     │  (with new token)                  │
     ├────────────────>│                  │
     │                 │                  │
     │  Success        │                  │
     │<────────────────┤                  │
```

## Configuration

### Backend Configuration (`configs/cp.toml`)

```toml
[auth]
# Authentication mode: development, production, or mixed
mode = "development"

# Development token for local testing (optional, development/mixed only)
dev_token = "adapteros-local"

# Token expiry in hours (default: 8)
token_expiry_hours = 8

# Maximum login attempts before lockout (default: 5)
max_login_attempts = 5

# Lockout duration in minutes (default: 15)
lockout_duration_minutes = 15

[security]
# Require HTTPS in production (default: false)
require_https = true

# Allowed CORS origins
cors_origins = ["https://app.example.com"]

# Enable rate limiting (default: true)
enable_rate_limiting = true
```

**Citation**: `crates/adapteros-server-api/src/state.rs` L170-242

### Frontend Configuration

The frontend automatically detects the environment and adjusts authentication behavior:

- **Development**: Uses `adapteros-local` token by default
- **Production**: Requires valid JWT tokens from login

**Citation**: `ui/src/api/client.ts` L38-60

## Security Considerations

### Production Checklist

- [ ] Set `auth.mode = "production"` in configuration
- [ ] Remove or disable `auth.dev_token` setting
- [ ] Enable `security.require_https = true`
- [ ] Configure strict `security.cors_origins`
- [ ] Enable `security.enable_rate_limiting`
- [ ] Use strong JWT secrets (HMAC mode) or keypairs (EdDSA mode)
- [ ] Configure appropriate token expiry times
- [ ] Enable security event logging
- [ ] Monitor failed authentication attempts

### Token Security

1. **Storage**: Tokens stored in localStorage (browser)
   - Consider httpOnly cookies for enhanced security
   - Clear tokens on logout

2. **Transmission**: Always use HTTPS in production
   - Prevents token interception
   - Required for security

3. **Expiration**: Tokens auto-expire after configured duration
   - Refresh before expiration
   - Logout required after expiration

4. **Revocation**: Stateless JWT (no server-side tracking)
   - Tokens valid until expiration
   - Consider implementing revocation list for critical cases

**Citations**:
- `crates/adapteros-server-api/src/errors.rs` L13-56
- `ui/src/api/client.ts` L183-197

## Error Handling

### Authentication Errors

The system provides detailed error codes:

- `INVALID_TOKEN`: Token format or signature invalid
- `TOKEN_EXPIRED`: Token has expired
- `REFRESH_FAILED`: Token refresh attempt failed
- `AUTH_REQUIRED`: No authentication provided
- `INSUFFICIENT_PERMISSIONS`: User lacks required role
- `RATE_LIMIT_EXCEEDED`: Too many attempts
- `ACCOUNT_LOCKED`: Account temporarily locked
- `INVALID_CREDENTIALS`: Login credentials incorrect
- `MISSING_AUTH_HEADER`: Authorization header missing
- `INVALID_AUTH_FORMAT`: Authorization format incorrect

**Citation**: `crates/adapteros-server-api/src/errors.rs` L13-88

### Error Recovery

1. **401 Unauthorized**: Automatic token refresh attempt
2. **403 Forbidden**: Permission denied (no recovery)
3. **429 Too Many Requests**: Rate limit exceeded (wait)

**Citation**: `ui/src/api/client.ts` L278-298

## API Endpoints

### Public Endpoints (No Auth Required)

- `POST /v1/auth/login` - User login
- `GET /healthz` - Health check
- `GET /readyz` - Readiness check
- `GET /v1/meta` - API metadata

**Citation**: `crates/adapteros-server-api/src/routes.rs` L212-217

### Protected Endpoints (Auth Required)

#### Authentication Management
- `POST /v1/auth/logout` - User logout
- `GET /v1/auth/me` - Get current user info
- `POST /v1/auth/refresh` - Refresh authentication token
- `POST /v1/auth/logout-all` - Logout from all sessions
- `GET /v1/auth/sessions` - List active sessions
- `DELETE /v1/auth/sessions/{session_id}` - Revoke specific session
- `POST /v1/auth/token/rotate` - Rotate API token
- `GET /v1/auth/token` - Get token metadata
- `PUT /v1/auth/profile` - Update user profile
- `GET /v1/auth/config` - Get authentication configuration
- `PUT /v1/auth/config` - Update authentication configuration

#### Resource Endpoints
- All `/v1/adapters/*` endpoints
- All `/v1/tenants/*` endpoints
- All `/v1/workers/*` endpoints
- (and more)

**Citation**: `crates/adapteros-server-api/src/routes.rs` L256-697

## ServicePanel Authentication Management

AdapterOS includes a comprehensive authentication management interface accessible through the ServicePanel at port 3300. The "Authentication" tab provides administrators with runtime control over authentication settings and session management.

### Authentication Settings

**Production Mode Toggle**
- Enables/disables strict security policies
- Automatically disables development features when enabled
- Requires server restart for some changes

**Development Token Control**
- Toggle development bypass authentication
- Only available when production mode is disabled
- Provides instant access for development workflows

**JWT Configuration Display**
- Shows current JWT signing mode (HMAC/EdDSA)
- Displays token expiry settings
- Requires restart for algorithm changes

### Session Management

**Active Sessions List**
- Displays all current authentication sessions
- Shows session creation time and last activity
- Identifies current session vs other sessions

**Session Actions**
- **Rotate Token**: Generates new JWT with same claims
- **Revoke Session**: Invalidates specific session (client-side)
- **Logout All**: Clears all stored tokens across devices

**Security Status Dashboard**
- Visual indicators for production mode status
- Development token availability
- Token expiry time remaining

### Configuration Management

**Runtime Configuration Updates**
- Toggle production mode (enforces security policies)
- Enable/disable development features
- Automatic validation prevents invalid configurations

**Audit Trail**
- All configuration changes are logged
- Session management actions tracked
- Security events monitored

**Citation**: `ui/src/components/AuthenticationSettings.tsx` L1-456
**Citation**: `ui/src/components/ServicePanel.tsx` L267-451

## Development Workflow

### Local Development

1. Start server in development mode:
   ```bash
   cd /Users/star/Dev/adapter-os
   ./target/debug/adapteros-server --skip-pf-check --config configs/cp.toml
   ```

2. Start UI dev server:
   ```bash
   cd ui
   pnpm dev
   ```

3. Access UI at `http://localhost:3200`
   - Auto-authenticates with dev token
   - No login required

### Testing Authentication

1. **Test Login**:
   ```bash
   curl -X POST http://localhost:8080/api/v1/auth/login \
     -H "Content-Type: application/json" \
     -d '{"email": "admin@example.com", "password": "password"}'
   ```

2. **Test Protected Endpoint**:
   ```bash
   TOKEN="your-jwt-token-here"
   curl http://localhost:8080/api/v1/adapters \
     -H "Authorization: Bearer $TOKEN"
   ```

3. **Test Token Refresh**:
   ```bash
   curl -X POST http://localhost:8080/api/v1/auth/refresh \
     -H "Authorization: Bearer $TOKEN"
   ```

## Troubleshooting

### White Page / 401 Errors

**Problem**: UI shows white page or constant 401 errors  
**Cause**: Missing or invalid authentication token  
**Solution**:
1. Clear localStorage: `localStorage.clear()`
2. Refresh browser
3. Verify server is in development mode

**Citations**:
- `ui/src/api/client.ts` L278-298
- `ui/TROUBLESHOOTING.md`

### Token Expired Errors

**Problem**: Frequent "token expired" errors  
**Cause**: Token expiry too short or refresh not working  
**Solution**:
1. Increase `auth.token_expiry_hours` in configuration
2. Check token refresh logic in browser console
3. Verify server time is synchronized

**Citation**: `ui/src/api/client.ts` L104-123

### Development Token Not Working

**Problem**: `adapteros-local` token rejected  
**Cause**: Server not in development or mixed mode  
**Solution**:
1. Check `auth.mode` in `configs/cp.toml`
2. Verify `auth.dev_token` is configured
3. Restart server after configuration changes

**Citation**: `crates/adapteros-server-api/src/middleware.rs` L69-102

## References

### Code Citations

- **State Management**: `crates/adapteros-server-api/src/state.rs`
- **Middleware**: `crates/adapteros-server-api/src/middleware.rs`
- **Auth Logic**: `crates/adapteros-server-api/src/auth.rs`
- **Error Handling**: `crates/adapteros-server-api/src/errors.rs`
- **API Handlers**: `crates/adapteros-server-api/src/handlers.rs`
- **Routes**: `crates/adapteros-server-api/src/routes.rs`
- **Frontend Client**: `ui/src/api/client.ts`

### Design Documents

- `CLAUDE.md`: Security and authentication patterns
- `CONTRIBUTING.md`: Development guidelines
- Policy Pack #1 (Egress): Network security policies
- Policy Pack #9 (Telemetry): Event logging requirements

## Future Enhancements

1. **OAuth2/OIDC Support**: External identity providers
2. **Multi-Factor Authentication**: Enhanced security
3. **Token Revocation List**: Centralized token blacklist with database persistence
4. **Advanced Session Management**: Cross-device session tracking with geolocation
5. **API Key Management**: Long-lived service tokens with granular permissions
6. **Enhanced Audit Logging**: Comprehensive security event tracking with alerting
7. **Federated Authentication**: SAML and enterprise SSO integration

## Recently Implemented Features

1. **✅ Session Management**: Client-side session tracking and management UI
2. **✅ Role-Based Access Control**: Multi-role system (Admin/Operator/Compliance/Viewer)
3. **✅ Token Rotation**: API token rotation with new JTI generation
4. **✅ Authentication Configuration UI**: ServicePanel authentication management interface
5. **✅ Production Mode Gating**: Runtime security policy enforcement

**Note**: These enhancements should be implemented with careful consideration of the stateless JWT design and performance implications.

