use crate::auth::{generate_token, generate_token_ed25519};
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
    pub token_ttl_seconds: u64,
    pub production_mode: bool,
    pub dev_login_enabled: bool,
    pub use_ed25519: bool,
    pub ed25519_keypair: &'a Keypair,
    pub jwt_secret: &'a [u8],
}

impl<'a> AuthConfig<'a> {
    /// Builds the configuration from the current app state.
    pub fn from_state(state: &'a AppState) -> Self {
        let config = state.config.read().unwrap();
        let ttl = config.security.token_ttl_seconds.unwrap_or(8 * 3600);
        let token_ttl_seconds = if ttl == 0 { 8 * 3600 } else { ttl };

        Self {
            token_ttl_seconds,
            production_mode: config.server.production_mode,
            dev_login_enabled: config.security.dev_login_enabled,
            use_ed25519: state.use_ed25519,
            ed25519_keypair: &state.ed25519_keypair,
            jwt_secret: state.jwt_secret.as_slice(),
        }
    }

    /// Effective TTL used for cookies and tokens.
    pub fn effective_ttl(&self) -> u64 {
        self.token_ttl_seconds
    }

    /// Whether dev login is allowed under the current config.
    /// Requires explicit opt-in via `dev_login_enabled` regardless of production mode.
    /// This allows staging/QA environments to use dev bypass when explicitly configured.
    pub fn dev_login_allowed(&self) -> bool {
        self.dev_login_enabled && cfg!(all(feature = "dev-bypass", debug_assertions))
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
}

impl AuthContext {
    /// Creates an auth context from a user row.
    pub fn from_user(user: User) -> Result<Self, AuthError> {
        let role = Role::from_str(&user.role).map_err(|e| AuthError::RoleParse(e.to_string()))?;
        let permissions = permissions_for_role(&role);
        let tenant_id = user.tenant_id.clone();
        let display_name = user.display_name.clone();

        Ok(Self {
            user,
            tenant_id,
            display_name,
            role,
            permissions,
            mfa_enabled: false, // TODO: wire actual MFA flag from user table
            password_rotated_at: None, // TODO: respect real password rotation metadata
            token_rotated_at: None, // TODO: wire token rotation timestamps
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
pub fn build_auth_token(ctx: &AuthContext, cfg: &AuthConfig) -> Result<String, AuthError> {
    let ttl = cfg.effective_ttl();

    if cfg.use_ed25519 {
        generate_token_ed25519(
            &ctx.user.id,
            &ctx.user.email,
            &ctx.role.to_string(),
            &ctx.tenant_id,
            cfg.ed25519_keypair,
            ttl,
        )
        .map_err(AuthError::Token)
    } else {
        generate_token(
            &ctx.user.id,
            &ctx.user.email,
            &ctx.role.to_string(),
            &ctx.tenant_id,
            cfg.jwt_secret,
            ttl,
        )
        .map_err(AuthError::Token)
    }
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
        last_login_at: None, // TODO: surface actual last-login timestamp
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
    let expires = cfg.cookie_expiration();
    let expires_value = expires.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    // In production: Secure + SameSite=Strict for max security
    // In development: SameSite=Lax to allow cross-origin cookie sharing (e.g., UI on :3200, API on :8080)
    let (secure_flag, samesite) = if cfg.production_mode {
        ("; Secure", "Strict")
    } else {
        ("", "Lax")
    };

    let cookie_value = format!(
        "auth_token={token}; HttpOnly; Path=/; Max-Age={max_age}; Expires={expires}; SameSite={samesite}{secure_flag}",
        token = token,
        max_age = cfg.effective_ttl(),
        expires = expires_value,
        samesite = samesite,
        secure_flag = secure_flag
    );

    let header_value = HeaderValue::from_str(&cookie_value)?;
    headers.insert(header::SET_COOKIE, header_value);
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
            token_ttl_seconds: 60,
            production_mode: true,
            dev_login_enabled: false,
            use_ed25519: true,
            ed25519_keypair: &keypair,
            jwt_secret: &jwt_secret,
        };

        let ctx = AuthContext::from_user(sample_user()).unwrap();
        let token = build_auth_token(&ctx, &cfg).expect("token generated");
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
            token_ttl_seconds: 60,
            production_mode: false,
            dev_login_enabled: true,
            use_ed25519: true,
            ed25519_keypair: &keypair,
            jwt_secret: &jwt_secret,
        };

        let ctx = AuthContext::from_user(sample_user()).unwrap();
        let token = build_auth_token(&ctx, &cfg).expect("token generated");
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
            token_ttl_seconds: 60,
            production_mode: false,
            dev_login_enabled: false,
            use_ed25519: true,
            ed25519_keypair: &keypair,
            jwt_secret: &jwt_secret,
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
            token_ttl_seconds: 60,
            use_ed25519: true,
            ed25519_keypair: &keypair,
            jwt_secret: &jwt_secret,
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
