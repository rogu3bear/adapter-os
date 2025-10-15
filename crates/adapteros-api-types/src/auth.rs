//! Authentication related types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Login request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Login response with JWT token
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoginResponse {
    pub token: String,
    pub user_id: String,
    pub role: String,
}

/// User information response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserInfoResponse {
    pub user_id: String,
    pub email: String,
    pub role: String,
    pub created_at: String,
}

/// Logout request (empty for now, but extensible)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LogoutRequest {
    // Future: could include session invalidation details
}
