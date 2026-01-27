//! Authentication types for admin handlers
//!
//! Provides the AdminClaims type that handlers use for authorization.

use serde::{Deserialize, Serialize};

/// JWT claims for admin authentication
///
/// This is a subset of the full Claims type from adapteros-server-api,
/// containing only the fields needed for admin handler authorization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminClaims {
    /// Subject (user ID)
    pub sub: String,
    /// User role (e.g., "admin", "operator", "viewer")
    pub role: String,
    /// Tenant ID
    pub tenant_id: String,
    /// Admin tenants list (for wildcard admin checks)
    #[serde(default)]
    pub admin_tenants: Vec<String>,
}

impl AdminClaims {
    /// Create new admin claims
    pub fn new(sub: String, role: String, tenant_id: String) -> Self {
        Self {
            sub,
            role,
            tenant_id,
            admin_tenants: Vec::new(),
        }
    }

    /// Add admin tenants
    pub fn with_admin_tenants(mut self, tenants: Vec<String>) -> Self {
        self.admin_tenants = tenants;
        self
    }
}
