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
use adapteros_db::users::Role;
use adapteros_db::Adapter;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    Extension,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

// ============================================================================
// PRD-07: Adapter Lifecycle Promotion/Demotion
// ============================================================================

/// Lifecycle state transition request
#[derive(Debug, Deserialize, Serialize)]
pub struct LifecycleTransitionRequest {
    /// Reason for the transition (required for audit trail)
    pub reason: String,
}

/// Lifecycle state transition response
#[derive(Debug, Serialize)]
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
    let new_state = match old_state.as_str() {
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

    // Update state in database
    state
        .db
        .update_adapter_state_tx(&adapter_id, new_state, &req.reason)
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
    let new_state = match old_state.as_str() {
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

    // Update state in database
    state
        .db
        .update_adapter_state_tx(&adapter_id, new_state, &req.reason)
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
