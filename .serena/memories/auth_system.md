# AdapterOS Authentication System

## Overview

AdapterOS implements a comprehensive authentication system with JWT tokens, session management, API key authentication, and a development bypass mode. The system is designed for multi-tenant environments with strict security controls.

## Key Files

- `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/auth.rs` - Core JWT token generation and validation
- `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/auth_common.rs` - Shared auth utilities, cookie management
- `/Users/star/Dev/adapter-os/crates/adapteros-auth/src/lib.rs` - Centralized auth configuration crate
- `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers/auth.rs` - `/v1/auth/me` endpoint
- `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers/auth_enhanced/` - Full auth handlers
- `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers/api_keys.rs` - API key management
- `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/security/token_revocation.rs` - Token blacklist
- `/Users/star/Dev/adapter-os/crates/adapteros-db/src/auth_sessions.rs` - Session storage

## Authentication Modes

```rust
pub enum AuthMode {
    BearerToken,    // JWT in Authorization header or query param
    Cookie,         // JWT from auth_token cookie
    ApiKey,         // Hashed API key validated against database
    DevBypass,      // Debug builds only (AOS_DEV_NO_AUTH=1)
    Unauthenticated // Public endpoints
}
```

## JWT Token Handling

### Algorithms Supported

1. **Ed25519 (EdDSA)** - Asymmetric, required for production
2. **HMAC-SHA256 (HS256)** - Symmetric, for development/fallback

### Key Selection

- Tokens include a `kid` (key ID) in the header
- Key ID derived from public key/secret using BLAKE3 hash (truncated to 16 bytes)
- Supports key rotation via `(kid, key)` tuples for verification

### Token Types

**Access Token (Claims/AccessClaims)**
- Short-lived: Default 15 minutes (`DEFAULT_ACCESS_TOKEN_TTL_SECS`)
- Contains: sub, email, role, roles[], tenant_id, admin_tenants[], device_id, session_id, mfa_level, jti, exp, iat, nbf, iss
- JTI (JWT ID) aligned with session_id for revocation

**Refresh Token (RefreshClaims)**
- Longer-lived: Default 2 hours (`DEFAULT_SESSION_TTL_SECS`)
- Contains: sub, tenant_id, roles[], device_id, session_id, rot_id, exp, iat, iss
- rot_id used for rotation detection (prevents replay)

### Token Generation Functions

```rust
// Ed25519 signing
issue_access_token_ed25519(user_id, email, role, roles, tenant_id, admin_tenants, device_id, session_id, mfa_level, keypair, ttl)
issue_refresh_token_ed25519(user_id, tenant_id, roles, device_id, session_id, rot_id, keypair, ttl)

// HMAC signing (dev/fallback)
issue_access_token_hmac(..., secret, ttl)
issue_refresh_token_hmac(..., secret, ttl)

// Validation
validate_access_token_ed25519(token, keys, fallback_pem)
validate_refresh_token_ed25519(token, keys, fallback_pem)
validate_token(token, keys, fallback_secret) // HMAC
```

### Security Properties

- **Issuer validation**: Must match `JWT_ISSUER` ("adapteros-server")
- **NBF validation**: Token "not before" timestamp checked
- **Clock skew tolerance**: 60 seconds leeway
- **JTI uniqueness**: Generated using BLAKE3(user_id + timestamp + tenant_id + UUIDv7 nonce)

## Password Handling

- **Algorithm**: Argon2id (64 MiB memory, 3 iterations, parallelism 1)
- **Legacy support**: Bcrypt with automatic upgrade on successful login
- **Timing-safe**: Constant-time comparison via `subtle` crate
- **Timing-consistent failures**: Executes hardened hash on failures to prevent timing oracles

```rust
pub fn verify_password(password: &str, hash: &str) -> Result<PasswordVerification>
// Returns: { valid: bool, needs_rehash: bool }
```

## Session Management

### Session Storage (`auth_sessions` table)

```rust
pub struct AuthSession {
    jti: String,           // JWT ID (primary key)
    session_id: Option<String>,
    user_id: String,
    tenant_id: String,
    device_id: Option<String>,
    rot_id: Option<String>,    // Rotation ID for refresh token replay detection
    refresh_hash: Option<String>, // BLAKE3 hash of refresh token
    ip_address: Option<String>,
    user_agent: Option<String>,
    created_at: String,
    last_activity: String,
    expires_at: i64,
    locked: bool
}
```

### Session Operations

- `create_auth_session()` - Create new session on login
- `delete_auth_session(jti)` - Delete session (logout/revoke)
- `get_user_sessions(user_id)` - List active sessions
- `update_auth_session_activity(jti)` - Touch last_activity
- `cleanup_expired_sessions()` - Background cleanup

### Refresh Token Rotation

1. Each refresh token has a `rot_id` (rotation ID)
2. On refresh, server checks `rot_id` matches stored session
3. New `rot_id` generated, old one invalidated
4. Mismatched `rot_id` = potential token replay attack

## API Key Authentication

### Key Generation

```rust
fn generate_token() -> (String, String) {
    // 32 random bytes -> Base64 URL-safe encoded
    // Hash: BLAKE3(token) -> stored in database
}
```

### Key Management Endpoints

- `POST /v1/api-keys` - Create API key (returns plaintext token ONCE)
- `GET /v1/api-keys` - List keys (tenant-scoped)
- `DELETE /v1/api-keys/{id}` - Revoke key

### Scope Enforcement

- Non-admin callers can only mint keys with their own role scope
- Admins can mint any scope
- Scope escalation attempts are logged and blocked

## Token Revocation

### Revocation Blacklist (`revoked_tokens` table)

```rust
pub struct RevokedToken {
    jti: String,
    user_id: String,
    tenant_id: String,
    revoked_at: String,
    revoked_by: Option<String>,
    reason: Option<String>,
    expires_at: String
}
```

### Revocation Functions

- `is_token_revoked(db, jti)` - Check if token is blacklisted
- `revoke_token(db, jti, user_id, tenant_id, expires_at, revoked_by, reason)` - Add to blacklist
- `revoke_all_user_tokens(db, user_id, tenant_id, revoked_by, reason)` - Mass revocation
- `cleanup_expired_revocations(db)` - Remove expired entries

## Dev Bypass Mode

### Activation Requirements

1. Debug build (`debug_assertions`)
2. `dev-bypass` feature flag enabled
3. Environment variable: `AOS_DEV_NO_AUTH=1` OR config `security.dev_bypass=true`

### Behavior

- **Release builds**: Always disabled, env var ignored with error log
- **Debug builds**: Creates synthetic admin claims with wildcard tenant access

### Dev Bypass Handler (`/v1/auth/dev-bypass`)

1. Creates/ensures "default" tenant
2. Creates/ensures dev admin user
3. Creates default workspace
4. Issues proper JWT token (not just synthetic claims)
5. Sets session in database

### Dev Bootstrap Handler (`/v1/dev/bootstrap`)

- Creates "system" tenant
- Creates admin user with provided email/password
- Grants admin access to system tenant
- Returns JWT token

## Cookie Configuration

```rust
pub struct CookieConfig {
    same_site: String,    // "Strict", "Lax", "None"
    secure: bool,         // Requires HTTPS
    http_only: bool,      // Always true for security
    domain: Option<String>,
    path: String          // Default "/"
}
```

### Cookie Names

- `auth_token` - Access token (HttpOnly)
- `refresh_token` - Refresh token (HttpOnly)
- `csrf_token` - CSRF protection (NOT HttpOnly, for JS access)

### Production Requirements

- `SameSite=None` requires `Secure` flag
- Default secrets rejected in release builds

## Public Paths (No Auth Required)

```rust
const PUBLIC_PATHS: &[&str] = &[
    "/healthz", "/readyz", "/livez", "/version",
    "/metrics", "/v1/metrics",
    "/v1/auth/login", "/v1/auth/register", "/v1/auth/refresh",
    "/v1/auth/config", "/v1/auth/health", "/v1/auth/bootstrap",
    "/v1/auth/dev-bypass", "/v1/dev/bootstrap",
    "/swagger-ui", "/api-doc", "/openapi.json",
    "/static", "/assets", "/favicon.ico"
];
```

## Login Flow

1. Normalize email
2. Check login lockout (FAIL-CLOSED on DB error)
3. Fetch user by email
4. Timing-safe password verification (dummy hash if user not found)
5. Check if account disabled
6. Generate session_id and rot_id
7. Issue access token
8. Issue refresh token
9. Create session in database
10. Track successful auth attempt
11. Update last_login timestamp
12. Attach HttpOnly cookies
13. Return LoginResponse with token

## Logout Flow

1. Extract session_id from claims
2. Delete session from database
3. Add JTI to revoked tokens blacklist
4. Clear auth cookies (set Max-Age=0)

## Security Invariants (Boot-Time Validation)

- SEC-001: Dev bypass must not be active in release builds
- AUTH-001: JWT mode requires key configuration (EdDSA needs key path, HS256 needs secret)
- AUTH-002: HMAC secret must not be a default value in production
- CFG-002: Access token TTL should be shorter than session TTL
- SEC-005: SameSite=None requires Secure flag in production

## Multi-Tenant Access

- Users have a primary `tenant_id`
- Admin users can access multiple tenants via `admin_tenants[]` claim
- Wildcard `"*"` grants access to all tenants (dev bypass)
- Tenant access grants stored in `user_tenant_access` table
