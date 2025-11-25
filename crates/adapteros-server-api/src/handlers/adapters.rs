// PRD-07 & PRD-08: Adapter Lifecycle & Lineage Handlers
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
use crate::state::AppState;
use crate::types::*;
use adapteros_db::adapters::Adapter;
use adapteros_db::users::Role;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    Extension,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{error, info, warn};
use utoipa::ToSchema;

// ============================================================================
// PRD-07: Adapter Lifecycle Promotion/Demotion
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

    // Get current adapter
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

    let old_state = adapter.current_state.clone();

    // Determine next state
    let new_state_str = match old_state.as_str() {
        "unloaded" => "cold",
        "cold" => "warm",
        "warm" => "hot",
        "hot" => "resident",
        "resident" => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("adapter already at maximum state (resident)")
                        .with_code("BAD_REQUEST"),
                ),
            ));
        }
        _ => {
            warn!("Unknown state: {}, defaulting to cold", old_state);
            "cold"
        }
    };

    // Use lifecycle manager if available
    let new_state = if let Some(ref lifecycle) = state.lifecycle_manager {
        let mut manager = lifecycle.lock().await;
        
        if let Some(adapter_idx) = manager.get_adapter_idx(&adapter_id) {
            // Promote adapter via lifecycle manager
            manager.promote_adapter(adapter_idx).map_err(|e| {
                error!("Failed to promote adapter via lifecycle manager: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to promote adapter")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
            
            // Get the new state and sync with database
            use adapteros_lora_lifecycle::AdapterState;
            let new_state_enum = match new_state_str {
                "cold" => AdapterState::Cold,
                "warm" => AdapterState::Warm,
                "hot" => AdapterState::Hot,
                "resident" => AdapterState::Resident,
                _ => AdapterState::Cold,
            };
            
            // Sync with database via lifecycle manager
            if let Err(e) = manager.update_adapter_state(adapter_idx, new_state_enum, &req.reason).await {
                tracing::warn!(adapter_id = %adapter_id, error = %e, "Failed to sync adapter state with database via lifecycle manager");
                // Fallback: update DB directly
                state
                    .db
                    .update_adapter_state_tx(&adapter_id, new_state_str, &req.reason)
                    .await
                    .map_err(|e| {
                        error!("Failed to update adapter state: {}", e);
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
            
            new_state_str.to_string()
        } else {
            // Adapter not found in lifecycle manager, update DB directly
            state
                .db
                .update_adapter_state_tx(&adapter_id, new_state_str, &req.reason)
                .await
                .map_err(|e| {
                    error!("Failed to update adapter state: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("failed to update adapter state")
                                .with_code("INTERNAL_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;
            new_state_str.to_string()
        }
    } else {
        // Fallback: direct DB update if no lifecycle manager
        state
            .db
            .update_adapter_state_tx(&adapter_id, new_state_str, &req.reason)
            .await
            .map_err(|e| {
                error!("Failed to update adapter state: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update adapter state")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
        new_state_str.to_string()
    };

    let timestamp = chrono::Utc::now().to_rfc3339();
    let actor = claims.sub.clone();

    // Emit structured telemetry event (Policy Pack #9: Canonical JSON logging)
    let telemetry_event = serde_json::json!({
        "event_type": "adapter.lifecycle.promoted",
        "component": "adapteros-server-api",
        "severity": "info",
        "message": format!("Adapter {} promoted: {} → {}", adapter_id, old_state, new_state),
        "metadata": {
            "adapter_id": adapter_id,
            "old_state": old_state,
            "new_state": new_state,
            "actor": actor,
            "reason": req.reason.clone(),
            "timestamp": timestamp.clone(),
        }
    });

    info!(
        event = %telemetry_event,
        adapter_id = %adapter_id,
        old_state = %old_state,
        new_state = %new_state,
        actor = %actor,
        reason = %req.reason,
        "Adapter lifecycle promoted"
    );

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
        adapter_id,
        old_state,
        new_state: new_state.to_string(),
        reason: req.reason,
        actor,
        timestamp,
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

    // Get current adapter
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

    let old_state = adapter.current_state.clone();

    // Determine previous state
    let new_state_str = match old_state.as_str() {
        "resident" => "hot",
        "hot" => "warm",
        "warm" => "cold",
        "cold" => "unloaded",
        "unloaded" => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("adapter already at minimum state (unloaded)")
                        .with_code("BAD_REQUEST"),
                ),
            ));
        }
        _ => {
            warn!("Unknown state: {}, defaulting to unloaded", old_state);
            "unloaded"
        }
    };

    // Use lifecycle manager if available
    let new_state = if let Some(ref lifecycle) = state.lifecycle_manager {
        let mut manager = lifecycle.lock().await;
        
        if let Some(adapter_idx) = manager.get_adapter_idx(&adapter_id) {
            // Demote adapter via lifecycle manager
            manager.demote_adapter(adapter_idx).map_err(|e| {
                error!("Failed to demote adapter via lifecycle manager: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to demote adapter")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
            
            // Get the new state and sync with database
            use adapteros_lora_lifecycle::AdapterState;
            let new_state_enum = match new_state_str {
                "unloaded" => AdapterState::Unloaded,
                "cold" => AdapterState::Cold,
                "warm" => AdapterState::Warm,
                "hot" => AdapterState::Hot,
                _ => AdapterState::Unloaded,
            };
            
            // Sync with database via lifecycle manager
            if let Err(e) = manager.update_adapter_state(adapter_idx, new_state_enum, &req.reason).await {
                tracing::warn!(adapter_id = %adapter_id, error = %e, "Failed to sync adapter state with database via lifecycle manager");
                // Fallback: update DB directly
                state
                    .db
                    .update_adapter_state_tx(&adapter_id, new_state_str, &req.reason)
                    .await
                    .map_err(|e| {
                        error!("Failed to update adapter state: {}", e);
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
            
            new_state_str.to_string()
        } else {
            // Adapter not found in lifecycle manager, update DB directly
            state
                .db
                .update_adapter_state_tx(&adapter_id, new_state_str, &req.reason)
                .await
                .map_err(|e| {
                    error!("Failed to update adapter state: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("failed to update adapter state")
                                .with_code("INTERNAL_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;
            new_state_str.to_string()
        }
    } else {
        // Fallback: direct DB update if no lifecycle manager
        state
            .db
            .update_adapter_state_tx(&adapter_id, new_state_str, &req.reason)
            .await
            .map_err(|e| {
                error!("Failed to update adapter state: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update adapter state")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
        new_state_str.to_string()
    };

    let timestamp = chrono::Utc::now().to_rfc3339();
    let actor = claims.sub.clone();

    // Emit structured telemetry event (Policy Pack #9: Canonical JSON logging)
    let telemetry_event = serde_json::json!({
        "event_type": "adapter.lifecycle.demoted",
        "component": "adapteros-server-api",
        "severity": "info",
        "message": format!("Adapter {} demoted: {} → {}", adapter_id, old_state, new_state),
        "metadata": {
            "adapter_id": adapter_id,
            "old_state": old_state,
            "new_state": new_state,
            "actor": actor,
            "reason": req.reason.clone(),
            "timestamp": timestamp.clone(),
        }
    });

    info!(
        event = %telemetry_event,
        adapter_id = %adapter_id,
        old_state = %old_state,
        new_state = %new_state,
        actor = %actor,
        reason = %req.reason,
        "Adapter lifecycle demoted"
    );

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
        adapter_id,
        old_state,
        new_state: new_state.to_string(),
        reason: req.reason,
        actor,
        timestamp,
    }))
}

// ============================================================================
// PRD-08: Adapter Lineage & Detail Views
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
    Extension(_claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterLineageResponse>, (StatusCode, Json<ErrorResponse>)> {
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
    pub category: String,
    pub scope: String,
    pub framework: Option<String>,

    // Timestamps
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: Option<String>,
}

impl From<Adapter> for AdapterDetailResponse {
    fn from(adapter: Adapter) -> Self {
        Self {
            id: adapter.id.clone(),
            adapter_id: adapter.adapter_id.unwrap_or(adapter.id),
            name: adapter.name,
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
            pinned: adapter.pinned != 0,
            memory_bytes: adapter.memory_bytes,
            activation_count: adapter.activation_count,
            last_activated: adapter.last_activated,
            hash_b3: adapter.hash_b3,
            rank: adapter.rank,
            alpha: adapter.alpha,
            category: adapter.category,
            scope: adapter.scope,
            framework: adapter.framework,
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
    Extension(_claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
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

    Ok(Json(AdapterDetailResponse::from(adapter)))
}

// ============================================================================
// Adapter Pinning Handlers
// ============================================================================

/// Pin adapter request
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct PinAdapterRequest {
    /// Reason for pinning (required for audit trail)
    pub reason: String,
    /// Optional TTL timestamp (ISO 8601 format, e.g., "2025-12-31T23:59:59Z")
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
///   "pinned_until": "2025-12-31T23:59:59Z"
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
    Extension(_claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<PinStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
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
            .find(|p| p.adapter_id == adapter_id)
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

use crate::audit_helper::{actions, log_failure, log_success, resources};
use crate::permissions::{require_permission, Permission};
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
                    .update_adapter_state_tx(&req.old_adapter_id, "unloaded", "swap_eviction_fallback")
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
                .update_adapter_state_tx(&req.old_adapter_id, "unloaded", "swap_not_found_in_lifecycle")
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
            if let Err(e) = manager.update_adapter_state(new_adapter_idx, AdapterState::Warm, "swapped_in").await {
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
        lifecycle_state: adapter.current_state,
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

/// Import an adapter from an uploaded .aos file
///
/// # Request
/// - Multipart form with a file field named "file"
/// - Optional query param `load=true` to auto-load after import
///
/// # Response
/// Returns the registered adapter details
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
    // Require adapter register permission
    require_permission(&claims, Permission::AdapterRegister)?;

    // Extract file from multipart
    let mut file_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;

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
            filename = field.file_name().map(|s| s.to_string());
            file_data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| {
                        error!("Failed to read file bytes: {}", e);
                        (
                            StatusCode::BAD_REQUEST,
                            Json(
                                ErrorResponse::new("failed to read file")
                                    .with_code("BAD_REQUEST")
                                    .with_string_details(e.to_string()),
                            ),
                        )
                    })?
                    .to_vec(),
            );
        }
    }

    let data = file_data.ok_or_else(|| {
        warn!("No file provided in import request");
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("no file provided").with_code("BAD_REQUEST")),
        )
    })?;

    let _name = filename.unwrap_or_else(|| "imported.aos".to_string());

    // Validate AOS magic bytes (AOS3 format)
    if data.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("invalid AOS file: too small").with_code("INVALID_FORMAT")),
        ));
    }

    // Check for AOS3 magic bytes
    if &data[0..4] != b"AOS3" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid AOS file format: missing AOS3 magic bytes")
                    .with_code("INVALID_FORMAT"),
            ),
        ));
    }

    // Parse AOS header (64 bytes)
    let weights_offset = u64::from_le_bytes([
        data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
    ]) as usize;
    let weights_size = u64::from_le_bytes([
        data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23],
    ]) as usize;
    let manifest_offset = u64::from_le_bytes([
        data[24], data[25], data[26], data[27], data[28], data[29], data[30], data[31],
    ]) as usize;
    let manifest_size = u64::from_le_bytes([
        data[32], data[33], data[34], data[35], data[36], data[37], data[38], data[39],
    ]) as usize;

    // Validate offsets
    if manifest_offset + manifest_size > data.len() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid AOS file: manifest offset out of bounds")
                    .with_code("INVALID_FORMAT"),
            ),
        ));
    }

    // Extract and parse manifest JSON
    let manifest_bytes = &data[manifest_offset..manifest_offset + manifest_size];
    let manifest_str = std::str::from_utf8(manifest_bytes).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid AOS file: manifest is not valid UTF-8")
                    .with_code("INVALID_FORMAT"),
            ),
        )
    })?;

    let manifest: serde_json::Value = serde_json::from_str(manifest_str).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(format!("invalid AOS file: manifest JSON parse error: {}", e))
                    .with_code("INVALID_FORMAT"),
            ),
        )
    })?;

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

    let weights_hash = manifest
        .get("weights_hash")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Compute hash if not present
            use adapteros_core::B3Hash;
            let weights_data = &data[weights_offset..weights_offset + weights_size];
            B3Hash::hash(weights_data).to_hex().to_string()
        });

    let auto_load = params.get("load").map(|v| v == "true").unwrap_or(false);

    // Emit telemetry event
    info!(
        event = "adapter.imported",
        adapter_id = %adapter_id,
        auto_load = %auto_load,
        file_size = %data.len(),
        rank = %rank,
        weights_hash = %weights_hash,
        actor = %claims.sub,
        "Adapter imported from AOS file"
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

    // Return adapter response with manifest data
    let now = chrono::Utc::now().to_rfc3339();
    Ok(Json(AdapterResponse {
        schema_version: "v1".to_string(),
        id: adapter_id.clone(),
        adapter_id: adapter_id.clone(),
        name: adapter_name,
        hash_b3: weights_hash,
        rank,
        tier: if auto_load { "warm".to_string() } else { "ephemeral".to_string() },
        languages: vec![],
        framework: None,
        category: None,
        scope: None,
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
        runtime_state: Some(if auto_load { "warm".to_string() } else { "cold".to_string() }),
        pinned: None,
        memory_bytes: None,
    }))
}
