//! Authentication state management
//!
//! Provides reactive auth state and actions.

use crate::api::{ApiClient, ApiError};
use adapteros_api_types::UserInfoResponse;
use leptos::prelude::*;
use std::sync::Arc;

/// Check if we're running in dev mode on localhost
///
/// Returns true when running on localhost/127.0.0.1.
/// This allows the UI to work without a backend during development.
fn is_dev_localhost() -> bool {
    let hostname = web_sys::window().and_then(|w| w.location().hostname().ok());

    web_sys::console::log_1(&format!("[auth] Hostname check: {:?}", hostname).into());

    let result = hostname
        .map(|h| h == "localhost" || h == "127.0.0.1")
        .unwrap_or(false);

    web_sys::console::log_1(&format!("[auth] is_dev_localhost result: {}", result).into());
    result
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
    /// Auth error
    Error(String),
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
        self.attempt_id.get()
    }

    /// Increment attempt and return new ID
    fn next_attempt(&self) -> u32 {
        self.attempt_id.update(|id| *id += 1);
        self.attempt_id.get()
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
                        self.state.set(AuthState::Authenticated(Box::new(user)));
                        Ok(())
                    }
                    Err(e) => {
                        self.client.clear_auth_status();
                        self.state.set(AuthState::Error(e.to_string()));
                        Err(e)
                    }
                }
            }
            Err(e) => {
                self.client.clear_auth_status();
                self.state.set(AuthState::Error(e.to_string()));
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
        web_sys::console::log_1(&format!("[auth] Starting auth check (attempt {})", my_attempt).into());

        self.state.set(AuthState::Loading);

        // Try to get user info - will succeed if we have valid auth cookies
        let result = self.client.me().await;

        // Only update state if this is still the current attempt
        // (prevents late-arriving results from overwriting newer state)
        if self.current_attempt() != my_attempt {
            web_sys::console::log_1(
                &format!("[auth] Ignoring stale auth result (attempt {} != current {})",
                    my_attempt, self.current_attempt()).into()
            );
            return;
        }

        match result {
            Ok(user) => {
                self.client.set_auth_status(true);
                self.state.set(AuthState::Authenticated(Box::new(user)));
            }
            Err(ApiError::Unauthorized) => {
                self.client.clear_auth_status();
                self.state.set(AuthState::Unauthenticated);
            }
            Err(e) => {
                self.client.clear_auth_status();
                self.state.set(AuthState::Error(e.to_string()));
            }
        }
    }
}

/// Auth context type
pub type AuthContext = (ReadSignal<AuthState>, AuthAction);

/// Provide auth context to the application
pub fn provide_auth_context() {
    let client = Arc::new(ApiClient::new());
    let state = RwSignal::new(AuthState::Unknown);
    let action = AuthAction::new(Arc::clone(&client), state);

    // Dev bypass: skip auth check and use mock user on localhost
    if is_dev_localhost() {
        web_sys::console::log_1(&"[auth] Dev bypass active - using mock user".into());
        state.set(AuthState::Authenticated(Box::new(mock_dev_user())));
        provide_context((state.read_only(), action));
        return;
    }

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
            wasm_bindgen_futures::spawn_local(async move {
                // Race auth check against timeout
                let auth_future = action.check_auth();
                let timeout_future = gloo_timers::future::TimeoutFuture::new(AUTH_TIMEOUT_MS);

                // Use futures::select! to race the two futures
                futures::pin_mut!(auth_future);
                futures::pin_mut!(timeout_future);

                match futures::future::select(auth_future, timeout_future).await {
                    futures::future::Either::Left(_) => {
                        // Auth completed (success or error) - state already set by check_auth
                        web_sys::console::log_1(&"[auth] Auth check completed".into());
                    }
                    futures::future::Either::Right(_) => {
                        // Timeout - only set if still loading
                        if state_timeout.get().is_loading() {
                            web_sys::console::warn_1(
                                &format!("[auth] Auth check timed out after {}ms", AUTH_TIMEOUT_MS).into()
                            );
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
