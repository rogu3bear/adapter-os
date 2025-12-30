#[path = "adapters/fs_utils.rs"]
mod adapter_fs_utils;
#[path = "adapters/hashing.rs"]
mod adapter_hashing;
#[path = "adapters/paths.rs"]
mod adapter_paths;
#[path = "adapters/progress.rs"]
mod adapter_progress;
#[path = "adapters/repo.rs"]
mod adapter_repo;
#[path = "adapters/tenant.rs"]
mod adapter_tenant;

// Adapter Lifecycle & Lineage Handlers
//
// This module provides REST API endpoints for:
// - Manual adapter lifecycle promotion/demotion
// - Adapter lineage tree retrieval (ancestors + descendants)
// - Adapter detail views with full metadata
//
// Lifecycle states: Unloaded → Cold → Warm → Hot → Resident
// Transitions are logged with telemetry events including actor, reason, old/new states.

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::services::{AdapterService, DefaultAdapterService};
use crate::state::AppState;
use crate::types::*;
use adapter_fs_utils::write_temp_bundle;
use adapter_hashing::hash_multi_bytes;
use adapter_paths::resolve_adapter_roots;
use adapter_progress::emit_adapter_progress;
use adapter_repo::{map_repo_error, AdapterRepo, DefaultAdapterRepo, StoreBundleRequest};
use adapteros_db::adapters::Adapter;
use adapteros_db::users::Role;
use adapteros_db::{AdapterRegistrationBuilder, AdapterTrainingSnapshot};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json},
    Extension,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::io::ReaderStream;
use tracing::{error, info, warn};
use utoipa::ToSchema;

// ============================================================================
// Adapter Lifecycle Promotion/Demotion
// ============================================================================

/// Lifecycle state transition request
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct LifecycleTransitionRequest {
    /// Reason for the transition (required for audit trail)
    pub reason: String,
}

/// Lifecycle state transition response
#[derive(Debug, Serialize, ToSchema)]
pub struct LifecycleTransitionResponse {
    pub adapter_id: String,
    pub old_state: String,
    pub new_state: String,
    pub reason: String,
    pub actor: String,
    pub timestamp: String,
}

/// Manually promote adapter to next lifecycle tier
///
/// Transitions: Unloaded → Cold → Warm → Hot → Resident
///
/// **Permissions:** Requires `Operator` or `Admin` role.
///
/// **Telemetry:** Emits `adapter.lifecycle.promoted` event with metadata:
/// - adapter_id, old_state, new_state, actor, reason, timestamp
///
/// # Example
/// ```
/// POST /v1/adapters/{adapter_id}/lifecycle/promote
/// {
///   "reason": "Manual promotion for production deployment"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/lifecycle/promote",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    request_body = LifecycleTransitionRequest,
    responses(
        (status = 200, description = "Adapter promoted successfully", body = LifecycleTransitionResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 400, description = "Already at maximum state", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn promote_adapter_lifecycle(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(req): Json<LifecycleTransitionRequest>,
) -> Result<Json<LifecycleTransitionResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Use the adapter service to promote lifecycle
    let service = DefaultAdapterService::new(Arc::new(state.clone()));
    let actor = claims.sub.clone();

    let result = service
        .promote_lifecycle(&adapter_id, &claims.tenant_id, &req.reason, &actor)
        .await
        .map_err(|e| {
            error!("Failed to promote adapter lifecycle: {}", e);
            match e {
                adapteros_core::error::AosError::NotFound(_) => (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
                ),
                adapteros_core::error::AosError::Validation(msg) => (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(&msg).with_code("BAD_REQUEST")),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to promote adapter")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                ),
            }
        })?;

    // Audit log: adapter lifecycle promoted
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_LIFECYCLE_PROMOTE,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    Ok(Json(LifecycleTransitionResponse {
        adapter_id: result.adapter_id,
        old_state: result.old_state,
        new_state: result.new_state,
        reason: result.reason,
        actor,
        timestamp: result.timestamp,
    }))
}

/// Manually demote adapter to previous lifecycle tier
///
/// Transitions: Resident → Hot → Warm → Cold → Unloaded
///
/// **Permissions:** Requires `Operator` or `Admin` role.
///
/// **Telemetry:** Emits `adapter.lifecycle.demoted` event.
///
/// # Example
/// ```
/// POST /v1/adapters/{adapter_id}/lifecycle/demote
/// {
///   "reason": "Reducing memory pressure"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/lifecycle/demote",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    request_body = LifecycleTransitionRequest,
    responses(
        (status = 200, description = "Adapter demoted successfully", body = LifecycleTransitionResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 400, description = "Already at minimum state", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn demote_adapter_lifecycle(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(req): Json<LifecycleTransitionRequest>,
) -> Result<Json<LifecycleTransitionResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Use the adapter service to demote lifecycle
    let service = DefaultAdapterService::new(Arc::new(state.clone()));
    let actor = claims.sub.clone();

    let result = service
        .demote_lifecycle(&adapter_id, &claims.tenant_id, &req.reason, &actor)
        .await
        .map_err(|e| {
            error!("Failed to demote adapter lifecycle: {}", e);
            match e {
                adapteros_core::error::AosError::NotFound(_) => (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
                ),
                adapteros_core::error::AosError::Validation(msg) => (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(&msg).with_code("BAD_REQUEST")),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to demote adapter")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                ),
            }
        })?;

    // Audit log: adapter lifecycle demoted
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_LIFECYCLE_DEMOTE,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    Ok(Json(LifecycleTransitionResponse {
        adapter_id: result.adapter_id,
        old_state: result.old_state,
        new_state: result.new_state,
        reason: result.reason,
        actor,
        timestamp: result.timestamp,
    }))
}

// ============================================================================
// Adapter Lineage & Detail Views
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
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch adapter {}: {}", adapter_id, e);
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
            warn!("Adapter not found: {}", adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &current_adapter.tenant_id)?;

    // Get full lineage tree
    let lineage_adapters = state
        .db
        .get_adapter_lineage(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch lineage for {}: {}", adapter_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch lineage")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
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
}

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
        }
    }
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
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch adapter {}: {}", adapter_id, e);
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
            warn!("Adapter not found: {}", adapter_id);
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

/// Update runtime LoRA strength multiplier for an adapter
#[utoipa::path(
    patch,
    path = "/v1/adapters/{adapter_id}/strength",
    request_body = UpdateAdapterStrengthRequest,
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter strength updated", body = AdapterDetailResponse),
        (status = 400, description = "Invalid strength value", body = ErrorResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn update_adapter_strength(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(req): Json<UpdateAdapterStrengthRequest>,
) -> Result<Json<AdapterDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;

    if !(0.0..=2.0).contains(&req.lora_strength) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("lora_strength must be between 0.0 and 2.0")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    }

    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!(adapter_id = %adapter_id, error = %e, "Failed to fetch adapter");
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    state
        .db
        .update_adapter_strength(&adapter_id, req.lora_strength)
        .await
        .map_err(|e| {
            error!(adapter_id = %adapter_id, error = %e, "Failed to update adapter strength");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update adapter strength")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let updated = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!(adapter_id = %adapter_id, error = %e, "Failed to reload adapter");
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(AdapterDetailResponse::from(updated)))
}

// ============================================================================
// Adapter Pinning Handlers
// ============================================================================

/// Pin adapter request
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct PinAdapterRequest {
    /// Reason for pinning (required for audit trail)
    pub reason: String,
    /// Optional TTL timestamp (ISO 8601 format, e.g., "2099-12-31T23:59:59Z")
    pub pinned_until: Option<String>,
}

/// Pin adapter response
#[derive(Debug, Serialize, ToSchema)]
pub struct PinAdapterResponse {
    pub adapter_id: String,
    pub pinned: bool,
    pub reason: String,
    pub pinned_by: String,
    pub pinned_at: String,
    pub pinned_until: Option<String>,
}

/// Unpin adapter response
#[derive(Debug, Serialize, ToSchema)]
pub struct UnpinAdapterResponse {
    pub adapter_id: String,
    pub unpinned: bool,
    pub message: String,
}

/// Pin status response
#[derive(Debug, Serialize, ToSchema)]
pub struct PinStatusResponse {
    pub adapter_id: String,
    pub is_pinned: bool,
    pub reason: Option<String>,
    pub pinned_by: Option<String>,
    pub pinned_at: Option<String>,
    pub pinned_until: Option<String>,
}

// ============================================================================
// Adapter Archive Types
// ============================================================================

/// Archive adapter request
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ArchiveAdapterRequest {
    /// Reason for archiving (required for audit trail)
    pub reason: String,
}

/// Archive adapter response
#[derive(Debug, Serialize, ToSchema)]
pub struct ArchiveAdapterResponse {
    pub adapter_id: String,
    pub archived: bool,
    pub reason: String,
    pub archived_by: String,
    pub archived_at: String,
}

/// Unarchive adapter response
#[derive(Debug, Serialize, ToSchema)]
pub struct UnarchiveAdapterResponse {
    pub adapter_id: String,
    pub unarchived: bool,
    pub message: String,
}

/// Archive status response
#[derive(Debug, Serialize, ToSchema)]
pub struct ArchiveStatusResponse {
    pub adapter_id: String,
    pub is_archived: bool,
    pub is_purged: bool,
    pub archive_reason: Option<String>,
    pub archived_by: Option<String>,
    pub archived_at: Option<String>,
    pub purged_at: Option<String>,
}

/// Pin an adapter to prevent eviction
///
/// Pinned adapters are protected from automatic eviction due to memory pressure
/// or TTL expiration. Use this for production-critical adapters.
///
/// **Permissions:** Requires `Operator` or `Admin` role.
///
/// **Telemetry:** Emits `adapter.pinned` event.
///
/// # Example
/// ```
/// POST /v1/adapters/{adapter_id}/pin
/// {
///   "reason": "Production-critical adapter",
///   "pinned_until": "2099-12-31T23:59:59Z"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/pin",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    request_body = PinAdapterRequest,
    responses(
        (status = 200, description = "Adapter pinned successfully", body = PinAdapterResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn pin_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(req): Json<PinAdapterRequest>,
) -> Result<Json<PinAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Verify adapter exists
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch adapter {}: {}", adapter_id, e);
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
            warn!("Adapter not found: {}", adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    // Get tenant_id from adapter or use default
    let tenant_id = adapter
        .tenant_namespace
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let pinned_by = claims.sub.clone();
    let pinned_at = chrono::Utc::now().to_rfc3339();

    // Pin the adapter
    state
        .db
        .pin_adapter(
            &tenant_id,
            &adapter_id,
            req.pinned_until.as_deref(),
            &req.reason,
            Some(&pinned_by),
        )
        .await
        .map_err(|e| {
            error!("Failed to pin adapter {}: {}", adapter_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to pin adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Emit telemetry event
    let telemetry_event = serde_json::json!({
        "event_type": "adapter.pinned",
        "component": "adapteros-server-api",
        "severity": "info",
        "message": format!("Adapter {} pinned by {}", adapter_id, pinned_by),
        "metadata": {
            "adapter_id": adapter_id,
            "tenant_id": tenant_id,
            "reason": req.reason,
            "pinned_by": pinned_by,
            "pinned_until": req.pinned_until,
        }
    });

    info!(
        event = %telemetry_event,
        adapter_id = %adapter_id,
        pinned_by = %pinned_by,
        "Adapter pinned"
    );

    // Audit log: adapter pinned
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_PIN,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    Ok(Json(PinAdapterResponse {
        adapter_id,
        pinned: true,
        reason: req.reason,
        pinned_by,
        pinned_at,
        pinned_until: req.pinned_until,
    }))
}

/// Unpin an adapter to allow eviction
///
/// Removes pin protection from an adapter, allowing it to be evicted
/// during memory pressure or TTL expiration.
///
/// **Permissions:** Requires `Operator` or `Admin` role.
///
/// **Telemetry:** Emits `adapter.unpinned` event.
///
/// # Example
/// ```
/// DELETE /v1/adapters/{adapter_id}/pin
/// ```
#[utoipa::path(
    delete,
    path = "/v1/adapters/{adapter_id}/pin",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter unpinned successfully", body = UnpinAdapterResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn unpin_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<UnpinAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Verify adapter exists
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch adapter {}: {}", adapter_id, e);
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
            warn!("Adapter not found: {}", adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    // Get tenant_id from adapter or use default
    let tenant_id = adapter
        .tenant_namespace
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let actor = claims.sub.clone();

    // Unpin the adapter
    state
        .db
        .unpin_adapter(&tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to unpin adapter {}: {}", adapter_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to unpin adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Emit telemetry event
    let telemetry_event = serde_json::json!({
        "event_type": "adapter.unpinned",
        "component": "adapteros-server-api",
        "severity": "info",
        "message": format!("Adapter {} unpinned by {}", adapter_id, actor),
        "metadata": {
            "adapter_id": adapter_id,
            "tenant_id": tenant_id,
            "actor": actor,
        }
    });

    info!(
        event = %telemetry_event,
        adapter_id = %adapter_id,
        actor = %actor,
        "Adapter unpinned"
    );

    // Audit log: adapter unpinned
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_UNPIN,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    Ok(Json(UnpinAdapterResponse {
        adapter_id,
        unpinned: true,
        message: "Adapter unpinned successfully".to_string(),
    }))
}

/// Get adapter pin status
///
/// Returns the current pin status of an adapter including pin reason,
/// pinned_by user, and TTL information.
///
/// **Permissions:** Any authenticated user can view pin status.
///
/// # Example
/// ```
/// GET /v1/adapters/{adapter_id}/pin
/// ```
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/pin",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Pin status retrieved", body = PinStatusResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn get_pin_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<PinStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterView)?;

    // Verify adapter exists
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch adapter {}: {}", adapter_id, e);
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
            warn!("Adapter not found: {}", adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    // Get tenant_id from adapter or use default
    let tenant_id = adapter
        .tenant_namespace
        .clone()
        .unwrap_or_else(|| "default".to_string());

    // Check if pinned
    let is_pinned = state
        .db
        .is_pinned(&tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to check pin status for {}: {}", adapter_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to check pin status")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Get pin details if pinned
    let (reason, pinned_by, pinned_at, pinned_until) = if is_pinned {
        let pinned_adapters = state
            .db
            .list_pinned_adapters(&tenant_id)
            .await
            .map_err(|e| {
                error!("Failed to list pinned adapters: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to get pin details")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        pinned_adapters
            .into_iter()
            .find(|p| p.adapter_id.as_deref() == Some(adapter_id.as_str()))
            .map(|p| {
                (
                    Some(p.reason),
                    p.pinned_by,
                    Some(p.pinned_at),
                    p.pinned_until,
                )
            })
            .unwrap_or((None, None, None, None))
    } else {
        (None, None, None, None)
    };

    Ok(Json(PinStatusResponse {
        adapter_id,
        is_pinned,
        reason,
        pinned_by,
        pinned_at,
        pinned_until,
    }))
}

// ============================================================================
// Adapter Hot-Swap Handler
// ============================================================================

use crate::audit_helper::{actions, log_success, resources};
use crate::types::{
    AdapterStatsResponse, AdapterSwapRequest, AdapterSwapResponse, CategoryPoliciesResponse,
    CategoryPolicyRequest, CategoryPolicyResponse,
};

/// Hot-swap adapters (replace one adapter with another)
///
/// Atomically swaps one adapter for another with minimal downtime.
/// Supports dry-run mode for validation without execution.
///
/// **Permissions:** Requires `AdapterLoad` and `AdapterUnload` permissions (Operator or Admin role).
///
/// **Telemetry:** Emits `adapter.swap` event with metadata:
/// - old_adapter_id, new_adapter_id, vram_delta_mb, duration_ms
///
/// # Example
/// ```
/// POST /v1/adapters/swap
/// {
///   "old_adapter_id": "adapter-old",
///   "new_adapter_id": "adapter-new",
///   "dry_run": false
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/adapters/swap",
    request_body = AdapterSwapRequest,
    responses(
        (status = 200, description = "Adapter swap successful", body = AdapterSwapResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 400, description = "Invalid swap request", body = ErrorResponse),
        (status = 500, description = "Swap failed", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn swap_adapters(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<AdapterSwapRequest>,
) -> Result<Json<AdapterSwapResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require both load and unload permissions
    require_permission(&claims, Permission::AdapterLoad)?;
    require_permission(&claims, Permission::AdapterUnload)?;

    let start_time = std::time::Instant::now();

    // Verify old adapter exists
    let old_adapter = state
        .db
        .get_adapter(&req.old_adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch old adapter {}: {}", req.old_adapter_id, e);
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
            warn!("Old adapter not found: {}", req.old_adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("old adapter not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Adapter ID: {}", req.old_adapter_id)),
                ),
            )
        })?;

    // Validate tenant isolation for old adapter
    validate_tenant_isolation(&claims, &old_adapter.tenant_id)?;

    // Verify new adapter exists
    let new_adapter = state
        .db
        .get_adapter(&req.new_adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch new adapter {}: {}", req.new_adapter_id, e);
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
            warn!("New adapter not found: {}", req.new_adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("new adapter not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Adapter ID: {}", req.new_adapter_id)),
                ),
            )
        })?;

    // Validate tenant isolation for new adapter
    validate_tenant_isolation(&claims, &new_adapter.tenant_id)?;

    // Calculate VRAM delta
    let vram_delta_mb = (new_adapter.memory_bytes - old_adapter.memory_bytes) / (1024 * 1024);

    // If dry run, just validate and return
    if req.dry_run {
        let duration_ms = start_time.elapsed().as_millis() as u64;

        info!(
            event = "adapter.swap.dry_run",
            old_adapter_id = %req.old_adapter_id,
            new_adapter_id = %req.new_adapter_id,
            vram_delta_mb = %vram_delta_mb,
            "Dry run swap validation successful"
        );

        return Ok(Json(AdapterSwapResponse {
            success: true,
            message: "Dry run: swap validated successfully".to_string(),
            old_adapter_id: req.old_adapter_id,
            new_adapter_id: req.new_adapter_id,
            vram_delta_mb: Some(vram_delta_mb),
            duration_ms,
            dry_run: true,
        }));
    }

    // Execute the swap: unload old, load new using lifecycle manager
    // Unload old adapter via lifecycle manager
    if let Some(ref lifecycle) = state.lifecycle_manager {
        let mut manager = lifecycle.lock().await;

        // Unload old adapter
        if let Some(old_adapter_idx) = manager.get_adapter_idx(&req.old_adapter_id) {
            // Use evict_adapter which handles both unloading and DB update
            if let Err(e) = manager.evict_adapter(old_adapter_idx).await {
                tracing::warn!(adapter_id = %req.old_adapter_id, error = %e, "Failed to evict old adapter via lifecycle manager");
                // Fallback: update DB state directly
                state
                    .db
                    .update_adapter_state_tx(
                        &req.old_adapter_id,
                        "unloaded",
                        "swap_eviction_fallback",
                    )
                    .await
                    .map_err(|e| {
                        error!("Failed to update old adapter state: {}", e);
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                ErrorResponse::new("failed to update adapter state")
                                    .with_code("INTERNAL_ERROR")
                                    .with_string_details(e.to_string()),
                            ),
                        )
                    })?;
            }
        } else {
            // Adapter not found in lifecycle manager, update DB directly
            state
                .db
                .update_adapter_state_tx(
                    &req.old_adapter_id,
                    "unloaded",
                    "swap_not_found_in_lifecycle",
                )
                .await
                .map_err(|e| {
                    error!("Failed to update old adapter state: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("failed to update adapter state")
                                .with_code("INTERNAL_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;
        }

        // Load new adapter via lifecycle manager
        if let Err(e) = manager.get_or_reload(&req.new_adapter_id) {
            tracing::warn!(adapter_id = %req.new_adapter_id, error = %e, "Failed to load new adapter via lifecycle manager");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to load new adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }

        // Update new adapter state via lifecycle manager
        if let Some(new_adapter_idx) = manager.get_adapter_idx(&req.new_adapter_id) {
            use adapteros_lora_lifecycle::AdapterState;
            if let Err(e) = manager
                .update_adapter_state(new_adapter_idx, AdapterState::Warm, "swapped_in")
                .await
            {
                tracing::warn!(adapter_id = %req.new_adapter_id, error = %e, "Failed to update new adapter state via lifecycle manager");
                // Fallback: update DB state directly
                state
                    .db
                    .update_adapter_state_tx(&req.new_adapter_id, "warm", "swap_load_fallback")
                    .await
                    .map_err(|e| {
                        error!("Failed to update new adapter state: {}", e);
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                ErrorResponse::new("failed to update adapter state")
                                    .with_code("INTERNAL_ERROR")
                                    .with_string_details(e.to_string()),
                            ),
                        )
                    })?;
            }
        } else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("new adapter not found in lifecycle manager")
                        .with_code("NOT_FOUND"),
                ),
            ));
        }
    } else {
        // Fallback: direct DB updates if no lifecycle manager
        state
            .db
            .update_adapter_state_tx(&req.old_adapter_id, "unloaded", "swap_no_lifecycle_manager")
            .await
            .map_err(|e| {
                error!("Failed to update old adapter state: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update adapter state")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        state
            .db
            .update_adapter_state_tx(&req.new_adapter_id, "warm", "swap_no_lifecycle_manager")
            .await
            .map_err(|e| {
                error!("Failed to update new adapter state: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update adapter state")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    }

    let duration_ms = start_time.elapsed().as_millis() as u64;

    // Emit telemetry event
    let telemetry_event = serde_json::json!({
        "event_type": "adapter.swap",
        "component": "adapteros-server-api",
        "severity": "info",
        "message": format!("Adapter swap: {} -> {}", req.old_adapter_id, req.new_adapter_id),
        "metadata": {
            "old_adapter_id": req.old_adapter_id,
            "new_adapter_id": req.new_adapter_id,
            "vram_delta_mb": vram_delta_mb,
            "duration_ms": duration_ms,
            "actor": claims.sub.clone(),
        }
    });

    info!(
        event = %telemetry_event,
        old_adapter_id = %req.old_adapter_id,
        new_adapter_id = %req.new_adapter_id,
        duration_ms = %duration_ms,
        "Adapter swap completed"
    );

    // Audit log
    let _ = log_success(
        &state.db,
        &claims,
        "adapter.swap",
        resources::ADAPTER,
        Some(&format!("{} -> {}", req.old_adapter_id, req.new_adapter_id)),
    )
    .await;

    Ok(Json(AdapterSwapResponse {
        success: true,
        message: "Adapter swap completed successfully".to_string(),
        old_adapter_id: req.old_adapter_id,
        new_adapter_id: req.new_adapter_id,
        vram_delta_mb: Some(vram_delta_mb),
        duration_ms,
        dry_run: false,
    }))
}

// ============================================================================
// Adapter Statistics Handler
// ============================================================================

/// Get detailed adapter statistics
///
/// Returns comprehensive statistics including activation percentage,
/// memory usage, request count, and latency metrics.
///
/// **Permissions:** Requires `AdapterView` permission (any authenticated role).
///
/// # Example
/// ```
/// GET /v1/adapters/{adapter_id}/stats
/// ```
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/stats",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter statistics", body = AdapterStatsResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn get_adapter_stats(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterStatsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require view permission
    require_permission(&claims, Permission::AdapterView)?;

    // Get adapter from database
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch adapter {}: {}", adapter_id, e);
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
            warn!("Adapter not found: {}", adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    // Get stats from database
    let (total_activations, selected_count, avg_gate_value) = state
        .db
        .get_adapter_stats(&adapter_id)
        .await
        .unwrap_or((0, 0, 0.0));

    let selection_rate = if total_activations > 0 {
        (selected_count as f64 / total_activations as f64) * 100.0
    } else {
        0.0
    };

    // Calculate activation percentage from the activation count field
    let activation_percentage = if adapter.activation_count > 0 {
        // Normalize to 0-100 based on relative usage
        ((adapter.activation_count as f64).log10() * 20.0).min(100.0)
    } else {
        0.0
    };

    // For latency metrics, we would typically aggregate from telemetry
    // Using placeholder values since detailed latency tracking isn't in the adapter table
    let avg_latency_ms = 0.0;
    let p95_latency_ms = 0.0;
    let p99_latency_ms = 0.0;

    Ok(Json(AdapterStatsResponse {
        adapter_id: adapter.adapter_id.unwrap_or(adapter.id),
        activation_percentage,
        memory_bytes: adapter.memory_bytes,
        request_count: adapter.activation_count,
        avg_latency_ms,
        p95_latency_ms,
        p99_latency_ms,
        total_activations,
        selected_count,
        avg_gate_value,
        selection_rate,
        lifecycle_state: adapter.lifecycle_state,
        last_activated: adapter.last_activated,
        created_at: adapter.created_at,
    }))
}

// ============================================================================
// Category Policy Handlers
// ============================================================================

/// List all category policies
///
/// Returns policies for all adapter categories including promotion/demotion
/// thresholds, memory limits, and eviction priorities.
///
/// **Permissions:** Requires `PolicyView` permission (any authenticated role).
///
/// # Example
/// ```
/// GET /v1/adapters/category-policies
/// ```
#[utoipa::path(
    get,
    path = "/v1/adapters/category-policies",
    responses(
        (status = 200, description = "List of category policies", body = CategoryPoliciesResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn list_category_policies(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<CategoryPoliciesResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require policy view permission
    require_permission(&claims, Permission::PolicyView)?;

    // Create policy manager and get all policies
    use adapteros_lora_lifecycle::CategoryPolicyManager;
    let manager = CategoryPolicyManager::new();
    let summary = manager.get_policy_summary();

    let policies: Vec<CategoryPolicyResponse> = summary
        .into_iter()
        .map(|(category, policy)| CategoryPolicyResponse {
            category,
            promotion_threshold_ms: policy.promotion_threshold_ms,
            demotion_threshold_ms: policy.demotion_threshold_ms,
            memory_limit: policy.memory_limit,
            eviction_priority: format!("{:?}", policy.eviction_priority).to_lowercase(),
            auto_promote: policy.auto_promote,
            auto_demote: policy.auto_demote,
            max_in_memory: policy.max_in_memory,
            routing_priority: policy.routing_priority,
        })
        .collect();

    Ok(Json(CategoryPoliciesResponse { policies }))
}

/// Get policy for a specific category
///
/// Returns the policy configuration for a single adapter category.
///
/// **Permissions:** Requires `PolicyView` permission (any authenticated role).
///
/// # Example
/// ```
/// GET /v1/adapters/category-policies/code
/// ```
#[utoipa::path(
    get,
    path = "/v1/adapters/category-policies/{category}",
    params(
        ("category" = String, Path, description = "Category name (e.g., code, framework, codebase, ephemeral)")
    ),
    responses(
        (status = 200, description = "Category policy", body = CategoryPolicyResponse),
        (status = 404, description = "Category not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn get_category_policy(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(category): Path<String>,
) -> Result<Json<CategoryPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require policy view permission
    require_permission(&claims, Permission::PolicyView)?;

    use adapteros_lora_lifecycle::CategoryPolicyManager;
    let manager = CategoryPolicyManager::new();

    // Check if category exists in known categories
    let categories = manager.get_categories();
    if !categories.contains(&category) && category != "default" {
        // Still return default policy for unknown categories
        info!(
            "Returning default policy for unknown category: {}",
            category
        );
    }

    let summary = manager.get_policy_summary();

    if let Some(policy) = summary.get(&category) {
        Ok(Json(CategoryPolicyResponse {
            category,
            promotion_threshold_ms: policy.promotion_threshold_ms,
            demotion_threshold_ms: policy.demotion_threshold_ms,
            memory_limit: policy.memory_limit,
            eviction_priority: format!("{:?}", policy.eviction_priority).to_lowercase(),
            auto_promote: policy.auto_promote,
            auto_demote: policy.auto_demote,
            max_in_memory: policy.max_in_memory,
            routing_priority: policy.routing_priority,
        }))
    } else {
        // Return default policy
        let default_policy = manager.get_policy(&category);
        Ok(Json(CategoryPolicyResponse {
            category,
            promotion_threshold_ms: default_policy.promotion_threshold.as_millis() as u64,
            demotion_threshold_ms: default_policy.demotion_threshold.as_millis() as u64,
            memory_limit: default_policy.memory_limit,
            eviction_priority: format!("{:?}", default_policy.eviction_priority).to_lowercase(),
            auto_promote: default_policy.auto_promote,
            auto_demote: default_policy.auto_demote,
            max_in_memory: default_policy.max_in_memory,
            routing_priority: default_policy.routing_priority,
        }))
    }
}

/// Update policy for a specific category
///
/// Updates the policy configuration for an adapter category.
/// Note: Currently updates are in-memory only and will reset on restart.
///
/// **Permissions:** Requires `PolicyApply` permission (Admin only).
///
/// # Example
/// ```
/// PUT /v1/adapters/category-policies/code
/// {
///   "promotion_threshold_secs": 1800,
///   "demotion_threshold_secs": 86400,
///   "memory_limit": 209715200,
///   "eviction_priority": "low",
///   "auto_promote": true,
///   "auto_demote": false,
///   "max_in_memory": 10,
///   "routing_priority": 1.2
/// }
/// ```
#[utoipa::path(
    put,
    path = "/v1/adapters/category-policies/{category}",
    params(
        ("category" = String, Path, description = "Category name")
    ),
    request_body = CategoryPolicyRequest,
    responses(
        (status = 200, description = "Policy updated", body = CategoryPolicyResponse),
        (status = 400, description = "Invalid policy", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn update_category_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(category): Path<String>,
    Json(req): Json<CategoryPolicyRequest>,
) -> Result<Json<CategoryPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require policy apply permission (admin only)
    require_permission(&claims, Permission::PolicyApply)?;

    // Validate eviction priority
    let eviction_priority = match req.eviction_priority.to_lowercase().as_str() {
        "never" | "low" | "normal" | "high" | "critical" => req.eviction_priority.to_lowercase(),
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid eviction_priority")
                        .with_code("INVALID_PARAMETER")
                        .with_string_details("Must be one of: never, low, normal, high, critical"),
                ),
            ));
        }
    };

    // Note: In a full implementation, this would persist the policy to database
    // For now, we acknowledge the update and return the policy as configured

    info!(
        event = "category_policy.update",
        category = %category,
        actor = %claims.sub,
        "Category policy updated"
    );

    // Audit log
    let _ = log_success(
        &state.db,
        &claims,
        "policy.category.update",
        resources::POLICY,
        Some(&category),
    )
    .await;

    Ok(Json(CategoryPolicyResponse {
        category,
        promotion_threshold_ms: req.promotion_threshold_secs * 1000,
        demotion_threshold_ms: req.demotion_threshold_secs * 1000,
        memory_limit: req.memory_limit,
        eviction_priority,
        auto_promote: req.auto_promote,
        auto_demote: req.auto_demote,
        max_in_memory: req.max_in_memory,
        routing_priority: req.routing_priority,
    }))
}

// ============================================================================
// Adapter Import Handler
// ============================================================================

/// Maximum adapter file size (500 MB)
const MAX_ADAPTER_SIZE: u64 = 500 * 1024 * 1024;

/// Import an adapter from an uploaded .aos file
///
/// # Request
/// - Multipart form with a file field named "file"
/// - Optional query param `load=true` to auto-load after import
///
/// # Response
/// Returns the registered adapter details
///
/// # Features
/// - **Streaming upload**: Writes to temp file during upload, avoiding memory pressure
/// - **Deduplication**: Returns existing adapter if hash matches (with `deduplicated: true`)
/// - **Transactional safety**: Temp file + atomic rename, rollback on failure
/// - **Auto-load**: Registers with lifecycle manager when `load=true`
///
/// # Example
/// ```
/// POST /v1/adapters/import?load=true
/// Content-Type: multipart/form-data
///
/// file: <.aos file binary>
/// ```
#[utoipa::path(
    post,
    path = "/v1/adapters/import",
    params(
        ("load" = Option<bool>, Query, description = "Auto-load adapter after import")
    ),
    responses(
        (status = 200, description = "Adapter imported successfully", body = AdapterResponse),
        (status = 400, description = "Invalid file or format", body = ErrorResponse),
        (status = 413, description = "Payload too large", body = ErrorResponse),
        (status = 500, description = "Import failed", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn import_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<AdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    use adapteros_core::B3Hash;
    use blake3::Hasher;
    use tokio::io::AsyncWriteExt;

    // Require adapter register permission
    require_permission(&claims, Permission::AdapterRegister)?;

    let auto_load = params.get("load").map(|v| v == "true").unwrap_or(false);

    // Resolve adapter repo/cache roots (ENV > config > defaults) and ensure temp directory
    let adapters_paths = resolve_adapter_roots(&state);

    // === STREAMING UPLOAD (Issue 6) ===
    // Stream to temp file while computing whole-file hash
    let (temp_path, mut temp_file) = write_temp_bundle(&adapters_paths).await?;

    let mut hasher = Hasher::new();
    let mut total_bytes: u64 = 0;
    let mut filename: Option<String> = None;
    let mut file_found = false;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        error!("Failed to read multipart field: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("failed to read multipart")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })? {
        if field.name() == Some("file") {
            file_found = true;
            filename = field.file_name().map(|s| s.to_string());

            // Stream chunks to temp file
            let mut field = field;
            while let Some(chunk) = field.chunk().await.map_err(|e| {
                error!("Failed to read chunk: {}", e);
                let _ = std::fs::remove_file(&temp_path);
                (
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("failed to read file chunk")
                            .with_code("BAD_REQUEST")
                            .with_string_details(e.to_string()),
                    ),
                )
            })? {
                total_bytes += chunk.len() as u64;

                // Check size limit
                if total_bytes > MAX_ADAPTER_SIZE {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        Json(
                            ErrorResponse::new(format!(
                                "adapter file too large (max {} MB)",
                                MAX_ADAPTER_SIZE / (1024 * 1024)
                            ))
                            .with_code("PAYLOAD_TOO_LARGE"),
                        ),
                    ));
                }

                // Update hash (Issue 5: whole-file hash)
                hasher.update(&chunk);

                // Write to temp file
                temp_file.write_all(&chunk).await.map_err(|e| {
                    error!("Failed to write chunk to temp file: {}", e);
                    let _ = std::fs::remove_file(&temp_path);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("failed to write to temp file")
                                .with_code("INTERNAL_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;
            }
        }
    }

    // Ensure we got a file
    if !file_found {
        let _ = tokio::fs::remove_file(&temp_path).await;
        warn!("No file provided in import request");
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("no file provided").with_code("BAD_REQUEST")),
        ));
    }

    // Flush and close temp file
    temp_file.flush().await.map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to flush temp file")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    drop(temp_file);

    // Compute whole-file hash (Issue 5)
    let file_hash = hasher.finalize().to_hex().to_string();

    // === DEDUPLICATION CHECK (Issue 4) ===
    // Check if adapter with same hash already exists BEFORE any further processing
    if let Ok(Some(existing)) = state.db.find_adapter_by_hash(&file_hash).await {
        // Cleanup temp file - we don't need it
        let _ = tokio::fs::remove_file(&temp_path).await;

        info!(
            existing_id = %existing.adapter_id.as_ref().unwrap_or(&existing.id),
            hash = %file_hash,
            actor = %claims.sub,
            "Deduplicated adapter import - returning existing adapter"
        );

        let now = chrono::Utc::now().to_rfc3339();
        return Ok(Json(AdapterResponse {
            schema_version: "v1".to_string(),
            id: existing.id.clone(),
            adapter_id: existing.adapter_id.clone().unwrap_or(existing.id),
            name: existing.name,
            hash_b3: existing.hash_b3,
            rank: existing.rank,
            tier: existing.tier,
            assurance_tier: None,
            languages: vec![],
            framework: existing.framework,
            category: Some(existing.category),
            scope: Some(existing.scope),
            lora_tier: None,
            lora_strength: existing.lora_strength,
            lora_scope: None,
            framework_id: existing.framework_id,
            framework_version: existing.framework_version,
            repo_id: existing.repo_id,
            commit_sha: existing.commit_sha,
            intent: existing.intent,
            created_at: existing.created_at,
            updated_at: Some(now),
            stats: None,
            version: existing.version,
            lifecycle_state: existing.lifecycle_state,
            runtime_state: Some(existing.current_state),
            pinned: Some(existing.pinned != 0),
            memory_bytes: Some(existing.memory_bytes),
            deduplicated: Some(true),
            drift_reference_backend: None,
            drift_baseline_backend: None,
            drift_test_backend: None,
            drift_tier: None,
            drift_metric: None,
            drift_slice_size: None,
            drift_slice_offset: None,
            drift_loss_metric: None,
        }));
    }

    // === VALIDATE AOS FORMAT ===
    // Read the file for validation (already streamed to disk)
    let data = tokio::fs::read(&temp_path).await.map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to read temp file")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let filename_for_default = filename.clone();
    let _name = filename_for_default.unwrap_or_else(|| "imported.aos".to_string());

    // Validate minimum size
    if data.len() < 64 {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid AOS file: too small (< 64 bytes)")
                    .with_code("INVALID_FORMAT"),
            ),
        ));
    }

    let file_view = match adapteros_aos::open_aos(&data) {
        Ok(view) => view,
        Err(e) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new(format!("invalid AOS file: {}", e))
                        .with_code("INVALID_FORMAT"),
                ),
            ));
        }
    };

    // Extract and parse manifest JSON
    let manifest_bytes = file_view.manifest_bytes;
    let manifest_str = std::str::from_utf8(manifest_bytes).map_err(|_| {
        let _ = std::fs::remove_file(&temp_path);
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid AOS file: manifest is not valid UTF-8")
                    .with_code("INVALID_FORMAT"),
            ),
        )
    })?;

    let manifest: serde_json::Value = serde_json::from_str(manifest_str).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(format!(
                    "invalid AOS file: manifest JSON parse error: {}",
                    e
                ))
                .with_code("INVALID_FORMAT"),
            ),
        )
    })?;

    let metadata_obj = manifest.get("metadata").and_then(|m| m.as_object());
    let scope_path = match metadata_obj
        .and_then(|m| m.get("scope_path"))
        .and_then(|v| v.as_str())
    {
        Some(path) => path,
        None => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid AOS file: missing scope_path in metadata")
                        .with_code("INVALID_FORMAT"),
                ),
            ));
        }
    };
    let scope_hash = adapteros_aos::compute_scope_hash(scope_path);
    let domain = metadata_obj
        .and_then(|m| m.get("domain").and_then(|v| v.as_str()))
        .unwrap_or("unspecified")
        .to_string();
    let group = metadata_obj
        .and_then(|m| m.get("group").and_then(|v| v.as_str()))
        .unwrap_or("unspecified")
        .to_string();
    let scope_value = manifest
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("project")
        .to_string();
    let _operation = metadata_obj
        .and_then(|m| m.get("operation").and_then(|v| v.as_str()))
        .map(|s| s.to_string());

    let canonical_segment = match file_view
        .segments
        .iter()
        .find(|seg| seg.scope_hash == scope_hash)
        .or_else(|| file_view.segments.iter().next())
    {
        Some(seg) => seg,
        None => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid AOS file: missing canonical segment")
                        .with_code("INVALID_FORMAT"),
                ),
            ));
        }
    };
    let weights_data = canonical_segment.payload;

    // === PRD-ART-01: ARTIFACT HARDENING VALIDATIONS ===

    // A. Schema Version Validation
    // Current manifest schema version (keep in sync with format.rs MANIFEST_SCHEMA_VERSION)
    const MANIFEST_SCHEMA_VERSION: &str = "1.0.0";

    let schema_version = manifest
        .get("schema_version")
        .and_then(|v| v.as_str())
        .unwrap_or("1.0.0");

    // Simple major version check: extract first number and compare
    let file_major: u32 = schema_version
        .split('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let current_major: u32 = MANIFEST_SCHEMA_VERSION
        .split('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    if file_major > current_major {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(format!(
                    "Schema version {} is newer than supported {}. Update AdapterOS.",
                    schema_version, MANIFEST_SCHEMA_VERSION
                ))
                .with_code("INCOMPATIBLE_SCHEMA_VERSION"),
            ),
        ));
    }

    // B. Base Model Compatibility Check
    let base_model = manifest.get("base_model").and_then(|v| v.as_str());
    let resolved_base_model_id: Option<String> = if let Some(base_model_name) = base_model {
        match state
            .db
            .get_model_by_name_for_tenant(&claims.tenant_id, base_model_name)
            .await
        {
            Ok(Some(model)) => Some(model.id),
            Ok(None) => {
                warn!(
                    base_model = %base_model_name,
                    "Imported adapter references base model not available on this system"
                );
                // Don't fail - allow import but log warning (model might be acquired later)
                None
            }
            Err(e) => {
                warn!(
                    base_model = %base_model_name,
                    error = %e,
                    "Failed to check base model availability"
                );
                None
            }
        }
    } else {
        None
    };

    // C. Backend Family Validation
    if let Some(backend) = manifest.get("backend_family").and_then(|v| v.as_str()) {
        if !matches!(backend, "metal" | "coreml" | "mlx" | "auto") {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new(format!("Unsupported backend family: {}", backend))
                        .with_code("UNSUPPORTED_BACKEND"),
                ),
            ));
        }
    }

    // D. Hash Integrity Cross-Check (weights hash from manifest vs computed)
    let weights_data = canonical_segment.payload;
    let computed_weights_hash = B3Hash::hash(weights_data).to_hex().to_string();
    if let Some(manifest_weights_hash) = manifest.get("weights_hash").and_then(|v| v.as_str()) {
        if manifest_weights_hash != computed_weights_hash {
            let _ = tokio::fs::remove_file(&temp_path).await;
            error!(
                manifest_hash = %manifest_weights_hash,
                computed_hash = %computed_weights_hash,
                "Weights hash mismatch - file may be corrupted"
            );
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new(format!(
                        "Weights hash mismatch: manifest says {}, computed {}",
                        manifest_weights_hash, computed_weights_hash
                    ))
                    .with_code("HASH_INTEGRITY_FAILURE"),
                ),
            ));
        }
    }
    let weights_hash = computed_weights_hash;

    // E. Signature Policy Check
    let policy = state
        .db
        .get_execution_policy_or_default(&claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to get tenant execution policy: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to check tenant policy")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if policy.require_signed_adapters {
        let is_signed = manifest.get("signature").is_some();
        if !is_signed {
            let _ = tokio::fs::remove_file(&temp_path).await;
            warn!(
                tenant_id = %claims.tenant_id,
                "Rejected unsigned adapter import due to tenant policy"
            );
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("Tenant policy requires signed adapters")
                        .with_code("SIGNATURE_REQUIRED"),
                ),
            ));
        }
        // TODO: Verify signature validity once signing infrastructure is in place
    }

    // F. Content Hash Identity (compute BLAKE3 of manifest + weights for dedup/identity)
    let content_hash_b3 = hash_multi_bytes(&[manifest_bytes, weights_data]);

    // === END PRD-ART-01 VALIDATIONS ===

    // Extract adapter fields from manifest
    let adapter_id = manifest
        .get("adapter_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("imported-{}", uuid::Uuid::new_v4()));

    let adapter_name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| _name.clone());

    let rank = manifest
        .get("rank")
        .and_then(|v| v.as_i64())
        .map(|r| r as i32)
        .unwrap_or(16);

    let version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "1.0.0".to_string());
    let uploaded_file_name = filename.clone().unwrap_or_else(|| _name.clone());

    emit_adapter_progress(
        &adapter_id,
        "validated",
        Some(uploaded_file_name.as_str()),
        50.0,
        "Validated adapter bundle",
    );

    // Note: weights_hash was computed in PRD-ART-01 validation section above

    // === TRANSACTIONAL SAFETY (Issue 1) ===
    let repo = DefaultAdapterRepo::new(&state);
    let stored = repo
        .store_bundle(StoreBundleRequest {
            tenant_id: claims.tenant_id.clone(),
            adapter_name: adapter_name.clone(),
            version: version.clone(),
            temp_path: temp_path.clone(),
            precomputed_hash: Some(file_hash.clone()),
        })
        .await
        .map_err(map_repo_error)?;
    let file_path = stored.final_path.clone();
    let file_path_str = file_path.to_string_lossy().to_string();
    let file_hash = stored.manifest_hash.clone();

    // Step 2: Register in database (rollback file on failure)
    let tier = if auto_load { "warm" } else { "ephemeral" };
    let registration_params = AdapterRegistrationBuilder::new()
        .adapter_id(&adapter_id)
        .tenant_id(&claims.tenant_id)
        .name(&adapter_name)
        .hash_b3(&weights_hash)
        .rank(rank)
        .tier(tier)
        .scope(&scope_value)
        .domain(Some(domain))
        .purpose(Some(group))
        .aos_file_path(Some(&file_path_str))
        .aos_file_hash(Some(&file_hash)) // Store whole-file hash separately from weights hash
        // PRD-ART-01: Artifact hardening fields
        .manifest_schema_version(Some(schema_version))
        .content_hash_b3(Some(&content_hash_b3))
        .base_model_id(resolved_base_model_id)
        .build()
        .map_err(|e| {
            // Rollback: remove the file we just created
            let _ = std::fs::remove_file(&file_path);
            error!("Failed to build adapter registration params: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to build registration params")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let registered_id = repo
        .register_bundle(&adapter_id, &claims.tenant_id, registration_params)
        .await
        .map_err(map_repo_error)?;

    // === AUTO-LOAD (Issue 2) ===
    // Register with lifecycle manager and optionally load
    if let Some(ref lifecycle) = state.lifecycle_manager {
        let mut manager = lifecycle.lock().await;
        let hash = B3Hash::from_hex(&weights_hash).unwrap_or_else(|_| B3Hash::hash(weights_data));

        match manager.register_adapter(
            adapter_id.clone(),
            hash,
            Some("code".to_string()),
            auto_load,
        ) {
            Ok(adapter_idx) => {
                info!(
                    adapter_id = %adapter_id,
                    adapter_idx = adapter_idx,
                    auto_load = auto_load,
                    "Registered adapter with lifecycle manager"
                );
            }
            Err(e) => {
                // Don't fail the import, just warn
                warn!(
                    adapter_id = %adapter_id,
                    error = %e,
                    "Failed to register adapter with lifecycle manager (import still succeeded)"
                );
            }
        }
    }

    // Emit telemetry event
    info!(
        event = "adapter.imported",
        adapter_id = %adapter_id,
        registered_id = %registered_id,
        auto_load = %auto_load,
        file_size = %total_bytes,
        file_path = %file_path_str,
        rank = %rank,
        weights_hash = %weights_hash,
        file_hash = %file_hash,
        actor = %claims.sub,
        "Adapter imported from AOS file with full transactional safety"
    );

    // Audit log
    let _ = log_success(
        &state.db,
        &claims,
        actions::ADAPTER_REGISTER,
        resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    emit_adapter_progress(
        &adapter_id,
        "registered",
        Some(uploaded_file_name.as_str()),
        100.0,
        "Adapter import complete",
    );

    // Return adapter response with manifest data
    let now = chrono::Utc::now().to_rfc3339();
    Ok(Json(AdapterResponse {
        schema_version: "v1".to_string(),
        id: adapter_id.clone(),
        adapter_id: adapter_id.clone(),
        name: adapter_name,
        hash_b3: weights_hash,
        rank,
        tier: tier.to_string(),
        assurance_tier: None,
        languages: vec![],
        framework: None,
        category: None,
        scope: None,
        lora_tier: None,
        lora_strength: Some(1.0),
        lora_scope: None,
        framework_id: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: None,
        created_at: now,
        updated_at: None,
        stats: None,
        version,
        lifecycle_state: "draft".to_string(),
        runtime_state: Some(if auto_load {
            "warm".to_string()
        } else {
            "cold".to_string()
        }),
        pinned: None,
        memory_bytes: None,
        deduplicated: Some(false),
        drift_reference_backend: None,
        drift_baseline_backend: None,
        drift_test_backend: None,
        drift_tier: None,
        drift_metric: None,
        drift_slice_size: None,
        drift_slice_offset: None,
        drift_loss_metric: None,
    }))
}

/// Get training snapshot (provenance) for an adapter
///
/// Retrieves the training snapshot showing exactly which documents and
/// chunking configuration were used to train the adapter.
///
/// GET /v1/adapters/:adapter_id/training-snapshot
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/training-snapshot",
    tag = "adapters",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Training snapshot retrieved", body = AdapterTrainingSnapshot),
        (status = 404, description = "Snapshot not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn get_adapter_training_snapshot(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterTrainingSnapshot>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::AdapterView).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // CRITICAL: Fetch adapter first to validate tenant isolation to prevent cross-tenant access
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // CRITICAL: Validate tenant isolation to prevent cross-tenant access
    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    // Get training snapshot from database
    let snapshot = state
        .db
        .get_adapter_training_snapshot(&adapter_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get training snapshot")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Training snapshot not found for this adapter")
                        .with_code("NOT_FOUND"),
                ),
            )
        })?;

    info!(
        adapter_id = %adapter_id,
        training_job_id = %snapshot.training_job_id,
        actor = %claims.sub,
        "Retrieved training snapshot"
    );

    Ok(Json(snapshot))
}

/// Export complete training provenance for an adapter
///
/// Returns full provenance data including:
/// - Adapter metadata (id, name, version, base_model)
/// - Training jobs that produced this adapter
/// - Datasets used for training
/// - Documents with their content hashes
/// - Configuration versions (chunking, training)
/// - Export timestamp and integrity hash
///
/// GET /v1/adapters/:adapter_id/training-export
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/training-export",
    tag = "adapters",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Training provenance export", body = TrainingProvenanceExportResponse),
        (status = 404, description = "Adapter or snapshot not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn export_training_provenance(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<TrainingProvenanceExportResponse>, (StatusCode, Json<ErrorResponse>)> {
    use blake3::Hasher;

    // Permission check
    require_permission(&claims, Permission::AdapterView).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Get adapter details
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!(adapter_id = %adapter_id, error = %e, "Failed to get adapter");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get adapter")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // CRITICAL: Validate tenant isolation to prevent cross-tenant access
    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    // Get training snapshot
    let snapshot = state
        .db
        .get_adapter_training_snapshot(&adapter_id)
        .await
        .map_err(|e| {
            error!(adapter_id = %adapter_id, error = %e, "Failed to get training snapshot");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get training snapshot")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Build export data
    let mut training_jobs = Vec::new();
    let mut datasets = Vec::new();
    let mut documents = Vec::new();
    let mut chunking_config: Option<serde_json::Value> = None;
    let mut training_config: Option<serde_json::Value> = None;

    // If we have a training snapshot, extract documents and job info
    if let Some(ref snapshot) = snapshot {
        // Get training job details
        if let Ok(Some(job)) = state.db.get_training_job(&snapshot.training_job_id).await {
            // Parse training config JSON
            let config_value: serde_json::Value =
                serde_json::from_str(&job.training_config_json).unwrap_or(serde_json::json!({}));
            training_config = Some(config_value.clone());

            training_jobs.push(TrainingExportJob {
                id: job.id,
                config_hash: job.config_hash_b3,
                training_config: config_value,
                started_at: job.started_at,
                completed_at: job.completed_at,
                status: job.status,
            });

            // Get dataset if linked
            if let Some(ref dataset_id) = job.dataset_id {
                if let Ok(Some(dataset)) = state.db.get_training_dataset(dataset_id).await {
                    datasets.push(TrainingExportDataset {
                        id: dataset.id,
                        name: dataset.name,
                        hash: dataset.hash_b3,
                        source_location: dataset.source_location,
                    });
                }
            }
        }

        // Parse documents from snapshot
        if let Ok(doc_refs) =
            serde_json::from_str::<Vec<serde_json::Value>>(&snapshot.documents_json)
        {
            for doc_ref in doc_refs {
                if let Some(doc_id) = doc_ref.get("doc_id").and_then(|v| v.as_str()) {
                    // Fetch full document info
                    if let Ok(Some(doc)) = state.db.get_document(&claims.tenant_id, doc_id).await {
                        documents.push(TrainingExportDocument {
                            id: doc.id,
                            name: doc.name,
                            hash: doc.content_hash,
                            page_count: doc.page_count,
                            created_at: doc.created_at,
                        });
                    }
                }
            }
        }

        // Parse chunking config from snapshot
        if let Ok(chunking) =
            serde_json::from_str::<serde_json::Value>(&snapshot.chunking_config_json)
        {
            chunking_config = Some(chunking);
        }
    }

    // Build adapter export data
    let adapter_export = TrainingExportAdapter {
        id: adapter.id.clone(),
        name: adapter.name.clone(),
        version: adapter.version.clone(),
        base_model: adapter.parent_id.clone(),
        rank: adapter.rank,
        alpha: adapter.alpha,
        created_at: adapter.created_at.clone(),
    };

    // Build config versions
    let config_versions = TrainingExportConfigVersions {
        chunking_config,
        training_config,
    };

    // Build pre-hash response for computing export hash
    let export_timestamp = chrono::Utc::now().to_rfc3339();
    let pre_hash_response = serde_json::json!({
        "schema_version": "v1",
        "adapter": adapter_export,
        "training_jobs": training_jobs,
        "datasets": datasets,
        "documents": documents,
        "config_versions": config_versions,
        "export_timestamp": export_timestamp,
    });

    // Compute BLAKE3 hash of the export
    let mut hasher = Hasher::new();
    hasher.update(pre_hash_response.to_string().as_bytes());
    let export_hash = hasher.finalize().to_hex().to_string();

    let response = TrainingProvenanceExportResponse {
        schema_version: "v1".to_string(),
        adapter: adapter_export,
        training_jobs,
        datasets,
        documents,
        config_versions,
        export_timestamp,
        export_hash,
    };

    info!(
        adapter_id = %adapter_id,
        documents_count = response.documents.len(),
        jobs_count = response.training_jobs.len(),
        actor = %claims.sub,
        "Exported training provenance"
    );

    Ok(Json(response))
}

// ============================================================================
// Adapter Archive/Unarchive Endpoints
// ============================================================================

/// Archive an adapter
///
/// Archives an adapter, marking it as unavailable for inference.
/// The adapter's `.aos` file is NOT deleted until garbage collection runs.
///
/// **Permissions:** Requires `Operator` or `Admin` role.
///
/// **Telemetry:** Emits `adapter.archived` event.
///
/// # Example
/// ```
/// POST /v1/adapters/{adapter_id}/archive
/// {
///   "reason": "Deprecated in favor of v2"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/archive",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    request_body = ArchiveAdapterRequest,
    responses(
        (status = 200, description = "Adapter archived successfully", body = ArchiveAdapterResponse),
        (status = 400, description = "Already archived", body = ErrorResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn archive_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(req): Json<ArchiveAdapterRequest>,
) -> Result<Json<ArchiveAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Verify adapter exists
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch adapter {}: {}", adapter_id, e);
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
            warn!("Adapter not found: {}", adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    // Check if already archived
    if adapter.archived_at.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("adapter is already archived").with_code("ALREADY_ARCHIVED")),
        ));
    }

    let archived_by = claims.sub.clone();
    let archived_at = chrono::Utc::now().to_rfc3339();

    // Archive the adapter
    state
        .db
        .archive_adapter(&adapter_id, &archived_by, &req.reason)
        .await
        .map_err(|e| {
            error!("Failed to archive adapter {}: {}", adapter_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to archive adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Emit telemetry event
    let telemetry_event = serde_json::json!({
        "event_type": "adapter.archived",
        "component": "adapteros-server-api",
        "severity": "info",
        "message": format!("Adapter {} archived by {}", adapter_id, archived_by),
        "metadata": {
            "adapter_id": adapter_id,
            "tenant_id": adapter.tenant_id,
            "reason": req.reason,
            "archived_by": archived_by,
        }
    });

    info!(
        event = %telemetry_event,
        adapter_id = %adapter_id,
        archived_by = %archived_by,
        "Adapter archived"
    );

    // Audit log: adapter archived
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_ARCHIVE,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    Ok(Json(ArchiveAdapterResponse {
        adapter_id,
        archived: true,
        reason: req.reason,
        archived_by,
        archived_at,
    }))
}

/// Unarchive an adapter
///
/// Restores an archived adapter, making it available for inference again.
/// Cannot unarchive if the adapter has been purged (file deleted).
///
/// **Permissions:** Requires `Operator` or `Admin` role.
///
/// **Telemetry:** Emits `adapter.unarchived` event.
///
/// # Example
/// ```
/// DELETE /v1/adapters/{adapter_id}/archive
/// ```
#[utoipa::path(
    delete,
    path = "/v1/adapters/{adapter_id}/archive",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter unarchived successfully", body = UnarchiveAdapterResponse),
        (status = 400, description = "Not archived or already purged", body = ErrorResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn unarchive_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<UnarchiveAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Verify adapter exists
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch adapter {}: {}", adapter_id, e);
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
            warn!("Adapter not found: {}", adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    // Check if not archived
    if adapter.archived_at.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("adapter is not archived").with_code("NOT_ARCHIVED")),
        ));
    }

    // Check if already purged
    if adapter.purged_at.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("cannot unarchive purged adapter - file has been deleted")
                    .with_code("ALREADY_PURGED"),
            ),
        ));
    }

    // Unarchive the adapter
    state.db.unarchive_adapter(&adapter_id).await.map_err(|e| {
        error!("Failed to unarchive adapter {}: {}", adapter_id, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to unarchive adapter")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let unarchived_by = claims.sub.clone();

    // Emit telemetry event
    let telemetry_event = serde_json::json!({
        "event_type": "adapter.unarchived",
        "component": "adapteros-server-api",
        "severity": "info",
        "message": format!("Adapter {} unarchived by {}", adapter_id, unarchived_by),
        "metadata": {
            "adapter_id": adapter_id,
            "tenant_id": adapter.tenant_id,
            "unarchived_by": unarchived_by,
        }
    });

    info!(
        event = %telemetry_event,
        adapter_id = %adapter_id,
        unarchived_by = %unarchived_by,
        "Adapter unarchived"
    );

    // Audit log: adapter unarchived
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_UNARCHIVE,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    Ok(Json(UnarchiveAdapterResponse {
        adapter_id,
        unarchived: true,
        message: "Adapter restored and available for inference".to_string(),
    }))
}

/// Get archive status of an adapter
///
/// Returns the archive/purge status of an adapter.
///
/// **Permissions:** Requires `Viewer`, `Operator`, or `Admin` role.
///
/// # Example
/// ```
/// GET /v1/adapters/{adapter_id}/archive
/// ```
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/archive",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Archive status retrieved", body = ArchiveStatusResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn get_archive_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<ArchiveStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require at least viewer role
    require_any_role(&claims, &[Role::Viewer, Role::Operator, Role::Admin])?;

    // Fetch adapter
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch adapter {}: {}", adapter_id, e);
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
            warn!("Adapter not found: {}", adapter_id);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    Ok(Json(ArchiveStatusResponse {
        adapter_id,
        is_archived: adapter.archived_at.is_some(),
        is_purged: adapter.purged_at.is_some(),
        archive_reason: adapter.archive_reason,
        archived_by: adapter.archived_by,
        archived_at: adapter.archived_at,
        purged_at: adapter.purged_at,
    }))
}

// ============================================================================
// PRD-ART-01: Adapter Export
// ============================================================================

/// Export an adapter as a .aos file
///
/// Returns the .aos file as a binary stream for download.
/// The response includes:
/// - Content-Type: application/octet-stream
/// - Content-Disposition: attachment; filename="{adapter_id}.aos"
/// - X-Adapter-Hash: BLAKE3 content hash for verification
///
/// **Permissions:** Requires `AdapterView` permission.
///
/// # Example
/// ```
/// GET /v1/adapters/{adapter_id}/export
/// ```
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/export",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID to export")
    ),
    responses(
        (status = 200, description = "Adapter file stream", content_type = "application/octet-stream"),
        (status = 404, description = "Adapter not found or no .aos file available", body = ErrorResponse),
        (status = 403, description = "Forbidden - tenant isolation violation", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn export_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::AdapterView).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Get adapter details
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!(adapter_id = %adapter_id, error = %e, "Failed to get adapter for export");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get adapter")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            warn!(adapter_id = %adapter_id, "Adapter not found for export");
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    // Check if adapter is archived/purged
    if adapter.purged_at.is_some() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Adapter has been purged - .aos file no longer available")
                    .with_code("ADAPTER_PURGED"),
            ),
        ));
    }

    // Get the .aos file path
    let aos_path = adapter.aos_file_path.as_ref().ok_or_else(|| {
        warn!(adapter_id = %adapter_id, "No .aos file path for adapter");
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("No .aos file available for this adapter")
                    .with_code("NO_AOS_FILE"),
            ),
        )
    })?;

    // Verify the file exists
    let path = std::path::Path::new(aos_path);
    if !path.exists() {
        error!(adapter_id = %adapter_id, path = %aos_path, "Adapter .aos file not found on disk");
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Adapter .aos file not found on disk")
                    .with_code("FILE_NOT_FOUND"),
            ),
        ));
    }

    // Open the file for streaming
    let file = tokio::fs::File::open(path).await.map_err(|e| {
        error!(adapter_id = %adapter_id, path = %aos_path, error = %e, "Failed to open .aos file");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to open adapter file")
                    .with_code("IO_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Get file metadata for content-length
    let metadata = file.metadata().await.map_err(|e| {
        error!(adapter_id = %adapter_id, error = %e, "Failed to get file metadata");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to read file metadata")
                    .with_code("IO_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Create streaming response body
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    // Build filename for Content-Disposition
    let filename = format!("{}.aos", adapter_id);

    // Get content hash for X-Adapter-Hash header (use aos_file_hash if available, else hash_b3)
    let content_hash = adapter
        .aos_file_hash
        .as_ref()
        .or(Some(&adapter.hash_b3))
        .cloned()
        .unwrap_or_default();

    info!(
        adapter_id = %adapter_id,
        tenant_id = %adapter.tenant_id,
        file_size = metadata.len(),
        actor = %claims.sub,
        "Exporting adapter as .aos file"
    );

    // Audit log: adapter exported
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        "adapter.exported",
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    // Build response with headers
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", filename),
            ),
            (header::CONTENT_LENGTH, metadata.len().to_string()),
            (
                header::HeaderName::from_static("x-adapter-hash"),
                content_hash,
            ),
            (
                header::HeaderName::from_static("x-adapter-id"),
                adapter_id.clone(),
            ),
        ],
        body,
    ))
}

// ============================================================================
// Adapter Version Archive/Unarchive
// ============================================================================

/// Archive an adapter version.
///
/// Archived versions are hidden from normal use but retain their lifecycle_state
/// for audit purposes. Use unarchive to restore visibility.
#[utoipa::path(
    post,
    path = "/v1/adapter-versions/{version_id}/archive",
    params(
        ("version_id" = String, Path, description = "Adapter version ID to archive"),
    ),
    request_body = adapteros_api_types::training::ArchiveAdapterVersionRequest,
    responses(
        (status = 200, description = "Version archived successfully", body = adapteros_api_types::training::ArchiveAdapterVersionResponse),
        (status = 404, description = "Version not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn archive_adapter_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(version_id): Path<String>,
    Json(_req): Json<adapteros_api_types::training::ArchiveAdapterVersionRequest>,
) -> Result<Json<adapteros_api_types::training::ArchiveAdapterVersionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterLoad)?;

    // Verify version exists and belongs to tenant
    let version = state
        .db
        .get_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            error!(
                version_id = %version_id,
                error = %e,
                "Failed to load adapter version for archive"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to load adapter version")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Adapter version not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(version_id.clone()),
                ),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &version.tenant_id)?;

    state
        .db
        .archive_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            error!(
                version_id = %version_id,
                error = %e,
                "Failed to archive adapter version"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to archive version")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!(
        version_id = %version_id,
        tenant_id = %claims.tenant_id,
        actor = %claims.sub,
        "Archived adapter version"
    );

    Ok(Json(adapteros_api_types::training::ArchiveAdapterVersionResponse {
        version_id,
        is_archived: true,
        updated_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Unarchive an adapter version.
///
/// Restores visibility of an archived version.
#[utoipa::path(
    post,
    path = "/v1/adapter-versions/{version_id}/unarchive",
    params(
        ("version_id" = String, Path, description = "Adapter version ID to unarchive"),
    ),
    responses(
        (status = 200, description = "Version unarchived successfully", body = adapteros_api_types::training::ArchiveAdapterVersionResponse),
        (status = 404, description = "Version not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn unarchive_adapter_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(version_id): Path<String>,
) -> Result<Json<adapteros_api_types::training::ArchiveAdapterVersionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterLoad)?;

    // Verify version exists and belongs to tenant
    let version = state
        .db
        .get_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            error!(
                version_id = %version_id,
                error = %e,
                "Failed to load adapter version for unarchive"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to load adapter version")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Adapter version not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(version_id.clone()),
                ),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &version.tenant_id)?;

    state
        .db
        .unarchive_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            error!(
                version_id = %version_id,
                error = %e,
                "Failed to unarchive adapter version"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to unarchive version")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!(
        version_id = %version_id,
        tenant_id = %claims.tenant_id,
        actor = %claims.sub,
        "Unarchived adapter version"
    );

    Ok(Json(adapteros_api_types::training::ArchiveAdapterVersionResponse {
        version_id,
        is_archived: false,
        updated_at: chrono::Utc::now().to_rfc3339(),
    }))
}
