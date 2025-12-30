// Adapter Lineage & Detail Views
//
// This module provides REST API endpoints for:
// - Adapter lineage tree retrieval (ancestors + descendants)
// - Adapter detail views with full metadata

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use adapteros_db::adapters::Adapter;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Extension,
};
use serde::{Deserialize, Serialize};
use tracing::{error, warn};
use utoipa::ToSchema;

// ============================================================================
// Types
// ============================================================================

/// Lineage node in the adapter tree
#[derive(Debug, Serialize, ToSchema)]
pub struct LineageNode {
    pub adapter_id: String,
    pub adapter_name: Option<String>,
    pub tenant_namespace: Option<String>,
    pub domain: Option<String>,
    pub purpose: Option<String>,
    pub revision: Option<String>,
    pub parent_id: Option<String>,
    pub fork_type: Option<String>,
    pub fork_reason: Option<String>,
    pub current_state: String,
    pub tier: String,
    pub created_at: String,
}

impl From<Adapter> for LineageNode {
    fn from(adapter: Adapter) -> Self {
        Self {
            adapter_id: adapter.adapter_id.unwrap_or(adapter.id),
            adapter_name: adapter.adapter_name,
            tenant_namespace: adapter.tenant_namespace,
            domain: adapter.domain,
            purpose: adapter.purpose,
            revision: adapter.revision,
            parent_id: adapter.parent_id,
            fork_type: adapter.fork_type,
            fork_reason: adapter.fork_reason,
            current_state: adapter.current_state,
            tier: adapter.tier,
            created_at: adapter.created_at,
        }
    }
}

/// Adapter lineage tree response
#[derive(Debug, Serialize, ToSchema)]
pub struct AdapterLineageResponse {
    pub adapter_id: String,
    pub ancestors: Vec<LineageNode>,
    pub self_node: LineageNode,
    pub descendants: Vec<LineageNode>,
    pub total_nodes: usize,
}

/// Adapter detail response with full metadata
#[derive(Debug, Serialize, ToSchema)]
pub struct AdapterDetailResponse {
    // Core identity
    pub id: String,
    pub adapter_id: String,
    pub name: String,
    pub adapter_name: Option<String>,
    pub tenant_id: String,

    // Semantic naming
    pub tenant_namespace: Option<String>,
    pub domain: Option<String>,
    pub purpose: Option<String>,
    pub revision: Option<String>,

    // Lineage
    pub parent_id: Option<String>,
    pub fork_type: Option<String>,
    pub fork_reason: Option<String>,

    // State
    pub current_state: String,
    /// Alias for runtime state (unloaded/cold/warm/hot/resident)
    pub runtime_state: String,
    /// Release lifecycle state (draft/training/ready/active/deprecated/retired/failed)
    pub lifecycle_state: String,
    pub tier: String,
    pub pinned: bool,

    // Metrics
    pub memory_bytes: i64,
    pub activation_count: i64,
    pub last_activated: Option<String>,

    // Metadata
    pub hash_b3: String,
    pub rank: i32,
    pub alpha: f64,
    pub lora_strength: Option<f32>,
    pub category: String,
    pub scope: String,
    pub framework: Option<String>,
    pub base_model_id: Option<String>,
    pub manifest_schema_version: Option<String>,
    pub content_hash_b3: Option<String>,
    /// Basic signature/compliance flag (true when adapter has a recorded content hash)
    pub signature_valid: bool,
    /// True when SQL + KV records match required hashes
    pub kv_consistent: bool,
    /// Human-readable reason when KV is not consistent
    pub kv_message: Option<String>,

    // Timestamps
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: Option<String>,

    // Codebase adapter fields (from migration 0261)
    /// Adapter classification: "standard" (portable), "codebase" (stream-scoped), "core" (baseline)
    pub adapter_type: Option<String>,
    /// Base adapter ID for codebase adapters (the core adapter they extend as delta)
    pub base_adapter_id: Option<String>,
    /// Exclusive session binding for codebase adapters
    pub stream_session_id: Option<String>,
    /// Activation threshold for auto-versioning (default: 100)
    pub versioning_threshold: Option<i32>,
    /// BLAKE3 hash of fused CoreML package for deployment verification
    pub coreml_package_hash: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct UpdateAdapterStrengthRequest {
    /// Runtime LoRA strength multiplier (scales adapter effect)
    pub lora_strength: f32,
}

impl From<Adapter> for AdapterDetailResponse {
    fn from(adapter: Adapter) -> Self {
        let runtime_state = adapter.current_state.clone();

        Self {
            id: adapter.id.clone(),
            adapter_id: adapter.adapter_id.unwrap_or(adapter.id),
            name: adapter.name,
            adapter_name: adapter.adapter_name,
            tenant_id: adapter.tenant_id,
            tenant_namespace: adapter.tenant_namespace,
            domain: adapter.domain,
            purpose: adapter.purpose,
            revision: adapter.revision,
            parent_id: adapter.parent_id,
            fork_type: adapter.fork_type,
            fork_reason: adapter.fork_reason,
            current_state: adapter.current_state,
            runtime_state,
            lifecycle_state: adapter.lifecycle_state,
            tier: adapter.tier,
            pinned: adapter.pinned != 0,
            memory_bytes: adapter.memory_bytes,
            activation_count: adapter.activation_count,
            last_activated: adapter.last_activated,
            hash_b3: adapter.hash_b3,
            rank: adapter.rank,
            alpha: adapter.alpha,
            lora_strength: adapter.lora_strength,
            category: adapter.category,
            scope: adapter.scope,
            framework: adapter.framework,
            base_model_id: adapter.base_model_id,
            manifest_schema_version: adapter.manifest_schema_version,
            content_hash_b3: adapter.content_hash_b3.clone(),
            signature_valid: adapter.content_hash_b3.is_some(),
            kv_consistent: false,
            kv_message: None,
            created_at: adapter.created_at,
            updated_at: adapter.updated_at,
            expires_at: adapter.expires_at,
            // Codebase adapter fields
            adapter_type: adapter.adapter_type,
            base_adapter_id: adapter.base_adapter_id,
            stream_session_id: adapter.stream_session_id,
            versioning_threshold: adapter.versioning_threshold,
            coreml_package_hash: adapter.coreml_package_hash,
        }
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// Get adapter lineage tree (ancestors + descendants)
///
/// Returns the full lineage tree including:
/// - All ancestors (parent, grandparent, etc.)
/// - The adapter itself
/// - All descendants (children, grandchildren, etc.)
///
/// **Permissions:** Any authenticated user can view lineage.
///
/// # Example
/// ```
/// GET /v1/adapters/{adapter_id}/lineage
/// ```
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/lineage",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter lineage tree", body = AdapterLineageResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn get_adapter_lineage(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterLineageResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterView)?;

    // Verify adapter exists
    let current_adapter = state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to fetch adapter"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve adapter from database")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(format!(
                            "Adapter '{}' metadata could not be loaded for tenant '{}'. This may indicate a temporary database issue. Technical details: {}",
                            adapter_id, claims.tenant_id, e
                        )),
                ),
            )
        })?
        .ok_or_else(|| {
            warn!(adapter_id = %adapter_id, "Adapter not found");
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Adapter not found")
                        .with_code("ADAPTER_NOT_FOUND")
                        .with_string_details(format!(
                            "Adapter '{}' does not exist for tenant '{}'. Verify the adapter ID is correct or list available adapters using GET /v1/adapters",
                            adapter_id, claims.tenant_id
                        )),
                ),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &current_adapter.tenant_id)?;

    // Get full lineage tree
    let lineage_adapters = state
        .db
        .get_adapter_lineage(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to fetch lineage"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve adapter lineage tree")
                        .with_code("LINEAGE_RETRIEVAL_FAILED")
                        .with_string_details(format!(
                            "Lineage tree for adapter '{}' could not be constructed. Parent/child relationships may be corrupted. Technical details: {}",
                            adapter_id, e
                        )),
                ),
            )
        })?;

    // Separate into ancestors, self, and descendants
    let mut ancestors = Vec::new();
    let mut descendants = Vec::new();
    let self_node = LineageNode::from(current_adapter.clone());

    // Build parent chain for current adapter to identify ancestors
    let mut ancestor_ids = std::collections::HashSet::new();
    let mut current_parent = current_adapter.parent_id.clone();
    while let Some(parent_id) = current_parent {
        ancestor_ids.insert(parent_id.clone());
        // Find parent in lineage
        current_parent = lineage_adapters
            .iter()
            .find(|a| a.adapter_id.as_ref() == Some(&parent_id))
            .and_then(|a| a.parent_id.clone());
    }

    // Build descendant chain by checking if current adapter is in their parent chain
    let mut descendant_ids = std::collections::HashSet::new();
    for adapter in &lineage_adapters {
        let adapter_id_str = adapter.adapter_id.clone().unwrap_or(adapter.id.clone());
        if adapter_id_str == adapter_id {
            continue; // Skip self
        }

        // Walk up this adapter's parent chain to see if it includes current adapter
        let mut check_parent = adapter.parent_id.clone();
        while let Some(parent_id) = check_parent {
            if parent_id == adapter_id {
                descendant_ids.insert(adapter_id_str.clone());
                break;
            }
            check_parent = lineage_adapters
                .iter()
                .find(|a| a.adapter_id.as_ref() == Some(&parent_id))
                .and_then(|a| a.parent_id.clone());
        }
    }

    // Now separate the adapters based on the sets we built
    for adapter in lineage_adapters {
        let adapter_id_str = adapter.adapter_id.clone().unwrap_or(adapter.id.clone());

        if adapter_id_str == adapter_id {
            continue; // Skip self
        }

        if ancestor_ids.contains(&adapter_id_str) {
            ancestors.push(LineageNode::from(adapter));
        } else if descendant_ids.contains(&adapter_id_str) {
            descendants.push(LineageNode::from(adapter));
        }
    }

    // Sort ancestors by creation time (oldest first)
    ancestors.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    // Sort descendants by creation time (newest first)
    descendants.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let total_nodes = 1 + ancestors.len() + descendants.len();

    Ok(Json(AdapterLineageResponse {
        adapter_id,
        ancestors,
        self_node,
        descendants,
        total_nodes,
    }))
}

/// Get adapter detail with full metadata
///
/// Returns comprehensive adapter information including:
/// - Identity and naming metadata
/// - Lineage information (parent, fork type/reason)
/// - Lifecycle state and pinning status
/// - Performance metrics (memory, activation count)
/// - LoRA parameters (rank, alpha, hash)
///
/// **Permissions:** Any authenticated user can view details.
///
/// # Example
/// ```
/// GET /v1/adapters/{adapter_id}/detail
/// ```
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/detail",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter detail", body = AdapterDetailResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn get_adapter_detail(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterView)?;

    let adapter = state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to fetch adapter"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            warn!(adapter_id = %adapter_id, "Adapter not found");
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    let mut response = AdapterDetailResponse::from(adapter.clone());
    if let Ok(status) = state.db.check_adapter_consistency(&adapter_id).await {
        response.kv_consistent = status.is_ready();
        response.kv_message = status.message;
    } else {
        response.kv_consistent = false;
        response.kv_message = Some("KV consistency check failed".to_string());
    }

    Ok(Json(response))
}
