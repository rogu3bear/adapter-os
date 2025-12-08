//! Adapter package types

use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

/// Per-adapter strength override within a package
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterStrength {
    pub adapter_id: String,
    /// Optional LoRA strength multiplier [0.0, 1.0]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strength: Option<f32>,
}

/// Package definition returned by the API
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterPackage {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub stack_id: String,
    /// Tags for discovery and grouping
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Adapter strengths bound to this package (one entry per adapter_id)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adapter_strengths: Vec<AdapterStrength>,
    /// Effective adapter IDs from the bound stack
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adapter_ids: Vec<String>,
    /// Determinism mode configured for this package (inherits stack when None)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    /// Routing determinism mode (deterministic/adaptive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_determinism_mode: Option<String>,
    /// Optional domain categorization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Optional scope path filter (e.g., repo/file path)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_path: Option<String>,
    /// Optional scope path prefix for routing hints
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_path_prefix: Option<String>,
    /// Whether the package is installed for the tenant
    #[serde(default)]
    pub installed: bool,
    /// Timestamp of install (if installed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Request to create a package
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct CreatePackageRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Existing stack to bind. If omitted, a stack will be created from `adapters`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    /// Adapters (and optional strengths) to materialize into a stack if stack_id is not provided.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adapters: Vec<AdapterStrength>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_path_prefix: Option<String>,
}

/// Request to update an existing package
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct UpdatePackageRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    /// Replace the adapter set (and optional strengths) by creating/binding a new stack
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<AdapterStrength>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_path_prefix: Option<String>,
}

/// Standard package response wrapper
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PackageResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub package: AdapterPackage,
}

/// Package list response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PackageListResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub packages: Vec<AdapterPackage>,
}
