//! Tenant settings API types
//!
//! Types for the tenant settings endpoints:
//! - GET /v1/tenants/{tenant_id}/settings
//! - PUT /v1/tenants/{tenant_id}/settings

use serde::{Deserialize, Serialize};

use crate::schema_version;

/// Policy for serving codebase adapters
///
/// Controls how codebase adapters are served during inference, particularly
/// regarding CoreML fusion requirements.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct CodebaseServingPolicy {
    /// CoreML fusion policy: "strict" or "mlx_fallback"
    /// - "strict": Block inference until CoreML package is fused (requires coreml_package_hash)
    /// - "mlx_fallback": Serve via MLX while CoreML fusion runs in background (default)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_fusion_policy: Option<String>,

    /// Whether to require deployment verification before codebase adapter activation
    /// Default: true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_deployment_verification: Option<bool>,

    /// Whether to auto-version codebase adapters when threshold is exceeded
    /// Default: true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_versioning_enabled: Option<bool>,

    /// Default versioning threshold for codebase adapters (activations before auto-version)
    /// Default: 100
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_versioning_threshold: Option<i32>,
}

impl Default for CodebaseServingPolicy {
    fn default() -> Self {
        Self {
            coreml_fusion_policy: Some("mlx_fallback".to_string()),
            require_deployment_verification: Some(true),
            auto_versioning_enabled: Some(true),
            default_versioning_threshold: Some(100),
        }
    }
}

impl CodebaseServingPolicy {
    /// Check if strict CoreML mode is required
    pub fn requires_coreml_fusion(&self) -> bool {
        self.coreml_fusion_policy.as_deref() == Some("strict")
    }

    /// Check if deployment verification is required
    pub fn requires_verification(&self) -> bool {
        self.require_deployment_verification.unwrap_or(true)
    }

    /// Check if auto-versioning is enabled
    pub fn auto_version_enabled(&self) -> bool {
        self.auto_versioning_enabled.unwrap_or(true)
    }

    /// Get the versioning threshold
    pub fn versioning_threshold(&self) -> i32 {
        self.default_versioning_threshold.unwrap_or(100)
    }
}

/// Policy knobs for determinism and routing control
///
/// These settings control how the inference engine handles determinism modes,
/// backend fallback, and pinned adapter enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct DeterminismPolicyKnobs {
    /// Allowed determinism modes for this tenant: ["strict", "besteffort", "relaxed"]
    /// If empty or None, all modes are allowed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_modes: Option<Vec<String>>,

    /// How to handle pins outside effective routing set: "warn" (default) or "error"
    /// - "warn": Log warning but continue with available adapters
    /// - "error": Reject request with clear error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pins_outside_effective: Option<String>,

    /// Whether backend fallback is allowed (affects replay guarantee)
    /// - true: Allow fallback to secondary backend on failure (default)
    /// - false: Fail request if primary backend fails (forces strict_mode on worker)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_allowed: Option<bool>,
}

impl Default for DeterminismPolicyKnobs {
    fn default() -> Self {
        Self {
            allowed_modes: None, // All modes allowed
            pins_outside_effective: Some("warn".to_string()),
            fallback_allowed: Some(true),
        }
    }
}

/// Tenant settings response
///
/// Contains the tenant's settings for controlling default stack/adapter behavior.
/// All boolean fields default to false (disabled) for backwards compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TenantSettingsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// The tenant ID these settings belong to
    pub tenant_id: String,
    /// When true, new chat sessions inherit stack_id from tenants.default_stack_id
    pub use_default_stack_on_chat_create: bool,
    /// When true, inference with session_id falls back to tenant default stack
    /// when no adapters/stack are specified in the request
    pub use_default_stack_on_infer_session: bool,
    /// Optional JSON object for experimental settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_json: Option<serde_json::Value>,
    /// Policy knobs for determinism and routing control
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_policy: Option<DeterminismPolicyKnobs>,
    /// Policy for serving codebase adapters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codebase_serving_policy: Option<CodebaseServingPolicy>,
    /// When the settings were created (null if using defaults)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// When the settings were last updated (null if using defaults)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Update tenant settings request
///
/// All fields are optional to support partial updates.
/// Fields not provided will preserve existing values.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct UpdateTenantSettingsRequest {
    /// When true, new chat sessions inherit stack_id from tenants.default_stack_id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_default_stack_on_chat_create: Option<bool>,
    /// When true, inference with session_id falls back to tenant default stack
    /// when no adapters/stack are specified in the request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_default_stack_on_infer_session: Option<bool>,
    /// Optional JSON object for experimental settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_json: Option<serde_json::Value>,
    /// Policy knobs for determinism and routing control
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_policy: Option<DeterminismPolicyKnobs>,
    /// Policy for serving codebase adapters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codebase_serving_policy: Option<CodebaseServingPolicy>,
}
