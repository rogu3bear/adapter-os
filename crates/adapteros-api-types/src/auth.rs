//! Authentication related types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

/// Login request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Login response with JWT token
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoginResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub token: String,
    pub user_id: String,
    pub role: String,
}

/// User information response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserInfoResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
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
