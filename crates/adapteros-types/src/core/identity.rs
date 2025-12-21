//! Identity primitives for AdapterOS entities

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Tenant identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub String);

impl TenantId {
    /// Create a new tenant ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the inner string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for TenantId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TenantId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// User identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub Uuid);

impl UserId {
    /// Create a new user ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from existing UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

/// Adapter identifier (BLAKE3 hash)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AdapterId(pub String);

impl AdapterId {
    /// Create a new adapter ID from hash string
    pub fn new(hash: impl Into<String>) -> Self {
        Self(hash.into())
    }

    /// Get the inner hash string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for AdapterId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for AdapterId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Training job identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrainingJobId(pub Uuid);

impl TrainingJobId {
    /// Create a new training job ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from existing UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for TrainingJobId {
    fn default() -> Self {
        Self::new()
    }
}
