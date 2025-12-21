# Authentication Architecture

## Overview

AdapterOS currently uses short-lived JWT access tokens plus refresh/session cookies. The authoritative behavior comes from:
- `crates/adapteros-server-api/src/handlers/auth_enhanced.rs` (login, refresh, dev bypass/bootstrap)
- `crates/adapteros-server-api/src/middleware/mod.rs` (auth/dual/optional auth, tenant guard, CSRF)
- `crates/adapteros-server-api/src/auth_common.rs` (token TTLs, cookie attributes, dev login gating)
- `crates/adapteros-server-api/src/security/mod.rs` (tenant isolation, lockouts, revocation baseline)
- UI client: `ui/src/api/client.ts` (in-memory bearer token, cookie-based refresh with `credentials: 'include'`)

## Architecture

### Authentication Modes (current behavior)

- **Production mode flag**: `server.production_mode` controls HTTPS/cookie defaults. In production mode HMAC/HS256 is rejected; EdDSA keys are required (`auth_middleware`).
- **Standard auth**: JWT Bearer tokens validated from `Authorization: Bearer`, `token` query param, or `auth_token` cookie. Validation re-checks the session (SQL or KV), tenant token baseline, and token revocation.
- **Dev no-auth bypass**: `AOS_DEV_NO_AUTH=1` works only in debug builds; release builds ignore it. Injects synthetic admin claims with `admin_tenants=["*"]` and `tenant_id="system"`.
- **Dev login endpoint**: `/v1/auth/dev-bypass` is compiled only with `--features dev-bypass` **and** `debug_assertions`, and still requires `security.dev_login_enabled=true`. It mints an admin JWT for tenant `default`, ensures the dev user exists, and sets cookies/CSRF like a normal login.
- **Bootstrap**: `/v1/auth/bootstrap` creates the first admin only when no users exist; it is public but guarded by user-count check.
- **API keys**: `Authorization: ApiKey <token>` hashes to a stored record; roles/scopes drive `Claims.role/roles`, and tenant mismatch is rejected.

## JWT Token Management

### Token Structure

JWT tokens contain the following claims:
- `sub`: User ID
- `email`: User email address
- `role`: Primary role (Admin, Operator, User, etc.)
- `roles`: Role list (primary first)
- `tenant_id`: Tenant identifier (required for all tokens)
- `admin_tenants`: Tenant allowlist for admins (empty = own tenant only; `"*"` only appears in dev bypass/debug)
- `session_id`: Session/JTI identifier (set on access tokens)
- `device_id`: Optional device binding (refresh + access)
- `exp`: Expiration timestamp
- `iat`: Issued at timestamp
- `jti`: JWT ID for tracking and revocation
- `nbf`: Not before timestamp
- `iss`: Must be `adapteros-server`

**Citation**: `crates/adapteros-server-api/src/auth.rs`

### Token Lifecycle

1. **Login** (`POST /v1/auth/login`)
   - Returns a JSON body with `token`, plus `auth_token` (HttpOnly), `refresh_token` (HttpOnly), and `csrf_token` cookies.
   - Access token TTL: `security.access_token_ttl_seconds` (default 15 minutes). Session/refresh TTL: `auth.session_lifetime` or `security.session_ttl_seconds` (default 2 hours).
   - MFA (TOTP/backup code) enforced when enabled on the user.

2. **Token Usage**
   - `Authorization: Bearer <token>` or `auth_token` cookie; query param `token` is also accepted.
   - Every request validates the token signature, `iss`, `tenant_id`, session (SQL or KV), tenant token baseline, and revocation list.
   - Unsafe methods with an auth cookie require `X-CSRF-Token` matching the `csrf_token` cookie (double-submit).

3. **Token Refresh** (`POST /v1/auth/refresh`)
   - Public route but requires a valid `refresh_token` cookie; performs refresh-token validation, session lookup, revocation, and tenant baseline checks.
   - Issues a new access token (body + `auth_token` cookie), rotates `refresh_token`, and sets a new `csrf_token`.
   - UI client performs one silent refresh on 401 before surfacing expiry (`ApiClient.performRefresh`).

4. **Logout** (`POST /v1/auth/logout`)
   - Clears auth/refresh/CSRF cookies. Tokens remain valid until expiry or explicit revocation; session revocation can be done via `/v1/auth/sessions/{jti}` or the revocation list.

### Token Expiry (current defaults)

- Access tokens: 15 minutes (`security.access_token_ttl_seconds`, legacy `security.token_ttl_seconds` fallback).
- Refresh/session cookies: 2 hours (`auth.session_lifetime` or `security.session_ttl_seconds`, defaulting to `DEFAULT_SESSION_TTL_SECS`).
- Tenant baselines: tokens with `iat` before `tenants.token_issued_at_min` are rejected.
- Revocation: `jti` checked against the revocation list on every authenticated request and during refresh.

## Frontend Integration

### API Client

The frontend API client (`ui/src/api/client.ts`) currently:

1. Keeps the bearer token in memory (not localStorage); refreshed tokens update the in-memory value.
2. Uses `credentials: 'include'` so auth/refresh/CSRF cookies are sent; unsafe requests with cookies must set `X-CSRF-Token` to the cookie value.
3. On 401, performs a single silent refresh via `/v1/auth/refresh`; if that fails and a bearer token was present, it calls `markSessionExpired()`.
4. Exposes `devBypass()` which calls `/v1/auth/dev-bypass`; success requires the backend to be compiled with `dev-bypass`, running in debug, and `security.dev_login_enabled=true`.

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

Frontend knobs:
- `VITE_API_URL` sets the base path (defaults to `/api`).
- Dev bypass UI button simply calls `/v1/auth/dev-bypass`; it only works when the backend is compiled with `dev-bypass`, running in debug, and `security.dev_login_enabled=true`.
- Cookies are always sent (`credentials: 'include'`); there is no localStorage token cache.
- See `docs/DEV_BYPASS_POLICY.md` for the UI banner matrix; backend gating still controls success.

## Security Considerations

### Production Checklist

- [ ] `server.production_mode = true`; `security.jwt_mode = "eddsa"` (HMAC disabled in production paths).
- [ ] `security.dev_login_enabled = false`; do not build with `dev-bypass` in production.
- [ ] HTTPS + Secure cookies enabled; SameSite set appropriately for your deployment.
- [ ] Configure CORS and rate limiting; monitor auth attempts and revocations.
- [ ] Keep access tokens short-lived (15m default); set session TTL appropriately.

### Token Security

1. **Storage**: Access token kept in memory; cookies are HttpOnly (CSRF cookie is not) with SameSite and Secure defaults.
2. **Transmission**: HTTPS required in production; SameSite=None forces Secure cookies.
3. **Expiration**: Access ~15m, refresh/session ~2h by default; client refreshes once on 401.
4. **Revocation & baselines**: `session_id`/`jti` checked for revocation and tenant baselines on every request and during refresh.

## Error Handling

### Authentication Errors

Current responses include (see middleware + `auth_enhanced` handlers):
- `UNAUTHORIZED` for missing/invalid Authorization or missing session
- `SESSION_EXPIRED` for expired/locked/missing sessions or invalid refresh
- `TOKEN_REVOKED` when `jti` is revoked
- `TENANT_ISOLATION_ERROR` for cross-tenant violations
- `DEV_BYPASS_DISABLED` when calling `/v1/auth/dev-bypass` without backend gating enabled
- `USER_NOT_FOUND` when a JWT refers to a deleted user (`/v1/auth/me`)

### Error Recovery

1. **401 Unauthorized**: Automatic token refresh attempt
2. **403 Forbidden**: Permission denied (no recovery)
3. **429 Too Many Requests**: Rate limit exceeded (wait)

**Citation**: `ui/src/api/client.ts` L278-298

## API Endpoints

### Public Endpoints (No Auth Middleware)

- `POST /v1/auth/login`
- `POST /v1/auth/refresh` (requires valid `refresh_token` cookie)
- `POST /v1/auth/bootstrap` (only when no users exist)
- `GET /v1/auth/config`
- `GET /v1/auth/health`
- `GET /v1/meta`, `/healthz`, `/readyz`, `/system/ready`
- Dev-only (debug + `dev-bypass` feature): `POST /v1/auth/dev-bypass`, `POST /v1/dev/bootstrap`

### Protected Endpoints (Auth Middleware)

- `POST /v1/auth/logout`, `GET /v1/auth/me`
- MFA: `/v1/auth/mfa/status|start|verify|disable`
- Sessions: `GET /v1/auth/sessions`, `DELETE /v1/auth/sessions/{jti}`, `GET /v1/auth/tenants`, `POST /v1/auth/tenants/switch`
- API keys: `/v1/api-keys`, `/v1/api-keys/{id}`
- Tenant-scoped resources (`/v1/tenants/...`) also pass `tenant_route_guard_middleware` and `validate_tenant_isolation`.

## Dev-only shortcuts and hardening

- `AOS_DEV_NO_AUTH`: Debug builds only; release builds ignore it. Injects admin claims with `admin_tenants=["*"]`, `tenant_id="system"`.
- `/v1/auth/dev-bypass`: Debug + `dev-bypass` feature + `security.dev_login_enabled=true` required. Issues an admin token for tenant `default`, ensures the dev user exists, and sets auth/refresh/CSRF cookies. Returns `DEV_BYPASS_DISABLED` otherwise.
- Tenant isolation: `/tenants/{id}` routes use `tenant_route_guard_middleware` and `validate_tenant_isolation` (admin_tenants allowlist; wildcard only for dev bypass/no-auth).

## Development Workflow

1. Start the control plane (debug build) with config keys set; use `AOS_DEV_NO_AUTH=1` only for debugging.
2. UI dev server: `cd ui && pnpm dev` (uses `/api` by default, sends cookies).
3. Authenticate by:
   - Standard login: `curl -X POST http://localhost:8080/v1/auth/login ...` (returns bearer + cookies).
   - Dev bypass (only when compiled + enabled as above): `curl -X POST http://localhost:8080/v1/auth/dev-bypass`.
4. Test refresh: `curl -X POST http://localhost:8080/v1/auth/refresh --cookie "refresh_token=...; csrf_token=..." -H "X-CSRF-Token: <csrf_token>"`.

## Troubleshooting

- **401s in UI**: Ensure cookies are present; clear cookies (not localStorage) and re-login. If compiled without `dev-bypass`, the dev bypass button will fail with `DEV_BYPASS_DISABLED`.
- **Token expired**: Check `security.access_token_ttl_seconds` and `security.session_ttl_seconds`; verify system clock and that refresh requests include cookies + CSRF header.
- **Cross-tenant denial**: Confirm `admin_tenants` includes the target tenant or wildcard (debug-only).

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

- `AGENTS.md`: Security and authentication patterns
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

## Status Notes

- Role-based access and tenant isolation are enforced in middleware/handlers as described above.
- Token rotation and API-key management exist in the backend; a dedicated UI for rotation/session management is not yet present and should be treated as backlog.

---

## Performance Characteristics

### Performance Benchmarks

#### Token Refresh Performance
- **Endpoint**: `POST /v1/auth/refresh`
- **Expected Performance**:
  - Average response time: < 500ms
  - P95 response time: < 750ms
  - Throughput: > 100 requests/second (single client)

#### Session Management
- **Endpoint**: `GET /v1/auth/sessions`
- **Expected Performance**:
  - Average response time: < 150ms
  - P95 response time: < 300ms

### Performance Testing

```bash
# Run auth performance tests
cargo test --features extended-tests test_auth_performance_characteristics -- --nocapture
```

### Optimization Opportunities

- **JWT Processing**: HMAC-SHA256 validation is computationally inexpensive; Ed25519 has higher CPU cost but better security
- **Database Operations**: User lookup by email should use indexed queries
- **Middleware Overhead**: Authentication middleware runs on every protected request; consider short-lived token caching

### Security vs Performance Trade-offs

- **Session-validated JWTs**: Tokens are short-lived and still check session + revocation + tenant baselines, adding DB/KV lookups per request.
- **Token Refresh Strategy**: Short-lived access tokens reduce exposure; refresh requires session lookup and rotation.

### Monitoring and Alerting

**Key Performance Indicators (KPIs):**
1. Authentication Success Rate: >99.9%
2. Average Token Refresh Time: <500ms
3. P95 Token Validation Time: <200ms
4. Failed Authentication Rate: <0.1%

**Alert Thresholds:**
- Token refresh >1s (warning)
- Token refresh >5s (critical)
- Authentication failure rate >1% (warning)
- Authentication failure rate >5% (critical)

MLNavigator Inc 2025-12-10.
