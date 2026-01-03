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
}

impl AuthAction {
    /// Create new auth action
    pub fn new(client: Arc<ApiClient>, state: RwSignal<AuthState>) -> Self {
        Self { client, state }
    }

    /// Login with credentials
    pub async fn login(&self, username: &str, password: &str) -> Result<(), ApiError> {
        self.state.set(AuthState::Loading);

        match self.client.login(username, password).await {
            Ok(response) => {
                // Store token
                self.client.set_token(Some(response.token.clone()));

                // Fetch user info
                match self.client.me().await {
                    Ok(user) => {
                        self.state.set(AuthState::Authenticated(Box::new(user)));
                        Ok(())
                    }
                    Err(e) => {
                        self.state.set(AuthState::Error(e.to_string()));
                        Err(e)
                    }
                }
            }
            Err(e) => {
                self.state.set(AuthState::Error(e.to_string()));
                Err(e)
            }
        }
    }

    /// Logout
    pub async fn logout(&self) {
        let _ = self.client.logout().await;
        self.client.set_token(None);
        self.state.set(AuthState::Unauthenticated);
    }

    /// Check current auth status
    pub async fn check_auth(&self) {
        self.state.set(AuthState::Loading);

        if !self.client.is_authenticated() {
            self.state.set(AuthState::Unauthenticated);
            return;
        }

        match self.client.me().await {
            Ok(user) => {
                self.state.set(AuthState::Authenticated(Box::new(user)));
            }
            Err(ApiError::Unauthorized) => {
                self.state.set(AuthState::Unauthenticated);
            }
            Err(e) => {
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

    // Normal auth check for production
    // Use a guard to ensure check_auth only runs once on initial mount
    let action_check = action.clone();
    let has_checked = StoredValue::new(false);
    Effect::new(move || {
        // Only check auth once on initial mount to prevent infinite Effect re-runs
        if !has_checked.get_value() {
            has_checked.set_value(true);
            let action = action_check.clone();
            wasm_bindgen_futures::spawn_local(async move {
                action.check_auth().await;
            });
        }
    });

    provide_context((state.read_only(), action));
}

/// Use auth context
pub fn use_auth() -> AuthContext {
    expect_context::<AuthContext>()
}
