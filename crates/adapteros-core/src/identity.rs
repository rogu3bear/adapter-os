//! Identity envelope for events and logs
//!
//! Provides a typed structure ensuring every event and log carries complete identity information.
//! All fields are required - no optional values.
//!
//! # Citations
//! - PRD 1: Global Identity Envelope for Events & Logs

use crate::AosError;
use serde::{Deserialize, Serialize};

/// Identity envelope containing required context for all events and logs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentityEnvelope {
    /// Tenant identifier (e.g., "tenant-a")
    pub tenant_id: String,

    /// Domain of the operation (e.g., "router", "kernel", "cli", "api")
    pub domain: String,

    /// Purpose of the operation (e.g., "inference", "training", "maintenance")
    pub purpose: String,

    /// Revision identifier (code/config hash or semver)
    pub revision: String,
}

impl IdentityEnvelope {
    /// Create a new identity envelope
    pub fn new(tenant_id: String, domain: String, purpose: String, revision: String) -> Self {
        Self {
            tenant_id,
            domain,
            purpose,
            revision,
        }
    }

    /// Validate the envelope fields (basic non-empty check)
    pub fn validate(&self) -> Result<(), AosError> {
        if self.tenant_id.is_empty() {
            return Err(AosError::Validation(
                "tenant_id cannot be empty".to_string(),
            ));
        }
        if self.domain.is_empty() {
            return Err(AosError::Validation("domain cannot be empty".to_string()));
        }
        if self.purpose.is_empty() {
            return Err(AosError::Validation("purpose cannot be empty".to_string()));
        }
        if self.revision.is_empty() {
            return Err(AosError::Validation("revision cannot be empty".to_string()));
        }
        Ok(())
    }

    /// Create default revision from environment AOS_REVISION or git short hash
    pub fn default_revision() -> String {
        std::env::var("AOS_REVISION").unwrap_or_else(|_| {
            // Fallback to git rev-parse --short HEAD if in git repo
            if let Ok(output) = std::process::Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
            {
                if output.status.success() {
                    String::from_utf8_lossy(&output.stdout).trim().to_string()
                } else {
                    "unknown".to_string()
                }
            } else {
                "unknown".to_string()
            }
        })
    }
}
