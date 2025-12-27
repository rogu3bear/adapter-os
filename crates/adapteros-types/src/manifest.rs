//! Shared manifest metadata types.

use serde::{Deserialize, Serialize};

/// Persisted manifest metadata stored in the control plane.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct Manifest {
    /// Unique manifest identifier
    pub id: String,
    /// Tenant owning the manifest
    pub tenant_id: String,
    /// BLAKE3 hash of the manifest body
    pub hash_b3: String,
    /// Raw manifest JSON payload
    pub body_json: String,
    /// Creation timestamp (RFC3339)
    pub created_at: String,
}
