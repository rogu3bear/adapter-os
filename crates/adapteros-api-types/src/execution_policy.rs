//! Tenant execution policy API types
//!
//! Hierarchical policy model for determinism and routing enforcement:
//! - TenantExecutionPolicy: top-level container
//! - DeterminismPolicy: allowed modes, seed requirements, fallback behavior
//! - RoutingPolicy: allowed stacks/adapters, pin enforcement
//! - GoldenPolicy: golden-run verification configuration
//!
//! Types for the execution policy endpoints:
//! - GET /v1/tenants/{tenant_id}/execution-policy
//! - POST /v1/tenants/{tenant_id}/execution-policy
//! - PUT /v1/tenants/{tenant_id}/execution-policy
//! - DELETE /v1/tenants/{tenant_id}/execution-policy

use adapteros_core::backend::BackendKind;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

/// Determinism policy configuration
///
/// Controls which determinism modes are allowed for inference requests
/// and how strict mode constraints are enforced.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DeterminismPolicy {
    /// Allowed determinism modes for this tenant.
    /// Valid values: "strict", "besteffort", "relaxed"
    /// Empty array means all modes are allowed (permissive default).
    #[serde(default)]
    pub allowed_modes: Vec<String>,

    /// Optional allowlist of permitted backends for inference.
    /// When present, requests for other backends will be rejected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_backends: Option<Vec<BackendKind>>,

    /// Optional denylist of backends; takes precedence over allowlist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denied_backends: Option<Vec<BackendKind>>,

    /// Default mode when not specified in inference request.
    /// Must be one of the allowed_modes (or any mode if allowed_modes is empty).
    #[serde(default = "default_determinism_mode")]
    pub default_mode: String,

    /// Whether seed is required in strict mode.
    /// When true and mode is "strict", inference requests without seed will be rejected.
    #[serde(default)]
    pub require_seed: bool,

    /// Whether backend fallback is allowed.
    /// When false, inference will fail rather than fall back to a different backend.
    #[serde(default = "default_true")]
    pub allow_fallback: bool,

    /// Expected replay guarantee level: "exact", "approximate", or "none".
    /// Used for documentation and validation, does not directly affect inference.
    #[serde(default = "default_replay_mode")]
    pub replay_mode: String,
}

fn default_determinism_mode() -> String {
    "besteffort".to_string()
}

fn default_replay_mode() -> String {
    "approximate".to_string()
}

fn default_true() -> bool {
    true
}

fn default_epsilon_threshold() -> f64 {
    1e-6
}

impl Default for DeterminismPolicy {
    fn default() -> Self {
        Self {
            allowed_modes: vec![
                "strict".to_string(),
                "besteffort".to_string(),
                "relaxed".to_string(),
            ],
            allowed_backends: None,
            denied_backends: None,
            default_mode: default_determinism_mode(),
            require_seed: false,
            allow_fallback: true,
            replay_mode: default_replay_mode(),
        }
    }
}

/// Routing policy configuration
///
/// Minimal, deterministic constraints applied after router scoring but
/// before kernels run. Designed to express per-tenant/workspace adapter
/// allowlists/denylists and an optional per-token adapter cap without
/// changing router math.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RoutingPolicy {
    /// Restrict routing to specific stack IDs.
    /// When present, only these stacks may be used.
    /// When None/null, all stacks are allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_stack_ids: Option<Vec<String>>,

    /// Restrict routing to specific adapter IDs.
    /// When present, only these adapters may be routed.
    /// When None/null, all adapters are allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_adapter_ids: Option<Vec<String>>,

    /// Explicitly deny routing to specific adapter IDs.
    /// Takes precedence over allowlist when both are provided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denied_adapter_ids: Option<Vec<String>>,

    /// Maximum number of adapters allowed per token after routing policy is applied.
    /// When None/null, defaults to the router's K value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_adapters_per_token: Option<usize>,

    /// Restrict routing to specific clusters (semantic grouping of adapters).
    /// When present, only these clusters may be used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_clusters: Option<Vec<String>>,

    /// Explicitly deny routing to specific clusters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denied_clusters: Option<Vec<String>>,

    /// Maximum number of cluster transitions (hops) allowed per request.
    /// When exceeded, router applies cluster_fallback behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_reasoning_depth: Option<usize>,

    /// Behavior when a cluster transition is blocked.
    /// - stay_on_current: remain on current adapters
    /// - fallback_to_base: route to base-only
    #[serde(default = "default_cluster_fallback")]
    pub cluster_fallback: String,

    /// How to handle pins outside the effective routing set.
    /// "warn": Log warning but allow inference (default)
    /// "error": Reject inference request
    #[serde(default = "default_pin_enforcement")]
    pub pin_enforcement: String,

    /// Require a stack to be specified in inference requests.
    /// When true, requests without stack_id are rejected.
    #[serde(default)]
    pub require_stack: bool,

    /// Require pinned adapters to be specified.
    /// When true, requests without pinned_adapter_ids are rejected.
    #[serde(default)]
    pub require_pins: bool,
}

fn default_pin_enforcement() -> String {
    "warn".to_string()
}

fn default_cluster_fallback() -> String {
    "stay_on_current".to_string()
}

impl Default for RoutingPolicy {
    fn default() -> Self {
        Self {
            allowed_stack_ids: None,
            allowed_adapter_ids: None,
            denied_adapter_ids: None,
            max_adapters_per_token: None,
            allowed_clusters: None,
            denied_clusters: None,
            max_reasoning_depth: Some(10),
            cluster_fallback: default_cluster_fallback(),
            pin_enforcement: default_pin_enforcement(),
            require_stack: false,
            require_pins: false,
        }
    }
}

/// Golden-run verification policy configuration
///
/// Controls whether and how routing decisions are verified against
/// a golden baseline for drift detection.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct GoldenPolicy {
    /// Whether to fail inference when golden drift is detected.
    /// When false (default), drift is logged but inference proceeds.
    /// When true, inference is rejected on drift detection.
    #[serde(default)]
    pub fail_on_drift: bool,

    /// Golden baseline ID to compare against.
    /// When None/null, golden verification is disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub golden_baseline_id: Option<String>,

    /// Epsilon threshold for floating-point comparison of gate values.
    /// Gate differences within this threshold are not considered drift.
    /// Note: For CI, adapter selection/order changes always fail regardless of epsilon.
    #[serde(default = "default_epsilon_threshold")]
    pub epsilon_threshold: f64,
}

impl Default for GoldenPolicy {
    fn default() -> Self {
        Self {
            fail_on_drift: false,
            golden_baseline_id: None,
            epsilon_threshold: default_epsilon_threshold(),
        }
    }
}

/// Tenant execution policy
///
/// Hierarchical policy containing determinism, routing, and golden verification
/// configuration for a tenant. Loaded at inference time to enforce constraints.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct TenantExecutionPolicy {
    /// Unique policy ID (UUID)
    pub id: String,

    /// Tenant ID this policy belongs to
    pub tenant_id: String,

    /// Policy version for audit trail (increments on update)
    pub version: i64,

    /// Determinism policy configuration
    pub determinism: DeterminismPolicy,

    /// Routing policy configuration (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<RoutingPolicy>,

    /// Golden verification policy configuration (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub golden: Option<GoldenPolicy>,

    /// PRD-ART-01: Require Ed25519 signatures on imported .aos adapter files.
    /// When true, adapter imports without valid signatures will be rejected.
    /// Default: false (unsigned adapters allowed)
    #[serde(default)]
    pub require_signed_adapters: bool,

    /// Whether this policy is active (only one active policy per tenant)
    #[serde(default = "default_true")]
    pub active: bool,

    /// Whether this is an implicit permissive policy (no explicit policy configured)
    /// When true, this policy was auto-generated as a permissive default.
    #[serde(default)]
    pub is_implicit: bool,

    /// When the policy was created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,

    /// When the policy was last updated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,

    /// User who created/updated the policy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

impl TenantExecutionPolicy {
    /// Create a permissive default policy for tenants without explicit configuration.
    /// All modes allowed, no restrictions on routing, no golden verification.
    pub fn permissive_default(tenant_id: &str) -> Self {
        Self {
            id: format!("implicit-{}", tenant_id),
            tenant_id: tenant_id.to_string(),
            version: 0,
            determinism: DeterminismPolicy::default(),
            routing: None,
            golden: None,
            require_signed_adapters: false,
            active: true,
            is_implicit: true,
            created_at: None,
            updated_at: None,
            created_by: None,
        }
    }
}

/// Response for execution policy endpoints
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionPolicyResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,

    /// The execution policy
    pub policy: TenantExecutionPolicy,
}

/// Request to create or update an execution policy
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub struct CreateExecutionPolicyRequest {
    /// Determinism policy configuration
    pub determinism: DeterminismPolicy,

    /// Routing policy configuration (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<RoutingPolicy>,

    /// Golden verification policy configuration (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub golden: Option<GoldenPolicy>,

    /// PRD-ART-01: Require Ed25519 signatures on imported .aos adapter files.
    /// Default: false (unsigned adapters allowed)
    #[serde(default)]
    pub require_signed_adapters: bool,
}

/// Request to update an existing execution policy
///
/// All fields are optional to support partial updates.
/// Fields not provided will preserve existing values.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct UpdateExecutionPolicyRequest {
    /// Determinism policy configuration (replaces existing if provided)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub determinism: Option<DeterminismPolicy>,

    /// Routing policy configuration (replaces existing if provided)
    /// Use null to remove routing policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<Option<RoutingPolicy>>,

    /// Golden verification policy configuration (replaces existing if provided)
    /// Use null to remove golden policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub golden: Option<Option<GoldenPolicy>>,

    /// PRD-ART-01: Require Ed25519 signatures on imported .aos adapter files.
    /// When provided, updates the require_signed_adapters setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_signed_adapters: Option<bool>,
}

/// Execution policy enforcement result
///
/// Returned by the policy enforcement check, includes details about
/// what was checked and whether it passed.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PolicyEnforcementResult {
    /// Policy ID that was enforced
    pub policy_id: String,

    /// Policy version that was enforced
    pub policy_version: i64,

    /// Determinism mode that was resolved and enforced
    pub determinism_mode: String,

    /// Whether routing constraints passed
    pub routing_allowed: bool,

    /// Whether golden verification was performed
    pub golden_check_performed: bool,

    /// Whether golden drift was detected (only set if check was performed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub golden_drift_detected: Option<bool>,
}
