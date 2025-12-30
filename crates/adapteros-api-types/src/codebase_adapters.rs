//! Codebase adapter API types
//!
//! Types for codebase adapter management endpoints:
//! - POST /v1/adapters/codebase - Create a codebase adapter
//! - GET /v1/adapters/codebase/:id - Get codebase adapter details
//! - POST /v1/adapters/codebase/:id/bind - Bind to session
//! - POST /v1/adapters/codebase/:id/unbind - Unbind from session
//! - POST /v1/adapters/codebase/:id/version - Create new version
//! - POST /v1/adapters/codebase/:id/verify - Verify deployment readiness
//!
//! 【2025-01-29†prd-adapters†codebase_api_types】

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request to create a codebase adapter
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateCodebaseAdapterRequest {
    /// Unique adapter ID (must follow code.<repo_slug>.<commit> format)
    pub adapter_id: String,

    /// Base adapter ID (required - the core adapter this codebase extends)
    pub base_adapter_id: String,

    /// Repository identifier (e.g., "owner/repo")
    pub repo_id: String,

    /// Git commit SHA
    pub commit_sha: String,

    /// Manifest hash for deterministic verification
    pub manifest_hash: String,

    /// Optional human-readable name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Optional versioning threshold (default: 100)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub versioning_threshold: Option<i32>,

    /// Optional session ID to bind immediately
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Repository path for deployment verification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_path: Option<String>,
}

/// Response from creating a codebase adapter
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateCodebaseAdapterResponse {
    /// The created adapter ID
    pub adapter_id: String,

    /// The base adapter ID
    pub base_adapter_id: String,

    /// Initial version
    pub version: String,

    /// Session binding (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Creation timestamp
    pub created_at: String,
}

/// Request to bind a codebase adapter to a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BindSessionRequest {
    /// Session ID to bind to (exclusive binding)
    pub session_id: String,
}

/// Response from binding a codebase adapter
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BindSessionResponse {
    /// The adapter ID
    pub adapter_id: String,

    /// The session ID now bound
    pub session_id: String,

    /// Binding timestamp
    pub bound_at: String,
}

/// Response from unbinding a codebase adapter
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UnbindSessionResponse {
    /// The adapter ID
    pub adapter_id: String,

    /// The session ID that was unbound (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_session_id: Option<String>,

    /// Whether versioning was triggered
    pub versioned: bool,

    /// New version (if versioned)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_version: Option<String>,

    /// Unbinding timestamp
    pub unbound_at: String,
}

/// Request to create a new version of a codebase adapter
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VersionCodebaseAdapterRequest {
    /// Version bump type: "patch", "minor", or "major"
    #[serde(default = "default_bump_type")]
    pub bump_type: String,

    /// Optional reason for versioning
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn default_bump_type() -> String {
    "patch".to_string()
}

/// Response from creating a new version
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VersionCodebaseAdapterResponse {
    /// The new version adapter ID
    pub new_adapter_id: String,

    /// The previous adapter ID (now parent)
    pub previous_adapter_id: String,

    /// The new version string
    pub version: String,

    /// The previous version string
    pub previous_version: String,

    /// Creation timestamp
    pub created_at: String,
}

/// Request to verify deployment readiness
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerifyDeploymentRequest {
    /// Repository path to check (overrides stored path)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_path: Option<String>,

    /// Expected manifest hash to verify
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_manifest_hash: Option<String>,

    /// Expected CoreML package hash to verify
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_coreml_hash: Option<String>,

    /// Current session ID for conflict check
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Individual verification check result
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerificationCheck {
    /// Name of the check
    pub name: String,

    /// Whether the check passed
    pub passed: bool,

    /// Details about the check result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Response from deployment verification
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerifyDeploymentResponse {
    /// The adapter ID
    pub adapter_id: String,

    /// Whether all checks passed
    pub ready: bool,

    /// Individual check results
    pub checks: Vec<VerificationCheck>,

    /// Verification timestamp
    pub verified_at: String,
}

/// Codebase adapter detail response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CodebaseAdapterResponse {
    /// Adapter ID
    pub adapter_id: String,

    /// Human-readable name
    pub name: String,

    /// Base adapter ID (core adapter baseline)
    pub base_adapter_id: String,

    /// Current version
    pub version: String,

    /// Lifecycle state
    pub lifecycle_state: String,

    /// Bound session ID (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Repository ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,

    /// Commit SHA
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,

    /// Manifest hash
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_hash: Option<String>,

    /// CoreML package hash (if fused)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_package_hash: Option<String>,

    /// Activation count
    pub activation_count: i64,

    /// Versioning threshold
    pub versioning_threshold: i32,

    /// Whether auto-versioning is due
    pub auto_version_due: bool,

    /// Parent adapter ID (version lineage)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,

    /// Creation timestamp
    pub created_at: String,

    /// Last update timestamp
    pub updated_at: String,
}
