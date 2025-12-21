//! Tenant settings API types
//!
//! Types for the tenant settings endpoints:
//! - GET /v1/tenants/{tenant_id}/settings
//! - PUT /v1/tenants/{tenant_id}/settings

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

/// Policy knobs for determinism and routing control
///
/// These settings control how the inference engine handles determinism modes,
/// backend fallback, and pinned adapter enforcement.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
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
}
