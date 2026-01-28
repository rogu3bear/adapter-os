//! Policy types
//!
//! Placeholder types for policy management.
//! These will be populated when policy types are moved from adapteros-server-api.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Policy status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyStatus {
    /// Policy is active and enforced
    Active,
    /// Policy is disabled
    Disabled,
    /// Policy is in draft/testing mode
    Draft,
}

/// Policy summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySummary {
    /// Unique policy identifier
    pub id: Uuid,
    /// Policy name
    pub name: String,
    /// Policy description
    pub description: Option<String>,
    /// Current status
    pub status: PolicyStatus,
}

/// Policy list response
#[derive(Debug, Serialize)]
pub struct PolicyListResponse {
    /// List of policies
    pub policies: Vec<PolicySummary>,
    /// Total count
    pub total: usize,
}
