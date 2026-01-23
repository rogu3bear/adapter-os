use crate::auth::{
    generate_token_ed25519_with_admin_tenants_mfa, generate_token_with_admin_tenants_mfa,
    issue_access_token_ed25519, issue_access_token_hmac, issue_refresh_token_ed25519,
    issue_refresh_token_hmac, DEFAULT_SESSION_TTL_SECS,
};
use crate::permissions::{permissions_for_role, Permission};
use crate::state::AppState;
use adapteros_api_types::auth::UserInfoResponse;
use adapteros_api_types::API_SCHEMA_VERSION;
use adapteros_crypto::Keypair;
use adapteros_db::users::{Role, User};
use axum::http::header::InvalidHeaderValue;
use axum::http::{header, HeaderMap, HeaderValue};
use chrono::{DateTime, Duration, Utc};
use std::str::FromStr;

/// Shared authentication configuration derived from AppState settings.
pub struct AuthConfig<'a> {
    pub access_token_ttl_seconds: u64,
    pub session_ttl_seconds: u64,
    pub production_mode: bool,
    pub dev_login_enabled: bool,
    pub use_ed25519: bool,
    pub jwt_kid: String,
    pub ed25519_keypair: &'a Keypair,
    pub jwt_secret: &'a [u8],
    pub cookie_same_site: String,
    pub cookie_domain: Option<String>,
    pub cookie_secure: bool,
    pub clock_skew_seconds: u64,
}

impl<'a> AuthConfig<'a> {
    /// Builds the configuration from the current app state.
    pub fn from_state(state: &'a AppState) -> Self {
        // STABILITY: Use poison-safe lock access
        let config = state.config.read().unwrap_or_else(|e| {
            tracing::warn!("Config lock was poisoned in auth_common, recovering");
            e.into_inner()
        });
        let session_ttl_seconds = if config.auth.session_lifetime > 0 {
            config.auth.session_lifetime
        } else {
            config
                .security
                .session_ttl_seconds
                .unwrap_or(DEFAULT_SESSION_TTL_SECS)
        };
        let access_token_ttl_seconds = config.security.access_token_ttl_seconds.unwrap_or(15 * 60);
        let legacy_ttl = config
            .security
            .token_ttl_seconds
            .unwrap_or(access_token_ttl_seconds);
        let effective_access_ttl = if access_token_ttl_seconds == 0 {
            legacy_ttl
        } else {
            access_token_ttl_seconds
        };
        let cookie_secure = config
            .security
            .cookie_secure
            .unwrap_or(config.server.production_mode);
        let cookie_same_site = config
            .security
            .cookie_same_site
            .clone()
            .unwrap_or_else(|| "Lax".to_string());

        Self {
            access_token_ttl_seconds: effective_access_ttl,
            session_ttl_seconds,
            production_mode: config.server.production_mode,
            dev_login_enabled: config.security.dev_login_enabled,
            use_ed25519: state.use_ed25519,
            ed25519_keypair: &state.ed25519_keypair,
            jwt_secret: state.jwt_secret.as_slice(),
            cookie_same_site,
            cookie_domain: config.security.cookie_domain.clone(),
            cookie_secure,
            jwt_kid: state.jwt_primary_kid.clone(),
            clock_skew_seconds: config.security.clock_skew_seconds,
        }
    }

    /// Effective TTL used for cookies and tokens.
    pub fn effective_ttl(&self) -> u64 {
        self.session_ttl_seconds
    }

    /// Short-lived access token TTL.
    pub fn access_ttl(&self) -> u64 {
        self.access_token_ttl_seconds
    }

    /// Whether dev login is allowed under the current config.
    ///
    /// Uses the same check as the middleware bypass (`dev_no_auth_enabled`).
    /// When true, the UI should auto-skip the login page.
    pub fn dev_login_allowed(&self) -> bool {
        crate::auth::dev_no_auth_enabled()
    }

    /// Cookie expiration instant for the current TTL.
    pub fn cookie_expiration(&self) -> DateTime<Utc> {
        Utc::now() + Duration::seconds(self.effective_ttl() as i64)
    }
}

/// Shared context created for every authenticated user.
#[derive(Debug)]
pub struct AuthContext {
    pub user: User,
    pub tenant_id: String,
    pub display_name: String,
    pub role: Role,
    pub permissions: Vec<Permission>,
    pub mfa_enabled: bool,
    pub password_rotated_at: Option<DateTime<Utc>>,
    pub token_rotated_at: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub admin_tenants: Vec<String>,
}

impl AuthContext {
    /// Creates an auth context from a user row.
    pub fn from_user(user: User) -> Result<Self, AuthError> {
        let role = Role::from_str(&user.role).map_err(|e| AuthError::RoleParse(e.to_string()))?;
        let permissions = permissions_for_role(&role);
        let tenant_id = user.tenant_id.clone();
        let display_name = user.display_name.clone();
        let mfa_enabled = user.mfa_enabled;
        let password_rotated_at = user
            .password_rotated_at
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));
        let token_rotated_at = user
            .token_rotated_at
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));
        let last_login_at = user
            .last_login_at
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        Ok(Self {
            user,
            tenant_id,
            display_name,
            role,
            permissions,
            mfa_enabled,
            password_rotated_at,
            token_rotated_at,
            last_login_at,
            admin_tenants: Vec::new(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("role parsing failed: {0}")]
    RoleParse(String),
    #[error("token generation failed: {0}")]
    Token(#[from] anyhow::Error),
    #[error("cookie serialization failed: {0}")]
    Cookie(#[from] InvalidHeaderValue),
}

/// Builds the JWT according to the provided context and configuration.
pub fn build_auth_token(
    ctx: &AuthContext,
    cfg: &AuthConfig,
    mfa_level: Option<&str>,
) -> Result<String, AuthError> {
    let ttl = cfg.access_ttl();

    if cfg.use_ed25519 {
        generate_token_ed25519_with_admin_tenants_mfa(
            &ctx.user.id,
            &ctx.user.email,
            &ctx.role.to_string(),
            &ctx.tenant_id,
            &ctx.admin_tenants,
            cfg.ed25519_keypair,
            ttl,
            mfa_level,
            Some(cfg.jwt_kid.as_str()),
        )
        .map_err(AuthError::Token)
    } else {
        generate_token_with_admin_tenants_mfa(
            &ctx.user.id,
            &ctx.user.email,
            &ctx.role.to_string(),
            &ctx.tenant_id,
            &ctx.admin_tenants,
            cfg.jwt_secret,
            ttl,
            mfa_level,
            Some(cfg.jwt_kid.as_str()),
        )
        .map_err(AuthError::Token)
    }
}

/// Parameters for access token issuance.
///
/// Encapsulates all inputs needed to issue an access token, allowing the
/// ED25519/HMAC decision to be made internally by [`issue_access_token`].
#[derive(Debug)]
pub struct AccessTokenParams<'a> {
    pub user_id: &'a str,
    pub email: &'a str,
    pub role: &'a str,
    pub roles: &'a [String],
    pub tenant_id: &'a str,
    pub admin_tenants: &'a [String],
    pub device_id: Option<&'a str>,
    pub session_id: &'a str,
    pub mfa_level: Option<&'a str>,
}

/// Parameters for refresh token issuance.
///
/// Encapsulates all inputs needed to issue a refresh token, allowing the
/// ED25519/HMAC decision to be made internally by [`issue_refresh_token`].
#[derive(Debug)]
pub struct RefreshTokenParams<'a> {
    pub user_id: &'a str,
    pub tenant_id: &'a str,
    pub roles: &'a [String],
    pub device_id: Option<&'a str>,
    pub session_id: &'a str,
    pub rot_id: &'a str,
}

/// Issues an access token using the appropriate signing algorithm (ED25519 or HMAC).
///
/// This wrapper encapsulates the ED25519/HMAC dispatch decision, eliminating the
/// need to repeat the conditional logic across handlers.
///
/// # Arguments
///
/// * `state` - Application state containing signing keys and algorithm preference
/// * `params` - Token parameters (user info, session, etc.)
/// * `ttl_seconds` - Optional TTL override; uses config default if None
///
/// # Returns
///
/// The signed JWT access token string.
pub fn issue_access_token(
    state: &AppState,
    params: &AccessTokenParams<'_>,
    ttl_seconds: Option<u64>,
) -> Result<String, AuthError> {
    if state.use_ed25519 {
        issue_access_token_ed25519(
            params.user_id,
            params.email,
            params.role,
            params.roles,
            params.tenant_id,
            params.admin_tenants,
            params.device_id,
            params.session_id,
            params.mfa_level,
            &state.ed25519_keypair,
            ttl_seconds,
        )
        .map_err(AuthError::Token)
    } else {
        issue_access_token_hmac(
            params.user_id,
            params.email,
            params.role,
            params.roles,
            params.tenant_id,
            params.admin_tenants,
            params.device_id,
            params.session_id,
            params.mfa_level,
            &state.jwt_secret,
            ttl_seconds,
        )
        .map_err(AuthError::Token)
    }
}

/// Issues a refresh token using the appropriate signing algorithm (ED25519 or HMAC).
///
/// This wrapper encapsulates the ED25519/HMAC dispatch decision, eliminating the
/// need to repeat the conditional logic across handlers.
///
/// # Arguments
///
/// * `state` - Application state containing signing keys and algorithm preference
/// * `params` - Token parameters (user info, session, rotation ID, etc.)
/// * `ttl_seconds` - Optional TTL override; uses config default if None
///
/// # Returns
///
/// The signed JWT refresh token string.
pub fn issue_refresh_token(
    state: &AppState,
    params: &RefreshTokenParams<'_>,
    ttl_seconds: Option<u64>,
) -> Result<String, AuthError> {
    if state.use_ed25519 {
        issue_refresh_token_ed25519(
            params.user_id,
            params.tenant_id,
            params.roles,
            params.device_id,
            params.session_id,
            params.rot_id,
            &state.ed25519_keypair,
            ttl_seconds,
        )
        .map_err(AuthError::Token)
    } else {
        issue_refresh_token_hmac(
            params.user_id,
            params.tenant_id,
            params.roles,
            params.device_id,
            params.session_id,
            params.rot_id,
            &state.jwt_secret,
            ttl_seconds,
        )
        .map_err(AuthError::Token)
    }
}

/// Issues both access and refresh tokens in a single call.
///
/// This is a convenience wrapper that combines [`issue_access_token`] and
/// [`issue_refresh_token`], useful when both tokens are needed together
/// (e.g., login, registration, token refresh).
///
/// # Returns
///
/// A tuple of (access_token, refresh_token).
pub fn issue_token_pair(
    state: &AppState,
    access_params: &AccessTokenParams<'_>,
    refresh_params: &RefreshTokenParams<'_>,
    access_ttl: Option<u64>,
    refresh_ttl: Option<u64>,
) -> Result<(String, String), AuthError> {
    let access_token = issue_access_token(state, access_params, access_ttl)?;
    let refresh_token = issue_refresh_token(state, refresh_params, refresh_ttl)?;
    Ok((access_token, refresh_token))
}

/// Builds the canonical user info response used by the frontend.
pub fn build_user_info(ctx: &AuthContext) -> UserInfoResponse {
    UserInfoResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        user_id: ctx.user.id.clone(),
        email: ctx.user.email.clone(),
        role: ctx.role.to_string(),
        created_at: ctx.user.created_at.clone(),
        tenant_id: ctx.tenant_id.clone(),
        display_name: ctx.display_name.clone(),
        permissions: ctx.permissions.iter().map(|p| p.to_string()).collect(),
        admin_tenants: ctx.admin_tenants.clone(),
        last_login_at: ctx.last_login_at.map(|dt| dt.to_rfc3339()),
        mfa_enabled: Some(ctx.mfa_enabled),
        token_last_rotated_at: ctx.token_rotated_at.map(|dt| dt.to_rfc3339()),
    }
}

/// Attaches the auth cookie to the provided header map.
pub fn attach_auth_cookie(
    headers: &mut HeaderMap,
    token: &str,
    cfg: &AuthConfig,
) -> Result<(), AuthError> {
    attach_cookie(headers, "auth_token", token, cfg, cfg.access_ttl(), None)
}

/// Attaches the refresh/session cookie to the provided header map.
pub fn attach_refresh_cookie(
    headers: &mut HeaderMap,
    token: &str,
    cfg: &AuthConfig,
) -> Result<(), AuthError> {
    attach_cookie(
        headers,
        "refresh_token",
        token,
        cfg,
        cfg.effective_ttl(),
        Some(&cfg.cookie_same_site),
    )
}

/// Attaches a CSRF cookie (non-HttpOnly) for double-submit protection.
pub fn attach_csrf_cookie(
    headers: &mut HeaderMap,
    token: &str,
    cfg: &AuthConfig,
    max_age: u64,
) -> Result<(), AuthError> {
    let expires = Utc::now() + Duration::seconds(max_age as i64);
    let expires_value = expires.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    let mut secure_required = cfg.cookie_secure;
    let samesite_norm = match cfg.cookie_same_site.to_ascii_lowercase().as_str() {
        "none" => {
            secure_required = true;
            "None".to_string()
        }
        "strict" => "Strict".to_string(),
        _ => "Lax".to_string(),
    };
    let secure_flag = if secure_required { "; Secure" } else { "" };
    let domain = cfg
        .cookie_domain
        .as_ref()
        .map(|d| format!("; Domain={}", d))
        .unwrap_or_default();

    let cookie_value = format!(
        "csrf_token={token}; Path=/; Max-Age={max_age}; Expires={expires}; SameSite={samesite}{secure_flag}{domain}",
        token = token,
        max_age = max_age,
        expires = expires_value,
        samesite = samesite_norm,
        secure_flag = secure_flag,
        domain = domain,
    );
    headers.append(header::SET_COOKIE, HeaderValue::from_str(&cookie_value)?);
    Ok(())
}

/// Attaches all three auth cookies (access, refresh, and CSRF) in one call.
///
/// This is a convenience wrapper that consolidates the common pattern of attaching
/// all authentication cookies during login, refresh, or tenant switch operations.
pub fn attach_auth_cookies(
    headers: &mut HeaderMap,
    access_token: &str,
    refresh_token: &str,
    csrf_token: &str,
    cfg: &AuthConfig,
    session_ttl: u64,
) -> Result<(), AuthError> {
    attach_auth_cookie(headers, access_token, cfg)?;
    attach_refresh_cookie(headers, refresh_token, cfg)?;
    attach_csrf_cookie(headers, csrf_token, cfg, session_ttl)?;
    Ok(())
}

fn attach_cookie(
    headers: &mut HeaderMap,
    name: &str,
    token: &str,
    cfg: &AuthConfig,
    max_age: u64,
    same_site_override: Option<&str>,
) -> Result<(), AuthError> {
    let expires = Utc::now() + Duration::seconds(max_age as i64);
    let expires_value = expires.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    let mut secure_required = cfg.cookie_secure;
    let samesite_raw = same_site_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| cfg.cookie_same_site.clone());
    let samesite_norm = match samesite_raw.to_ascii_lowercase().as_str() {
        "none" => {
            secure_required = true; // Per spec, SameSite=None requires Secure
            "None".to_string()
        }
        "strict" => "Strict".to_string(),
        _ => "Lax".to_string(),
    };
    let secure_flag = if secure_required { "; Secure" } else { "" };
    let domain = cfg
        .cookie_domain
        .as_ref()
        .map(|d| format!("; Domain={}", d))
        .unwrap_or_default();

    let cookie_value = format!(
        "{name}={token}; HttpOnly; Path=/; Max-Age={max_age}; Expires={expires}; SameSite={samesite}{secure_flag}{domain}",
        name = name,
        token = token,
        max_age = max_age,
        expires = expires_value,
        samesite = samesite_norm,
        secure_flag = secure_flag,
        domain = domain,
    );

    let header_value = HeaderValue::from_str(&cookie_value)?;
    headers.append(header::SET_COOKIE, header_value);
    Ok(())
}

/// Append Set-Cookie headers to clear auth and refresh cookies.
pub fn clear_auth_cookies(headers: &mut HeaderMap, cfg: &AuthConfig) -> Result<(), AuthError> {
    // Reuse attach_cookie with Max-Age=0
    attach_cookie(headers, "auth_token", "", cfg, 0, None)?;
    attach_cookie(
        headers,
        "refresh_token",
        "",
        cfg,
        0,
        Some(&cfg.cookie_same_site),
    )?;
    attach_csrf_cookie(headers, "", cfg, 0)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_crypto::Keypair;
    use axum::http::HeaderMap;
    use chrono::Utc;

    fn sample_user() -> User {
        User {
            id: "user-123".to_string(),
            email: "test@adapteros.local".to_string(),
            display_name: "Test User".to_string(),
            pw_hash: "$argon2id".to_string(),
            role: "admin".to_string(),
            disabled: false,
            created_at: Utc::now().to_rfc3339(),
            tenant_id: "default".to_string(),
            failed_attempts: 0,
            last_failed_at: None,
            lockout_until: None,
            mfa_enabled: false,
            mfa_secret_enc: None,
            mfa_backup_codes_json: None,
            mfa_enrolled_at: None,
            mfa_last_verified_at: None,
            mfa_recovery_last_used_at: None,
            password_rotated_at: None,
            token_rotated_at: None,
            last_login_at: None,
        }
    }

    #[test]
    fn build_user_info_includes_permissions() {
        let user = sample_user();
        let ctx = AuthContext::from_user(user).expect("context should build");
        let info = build_user_info(&ctx);
        assert_eq!(info.tenant_id, "default");
        assert!(!info.permissions.is_empty());
    }

    #[test]
    fn attach_auth_cookie_sets_secure_flag_when_production() {
        let keypair = Keypair::generate();
        let jwt_secret = vec![0u8; 32];
        let cfg = AuthConfig {
            access_token_ttl_seconds: 60,
            session_ttl_seconds: 3600,
            production_mode: true,
            dev_login_enabled: false,
            use_ed25519: true,
            jwt_kid: "test-kid".to_string(),
            ed25519_keypair: &keypair,
            jwt_secret: &jwt_secret,
            cookie_same_site: "Strict".to_string(),
            cookie_domain: None,
            cookie_secure: true,
            clock_skew_seconds: 60,
        };

        let ctx = AuthContext::from_user(sample_user()).unwrap();
        let token = build_auth_token(&ctx, &cfg, None).expect("token generated");
        let mut headers = HeaderMap::new();
        attach_auth_cookie(&mut headers, &token, &cfg).expect("cookie attached");
        let cookie = headers
            .get(header::SET_COOKIE)
            .expect("cookie header should exist")
            .to_str()
            .unwrap();
        assert!(
            cookie.contains("Secure"),
            "production mode should set Secure flag"
        );
        assert!(cookie.contains("HttpOnly"));
        assert!(
            cookie.contains("SameSite=Strict"),
            "production mode should use SameSite=Strict"
        );
    }

    #[test]
    fn attach_auth_cookie_uses_lax_samesite_in_dev_mode() {
        let keypair = Keypair::generate();
        let jwt_secret = vec![0u8; 32];
        let cfg = AuthConfig {
            access_token_ttl_seconds: 60,
            session_ttl_seconds: 3600,
            production_mode: false,
            dev_login_enabled: true,
            use_ed25519: true,
            jwt_kid: "test-kid".to_string(),
            ed25519_keypair: &keypair,
            jwt_secret: &jwt_secret,
            cookie_same_site: "Lax".to_string(),
            cookie_domain: None,
            cookie_secure: false,
            clock_skew_seconds: 60,
        };

        let ctx = AuthContext::from_user(sample_user()).unwrap();
        let token = build_auth_token(&ctx, &cfg, None).expect("token generated");
        let mut headers = HeaderMap::new();
        attach_auth_cookie(&mut headers, &token, &cfg).expect("cookie attached");
        let cookie = headers
            .get(header::SET_COOKIE)
            .expect("cookie header should exist")
            .to_str()
            .unwrap();
        assert!(
            !cookie.contains("Secure"),
            "dev mode should not set Secure flag"
        );
        assert!(cookie.contains("HttpOnly"));
        assert!(
            cookie.contains("SameSite=Lax"),
            "dev mode should use SameSite=Lax for cross-origin dev servers"
        );
    }

    #[test]
    fn attach_refresh_cookie_sets_refresh_name() {
        let keypair = Keypair::generate();
        let jwt_secret = vec![0u8; 32];
        let cfg = AuthConfig {
            access_token_ttl_seconds: 60,
            session_ttl_seconds: 3600,
            production_mode: false,
            dev_login_enabled: true,
            use_ed25519: true,
            jwt_kid: "test-kid".to_string(),
            ed25519_keypair: &keypair,
            jwt_secret: &jwt_secret,
            cookie_same_site: "Lax".to_string(),
            cookie_domain: None,
            cookie_secure: false,
            clock_skew_seconds: 60,
        };

        let ctx = AuthContext::from_user(sample_user()).unwrap();
        let token = build_auth_token(&ctx, &cfg, None).expect("token generated");
        let mut headers = HeaderMap::new();
        attach_refresh_cookie(&mut headers, &token, &cfg).expect("cookie attached");
        let cookie = headers
            .get(header::SET_COOKIE)
            .expect("cookie header should exist")
            .to_str()
            .unwrap();
        assert!(cookie.starts_with("refresh_token="));
    }

    #[test]
    fn permissions_for_role_returns_admin_permissions() {
        let role = Role::Admin;
        let perms = permissions_for_role(&role);
        assert!(perms.contains(&Permission::AdapterDelete));
        assert!(perms.contains(&Permission::PolicyApply));
    }

    #[test]
    fn dev_login_allowed_requires_explicit_enablement() {
        let keypair = Keypair::generate();
        let jwt_secret = vec![0u8; 32];

        // Dev mode, dev_login_enabled=false → NO bypass (explicit opt-in required)
        let cfg = AuthConfig {
            access_token_ttl_seconds: 60,
            session_ttl_seconds: 3600,
            production_mode: false,
            dev_login_enabled: false,
            use_ed25519: true,
            jwt_kid: "test-kid".to_string(),
            ed25519_keypair: &keypair,
            jwt_secret: &jwt_secret,
            cookie_same_site: "Lax".to_string(),
            cookie_domain: None,
            cookie_secure: false,
            clock_skew_seconds: 60,
        };
        assert!(
            !cfg.dev_login_allowed(),
            "dev bypass should require explicit enablement even in dev mode"
        );

        // Dev mode, dev_login_enabled=true → bypass allowed only if dev-bypass feature is enabled
        let cfg = AuthConfig {
            dev_login_enabled: true,
            ..cfg
        };
        // dev_login_allowed() requires BOTH dev_login_enabled AND cfg!(feature = "dev-bypass")
        // Without the feature flag, it should still return false
        #[cfg(feature = "dev-bypass")]
        assert!(
            cfg.dev_login_allowed(),
            "dev bypass should be allowed when explicitly enabled with dev-bypass feature"
        );
        #[cfg(not(feature = "dev-bypass"))]
        assert!(
            !cfg.dev_login_allowed(),
            "dev bypass requires dev-bypass feature at compile time"
        );

        // Prod mode, dev_login_enabled=false → NO bypass
        let cfg = AuthConfig {
            production_mode: true,
            dev_login_enabled: false,
            access_token_ttl_seconds: 60,
            session_ttl_seconds: 3600,
            use_ed25519: true,
            jwt_kid: "test-kid".to_string(),
            ed25519_keypair: &keypair,
            jwt_secret: &jwt_secret,
            cookie_same_site: "Strict".to_string(),
            cookie_domain: None,
            cookie_secure: true,
            clock_skew_seconds: 60,
        };
        assert!(
            !cfg.dev_login_allowed(),
            "dev bypass should be blocked when not enabled in prod mode"
        );

        // Prod mode, dev_login_enabled=true → bypass allowed only with feature flag AND debug_assertions
        let cfg = AuthConfig {
            dev_login_enabled: true,
            ..cfg
        };
        #[cfg(all(feature = "dev-bypass", debug_assertions))]
        assert!(
            cfg.dev_login_allowed(),
            "dev bypass should be allowed in prod when explicitly enabled (for staging/QA) with dev-bypass feature"
        );
        #[cfg(not(all(feature = "dev-bypass", debug_assertions)))]
        assert!(
            !cfg.dev_login_allowed(),
            "dev bypass requires both dev-bypass feature AND debug_assertions at compile time"
        );
    }
}
