//! Authentication state management
//!
//! Provides reactive auth state and actions.

use crate::api::{ApiClient, ApiError};
use adapteros_api_types::UserInfoResponse;
use leptos::prelude::*;
use std::sync::Arc;

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
    Authenticated(UserInfoResponse),
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
                        self.state.set(AuthState::Authenticated(user));
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
                self.state.set(AuthState::Authenticated(user));
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

    // Check auth on mount
    let action_check = action.clone();
    Effect::new(move || {
        let action = action_check.clone();
        wasm_bindgen_futures::spawn_local(async move {
            action.check_auth().await;
        });
    });

    provide_context((state.read_only(), action));
}

/// Use auth context
pub fn use_auth() -> AuthContext {
    expect_context::<AuthContext>()
}
