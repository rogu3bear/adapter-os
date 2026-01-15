//! Admin management types
//!
//! Types for user management and admin operations.

use serde::{Deserialize, Serialize};

use crate::schema_version;

/// User response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct UserResponse {
    pub user_id: String,
    /// Alias for user_id
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub tenant_id: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mfa_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<String>>,
}

/// List users response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ListUsersResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub users: Vec<UserResponse>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

/// List users query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ListUsersParams {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size")]
    pub page_size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

fn default_page() -> i64 {
    1
}

fn default_page_size() -> i64 {
    100
}
