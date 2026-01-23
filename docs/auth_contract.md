# Auth Contract

**Last Updated**: 2026-01-21
**Status**: Step 0 Complete - Issues Identified

## Authentication Methods

### Primary: httpOnly Cookies (Browser Clients)

| Cookie | Purpose | TTL | HttpOnly | SameSite |
|--------|---------|-----|----------|----------|
| `auth_token` | Access token (JWT) | 15 min | Yes | Lax/Strict |
| `refresh_token` | Session token | 2 hours | Yes | Lax |
| `csrf_token` | CSRF protection | 2 hours | **No** | Lax |

### Secondary: Authorization Header (API Clients)

- `Authorization: Bearer <JWT>`
- `Authorization: ApiKey <api-key>`

### Token Extraction Priority (Server Middleware)

1. Authorization header (Bearer or ApiKey)
2. Query parameter `?token=<jwt>`
3. Cookie (`auth_token`)

---

## Request Shapes

### Login: `POST /v1/auth/login`

**Request Body:**
```json
{
  "email": "user@example.com",
  "password": "password123"
}
```

**Response (200):**
```json
{
  "schema_version": "1.0",
  "token": "<JWT>",
  "user_id": "user-123",
  "tenant_id": "tenant-xyz",
  "role": "admin",
  "expires_in": 900,
  "tenants": [{ "id": "...", "name": "..." }],
  "mfa_level": null
}
```

**Cookies Set:** See [CRITICAL BUG #1](#critical-bug-1-login-handler-does-not-set-cookies)

---

### Refresh: `POST /v1/auth/refresh`

**Request:** No body. Uses `refresh_token` cookie.

**Response (200):**
```json
{
  "schema_version": "1.0",
  "token": "<NEW_JWT>",
  "user_id": "user-123",
  "tenant_id": "tenant-xyz",
  "role": "admin",
  "expires_in": 900,
  "tenants": null,
  "mfa_level": null
}
```

**Cookies Set:** `auth_token` (new access token)

---

### Protected Endpoint Example: `GET /v1/auth/me`

**Request:**
- Cookies automatically included via `RequestCredentials::Include`
- OR `Authorization: Bearer <JWT>`

**Response (200):**
```json
{
  "schema_version": "1.0",
  "user_id": "user-123",
  "email": "user@example.com",
  "role": "admin",
  "created_at": "2024-01-01T00:00:00Z",
  "tenant_id": "tenant-xyz",
  "display_name": "User Name",
  "permissions": ["read", "write", "admin"],
  "admin_tenants": ["tenant-xyz"],
  "last_login_at": "2026-01-21T12:00:00Z",
  "mfa_enabled": false,
  "token_last_rotated_at": null
}
```

---

### Logout: `POST /v1/auth/logout`

**Request:** Requires valid auth token.

**Response:** `200 OK`

**Server Action:** Deletes session from database.

**Cookies Cleared:** See [CRITICAL BUG #2](#critical-bug-2-logout-does-not-clear-cookies)

---

## UI-Side Auth Flow

1. UI calls `POST /v1/auth/login` with credentials
2. Server validates, creates session, returns JWT
3. UI stores auth status in memory (`AuthState::Authenticated`)
4. UI includes credentials on all requests via `RequestCredentials::Include`
5. Access token refresh via `POST /v1/auth/refresh` using refresh cookie
6. Logout calls `POST /v1/auth/logout`, clears local state

---

## CRITICAL BUGS IDENTIFIED

### Critical Bug #1: Login Handler Does Not Set Cookies

**File:** `crates/adapteros-server-api/src/handlers/auth_enhanced/login.rs:190`

**Issue:** The login handler returns only `Json<LoginResponse>`, NOT the cookies required for httpOnly auth:

```rust
// CURRENT (BROKEN):
Ok(Json(LoginResponse { ... }))

// EXPECTED (like dev_bypass.rs:694):
Ok((response_headers, Json(LoginResponse { ... })))
```

**Impact:** Browser auth relies on cookies, but login doesn't set them. Only the token in the response body is available, requiring UI to store it manually (defeating httpOnly security).

---

### Critical Bug #2: Logout Does Not Clear Cookies

**File:** `crates/adapteros-server-api/src/handlers/auth_enhanced/sessions.rs:124-139`

**Issue:** The logout handler only deletes the session from the database. It does NOT call `clear_auth_cookies()` to send Set-Cookie headers that clear the browser cookies.

```rust
// CURRENT:
Ok(StatusCode::OK)

// EXPECTED:
let mut headers = HeaderMap::new();
clear_auth_cookies(&mut headers, &cfg)?;
Ok((headers, StatusCode::OK))
```

**Impact:** After logout, cookies remain in the browser. If the session is re-created or the token is still valid, the user appears logged in.

---

### Critical Bug #3: Mock User Fallback on Network Error (Reference Mode Risk)

**File:** `crates/adapteros-ui/src/signals/auth.rs:238-241`

**Issue:** On localhost, any network error causes silent fallback to a mock user:

```rust
Err(ApiError::Network(msg)) if self.allow_mock_on_failure => {
    boot_log("auth", &format!("network error, using mock user: {}", msg));
    self.state.set(AuthState::Authenticated(Box::new(mock_dev_user())));
}
```

**Trigger:** `allow_mock_on_failure = is_dev_localhost()` (line 260)

**Impact:** In reference mode on localhost, if the server is slow or times out, the UI silently authenticates as a fake admin user. This is a reference-breaking security issue.

---

### Critical Bug #4: Dev Bypass Auto-Login on 401

**File:** `crates/adapteros-ui/src/signals/auth.rs:225-236`

**Issue:** On 401 response, the UI automatically attempts dev bypass login:

```rust
Err(ApiError::Unauthorized) => {
    if let Some(user) = self.try_dev_bypass_login().await {
        self.state.set(AuthState::Authenticated(Box::new(user)));
    } else {
        self.state.set(AuthState::Unauthenticated);
    }
}
```

**Impact:** Expected for dev, but in reference runs this auto-login behavior is undesirable. Should be explicitly disabled in reference mode.

---

## Cookie Configuration (Reference)

**From:** `crates/adapteros-server-api/src/auth_common.rs`

| Setting | Dev Default | Prod Default | Config Key |
|---------|-------------|--------------|------------|
| `cookie_secure` | false | true | `security.cookie_secure` |
| `cookie_same_site` | "Lax" | "Strict" | `security.cookie_same_site` |
| `cookie_domain` | None | None | `security.cookie_domain` |
| `access_token_ttl` | 900 (15m) | 900 (15m) | `security.access_token_ttl_seconds` |
| `session_ttl` | 7200 (2h) | 7200 (2h) | `security.session_ttl_seconds` |

---

## Dev Bypass Conditions

Dev bypass (`/v1/auth/dev-bypass`) requires ALL of:

1. Compile-time: `#[cfg(feature = "dev-bypass")]`
2. Compile-time: `#[cfg(debug_assertions)]`
3. Runtime: `config.security.dev_login_enabled = true`
4. OR runtime: `AOS_DEV_NO_AUTH=1` environment variable

---

## Next Steps

1. **Fix login handler** to set cookies (Bug #1)
2. **Fix logout handler** to clear cookies (Bug #2)
3. **Add REFERENCE_MODE flag** to disable mock user fallback (Bug #3)
4. **Disable auto dev bypass** in reference mode (Bug #4)
5. **Add integration tests** for cookie-based auth flow
