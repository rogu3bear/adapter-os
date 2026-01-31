//! Authentication state management
//!
//! Provides reactive auth state and actions.

use crate::api::{ApiClient, ApiError};
use crate::boot_log;
use adapteros_api_types::{FailureCode, UserInfoResponse};
use leptos::prelude::*;
use serde::Deserialize;
use std::sync::Arc;

/// Check if we're running in dev mode on localhost
///
/// Returns true when running on localhost/127.0.0.1.
/// This allows the UI to work without a backend during development.
fn is_dev_localhost() -> bool {
    web_sys::window()
        .and_then(|w| w.location().hostname().ok())
        .map(|h| h == "localhost" || h == "127.0.0.1")
        .unwrap_or(false)
}

#[derive(Debug, Deserialize)]
struct AuthConfigPublic {
    dev_bypass_allowed: bool,
}

async fn dev_bypass_allowed(client: &ApiClient) -> bool {
    match client.get::<AuthConfigPublic>("/v1/auth/config").await {
        Ok(cfg) => cfg.dev_bypass_allowed,
        Err(e) => {
            boot_log("auth", &format!("dev bypass check failed: {}", e));
            false
        }
    }
}

/// Create a mock user for dev bypass mode
fn mock_dev_user() -> UserInfoResponse {
    UserInfoResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        user_id: "dev-user-001".to_string(),
        email: "dev@localhost".to_string(),
        role: "admin".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        tenant_id: "dev-tenant-001".to_string(),
        display_name: "Dev User".to_string(),
        permissions: vec!["read".to_string(), "write".to_string(), "admin".to_string()],
        admin_tenants: vec!["dev-tenant-001".to_string()],
        last_login_at: None,
        mfa_enabled: Some(false),
        token_last_rotated_at: None,
    }
}

/// Auth check timeout in milliseconds
/// Dev: 3 seconds, Prod: 10 seconds (allows for cold start)
const AUTH_TIMEOUT_MS: u32 = if cfg!(debug_assertions) { 3000 } else { 10000 };

/// Classified authentication errors with user-friendly messages
#[derive(Debug, Clone)]
pub enum AuthError {
    /// Token expired or invalid - user should re-login
    TokenExpired,
    /// Token was revoked (e.g., password changed, admin action)
    TokenRevoked,
    /// User doesn't have access to the requested tenant
    TenantMismatch,
    /// Tenant ID missing from claims
    TenantMissing,
    /// Server unavailable or network error (retryable)
    ServerUnavailable,
    /// Generic auth failure with message
    Other(String),
}

impl AuthError {
    /// User-facing error message
    ///
    /// Uses standardized wording:
    /// - "Log in" for auth actions (not "Sign in")
    /// - "Retry" for retryable errors
    pub fn message(&self) -> &str {
        match self {
            Self::TokenExpired => "Your session has expired. Log in again.",
            Self::TokenRevoked => "Your session was revoked. Log in again.",
            Self::TenantMismatch => "You don't have access to this workspace.",
            Self::TenantMissing => "No workspace associated with your account.",
            Self::ServerUnavailable => "Unable to reach the server. Retry or check your connection.",
            Self::Other(msg) => msg,
        }
    }

    /// Whether this error is retryable (vs requiring re-login)
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::ServerUnavailable)
    }

    /// Whether this error requires the user to log in again
    pub fn requires_login(&self) -> bool {
        matches!(
            self,
            Self::TokenExpired | Self::TokenRevoked | Self::TenantMismatch | Self::TenantMissing
        )
    }

    /// Classify an ApiError into an AuthError
    fn from_api_error(err: &ApiError) -> Self {
        match err {
            ApiError::Unauthorized => Self::TokenExpired,
            ApiError::Forbidden(msg) if msg.contains("revoked") => Self::TokenRevoked,
            ApiError::Forbidden(_) => Self::TenantMismatch,
            ApiError::Network(_) => Self::ServerUnavailable,
            ApiError::Structured { failure_code, .. } => match failure_code {
                Some(FailureCode::TenantAccessDenied) => Self::TenantMismatch,
                _ => Self::Other(err.to_string()),
            },
            _ => Self::Other(err.to_string()),
        }
    }
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

/// Authentication state
#[derive(Debug, Clone)]
pub enum AuthState {
    /// Not yet checked
    Unknown,
    /// Checking authentication
    Loading,
    /// Not authenticated
    Unauthenticated,
    /// Authenticated with user info
    Authenticated(Box<UserInfoResponse>),
    /// Auth error with classified error type
    Error(AuthError),
    /// Auth check timed out
    Timeout,
}

impl AuthState {
    /// Check if user is authenticated
    pub fn is_authenticated(&self) -> bool {
        matches!(self, Self::Authenticated(_))
    }

    /// Check if auth is loading
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading | Self::Unknown)
    }

    /// Get user info if authenticated
    pub fn user(&self) -> Option<&UserInfoResponse> {
        match self {
            Self::Authenticated(user) => Some(user),
            _ => None,
        }
    }

    /// Get the auth error if in error state
    pub fn error(&self) -> Option<&AuthError> {
        match self {
            Self::Error(err) => Some(err),
            _ => None,
        }
    }
}

/// Auth actions
#[derive(Clone)]
pub struct AuthAction {
    client: Arc<ApiClient>,
    state: RwSignal<AuthState>,
    /// Attempt counter to prevent late state updates from stale requests
    attempt_id: RwSignal<u32>,
}

impl AuthAction {
    /// Create new auth action
    pub fn new(client: Arc<ApiClient>, state: RwSignal<AuthState>) -> Self {
        Self {
            client,
            state,
            attempt_id: RwSignal::new(0),
        }
    }

    /// Get current attempt ID
    fn current_attempt(&self) -> u32 {
        self.attempt_id.get_untracked()
    }

    /// Increment attempt and return new ID
    fn next_attempt(&self) -> u32 {
        self.attempt_id.update(|id| *id += 1);
        self.attempt_id.get_untracked()
    }

    /// Login with credentials
    ///
    /// Server sets httpOnly cookies automatically on successful login.
    pub async fn login(&self, username: &str, password: &str) -> Result<(), ApiError> {
        self.state.set(AuthState::Loading);

        match self.client.login(username, password).await {
            Ok(_response) => {
                // Server has set httpOnly auth cookies automatically
                // Mark client as authenticated
                self.client.set_auth_status(true);

                // Fetch user info to confirm auth
                match self.client.me().await {
                    Ok(user) => {
                        // Validate tenant_id is present
                        if user.tenant_id.is_empty() {
                            boot_log("auth", "login failed: tenant_id missing from claims");
                            self.client.clear_auth_status();
                            self.state.set(AuthState::Error(AuthError::TenantMissing));
                            return Err(ApiError::Validation(
                                "No workspace associated with account".to_string(),
                            ));
                        }
                        self.state.set(AuthState::Authenticated(Box::new(user)));
                        Ok(())
                    }
                    Err(e) => {
                        self.client.clear_auth_status();
                        self.state
                            .set(AuthState::Error(AuthError::from_api_error(&e)));
                        Err(e)
                    }
                }
            }
            Err(e) => {
                self.client.clear_auth_status();
                self.state
                    .set(AuthState::Error(AuthError::from_api_error(&e)));
                Err(e)
            }
        }
    }

    /// Logout
    ///
    /// Calls server to clear httpOnly cookies.
    pub async fn logout(&self) {
        let _ = self.client.logout().await;
        self.client.clear_auth_status();
        self.state.set(AuthState::Unauthenticated);
    }

    /// Check current auth status
    ///
    /// Verifies authentication by calling /v1/auth/me endpoint.
    /// With httpOnly cookies, we can't check the token directly.
    ///
    /// Uses attempt ID to prevent late state updates from stale requests.
    /// If a new check_auth is started before this one completes, this one's
    /// result will be ignored.
    pub async fn check_auth(&self) {
        // Increment attempt ID and capture it for this request
        let my_attempt = self.next_attempt();
        boot_log("auth", &format!("started (attempt {})", my_attempt));

        self.state.set(AuthState::Loading);

        // Try to get user info - will succeed if we have valid auth cookies
        let result = self.client.me().await;

        // Only update state if this is still the current attempt
        // (prevents late-arriving results from overwriting newer state)
        if self.current_attempt() != my_attempt {
            boot_log(
                "auth",
                &format!(
                    "ignoring stale result (attempt {} != current {})",
                    my_attempt,
                    self.current_attempt()
                ),
            );
            return;
        }

        match result {
            Ok(user) => {
                // Validate tenant_id is present
                if user.tenant_id.is_empty() {
                    boot_log("auth", "check_auth failed: tenant_id missing from claims");
                    self.client.clear_auth_status();
                    self.state.set(AuthState::Error(AuthError::TenantMissing));
                    return;
                }
                boot_log("auth", &format!("authenticated as {}", user.email));
                self.client.set_auth_status(true);
                self.state.set(AuthState::Authenticated(Box::new(user)));
            }
            Err(ApiError::Unauthorized) => {
                boot_log("auth", "401 unauthorized");
                self.client.clear_auth_status();
                self.state.set(AuthState::Unauthenticated);
            }
            Err(e) => {
                let auth_error = AuthError::from_api_error(&e);
                boot_log("auth", &format!("error: {} ({})", auth_error, e));
                self.client.clear_auth_status();
                self.state.set(AuthState::Error(auth_error));
            }
        }
    }
}

/// Auth context type
pub type AuthContext = (ReadSignal<AuthState>, AuthAction);

/// Provide auth context to the application
pub fn provide_auth_context() {
    boot_log("auth", "initializing auth context");
    let client = Arc::new(ApiClient::new());
    let state = RwSignal::new(AuthState::Unknown);
    let action = AuthAction::new(Arc::clone(&client), state);

    // Normal auth check for production with timeout
    // Use a guard to ensure check_auth only runs once on initial mount
    let action_check = action.clone();
    let has_checked = StoredValue::new(false);
    Effect::new(move || {
        // Only check auth once on initial mount to prevent infinite Effect re-runs
        if !has_checked.get_value() {
            has_checked.set_value(true);
            let action = action_check.clone();
            let state_timeout = state;
            let client = Arc::clone(&client);
            wasm_bindgen_futures::spawn_local(async move {
                if is_dev_localhost() && dev_bypass_allowed(&client).await {
                    boot_log("auth", "dev bypass active (localhost + allowed)");
                    state_timeout.set(AuthState::Authenticated(Box::new(mock_dev_user())));
                    return;
                }

                // Race auth check against timeout
                let auth_future = action.check_auth();
                let timeout_future = gloo_timers::future::TimeoutFuture::new(AUTH_TIMEOUT_MS);

                // Use futures::select! to race the two futures
                futures::pin_mut!(auth_future);
                futures::pin_mut!(timeout_future);

                match futures::future::select(auth_future, timeout_future).await {
                    futures::future::Either::Left(_) => {
                        // Auth completed (success or error) - state already set by check_auth
                        boot_log("auth", "check completed");
                    }
                    futures::future::Either::Right(_) => {
                        // Timeout - only set if still loading
                        if state_timeout.get().is_loading() {
                            boot_log("auth", &format!("TIMEOUT after {}ms", AUTH_TIMEOUT_MS));
                            state_timeout.set(AuthState::Timeout);
                        }
                    }
                }
            });
        }
    });

    provide_context((state.read_only(), action));
}

/// Use auth context
pub fn use_auth() -> AuthContext {
    expect_context::<AuthContext>()
}
