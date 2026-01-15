//! API key management types
//!
//! Types for API key creation, listing, and revocation.

use serde::{Deserialize, Serialize};

/// Request to create a new API key
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CreateApiKeyRequest {
    /// Name/label for the API key
    pub name: String,
    /// List of roles/scopes allowed for this key
    pub scopes: Vec<String>,
}

/// Response after creating an API key (includes the actual token - shown only once)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CreateApiKeyResponse {
    pub id: String,
    /// The actual API token - only shown once at creation time
    pub token: String,
    pub created_at: String,
}

/// API key info (without the actual token)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ApiKeyInfo {
    pub id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
}

/// Response for listing API keys
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ApiKeyListResponse {
    pub api_keys: Vec<ApiKeyInfo>,
}

/// Response after revoking an API key
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RevokeApiKeyResponse {
    pub id: String,
    pub revoked: bool,
}
