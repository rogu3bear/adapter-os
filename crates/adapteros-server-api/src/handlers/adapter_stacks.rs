use crate::api_error::ApiError;
use crate::audit_helper::{log_failure_or_warn, log_success_or_warn, resources};
use crate::auth::Claims;
use crate::handlers::guard_in_flight_requests;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use std::str::FromStr;

/// Audit action constants for stack operations
const ACTION_STACK_CREATE: &str = "stack.create";
const ACTION_STACK_DELETE: &str = "stack.delete";
use adapteros_core::B3Hash;
use adapteros_core::StackName;
use adapteros_db::LifecycleHistoryEvent;
use adapteros_lora_worker::memory::MemoryPressureLevel;
use adapteros_lora_worker::signal::{Signal, SignalPriority, SignalType};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use tracing::{debug, info, warn};
use utoipa::ToSchema;

/// Request to create a new adapter stack
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateStackRequest {
    pub name: String,
    pub description: Option<String>,
    pub adapter_ids: Vec<String>,
    pub workflow_type: Option<WorkflowType>,
    pub metadata: Option<HashMap<String, String>>,
    /// Determinism mode for this stack (strict, besteffort, relaxed)
    /// If not specified, inherits tenant execution policy default when set,
    /// otherwise falls back to the global default determinism mode (strict by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    /// Routing determinism mode for this stack (deterministic/adaptive)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
}

/// Response for adapter stack operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StackResponse {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub adapter_ids: Vec<String>,
    pub workflow_type: Option<WorkflowType>,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: bool,
    /// Whether this is the default stack for the tenant
    #[serde(default)]
    pub is_default: bool,
    /// Stack version for telemetry correlation
    pub version: i64,
    /// Lifecycle state: active, deprecated, retired, draft
    pub lifecycle_state: String,
    /// Warnings about capacity or memory pressure (memory guardrails)
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Determinism mode for this stack (strict, besteffort, relaxed)
    /// If not specified, uses global config
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    /// Routing determinism mode for adapter selection
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
}

fn default_schema_version() -> String {
    adapteros_api_types::API_SCHEMA_VERSION.to_string()
}

/// Workflow type for adapter stacks
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowType {
    Parallel,
    UpstreamDownstream,
    Sequential,
}

/// Validates that all adapters in a stack share the same base model.
async fn validate_stack_base_model_compatibility(
    db: &adapteros_db::Db,
    tenant_id: &str,
    adapter_ids: &[String],
) -> Result<Option<String>, (StatusCode, Json<ErrorResponse>)> {
    if adapter_ids.is_empty() {
        return Ok(None);
    }

    let mut expected_base_model: Option<String> = None;

    for adapter_id in adapter_ids {
        let adapter = db
            .get_adapter_for_tenant(tenant_id, adapter_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new(format!("Adapter '{}' not found: {}", adapter_id, e))
                            .with_code("ADAPTER_NOT_FOUND"),
                    ),
                )
            })?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new(format!("Adapter '{}' not found", adapter_id))
                            .with_code("ADAPTER_NOT_FOUND"),
                    ),
                )
            })?;

        match (&expected_base_model, &adapter.base_model_id) {
            (None, Some(model)) => {
                expected_base_model = Some(model.clone());
            }
            (None, None) => {
                tracing::warn!(adapter_id = %adapter_id, "Adapter missing base_model_id");
            }
            (Some(expected), Some(actual)) if expected != actual => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(format!(
                        "Stack adapters must target same base model. Expected '{}', got '{}' for adapter '{}'",
                        expected, actual, adapter_id
                    )).with_code("BASE_MODEL_MISMATCH")),
                ));
            }
            (Some(_), None) => {
                tracing::warn!(adapter_id = %adapter_id, "Adapter missing base_model_id in stack");
            }
            _ => {} // Match
        }
    }

    Ok(expected_base_model)
}

/// Create a new adapter stack
#[utoipa::path(
    post,
    path = "/v1/adapter-stacks",
    request_body = CreateStackRequest,
    responses(
        (status = 201, description = "Stack created", body = StackResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_stack(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateStackRequest>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Insufficient permissions").with_code("FORBIDDEN")),
        )
    })?;

    let tenant_id = claims.tenant_id.clone();

    // Validate stack name format
    let stack_name = StackName::parse(&req.name).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(format!("Invalid stack name: {}", e))
                    .with_code("VALIDATION_ERROR"),
            ),
        )
    })?;

    info!(
        tenant_id = %tenant_id,
        stack_name = %stack_name,
        adapter_count = req.adapter_ids.len(),
        "Creating adapter stack"
    );

    // Validate that all adapters in the stack share the same base model
    let _base_model =
        validate_stack_base_model_compatibility(&state.db, &tenant_id, &req.adapter_ids).await?;

    // Guardrail: Warn if stack creation would likely exceed capacity limits (memory guardrails)
    let uma_stats = state.uma_monitor.get_uma_stats().await;
    let pressure = state.uma_monitor.get_current_pressure();

    // Collect warnings to return in API response
    let mut warnings = Vec::new();

    // Check if adding this stack would exceed limits
    let current_adapters_loaded: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM adapters WHERE current_state IN ('warm', 'hot', 'resident') OR load_state IN ('loaded', 'warm')",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(0);

    let estimated_new_adapters = req.adapter_ids.len() as i64;
    let total_after_stack = current_adapters_loaded + estimated_new_adapters;

    // Check capacity limits from config (drop guard before await)
    let capacity_limits = {
        let config = state.config.read().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Configuration lock poisoned").with_code("INTERNAL_ERROR")),
            )
        })?;
        config.capacity_limits.clone()
    };

    // Warn if memory pressure is high or if we're approaching limits
    if pressure == MemoryPressureLevel::High || pressure == MemoryPressureLevel::Critical {
        let warning_msg = format!(
            "High memory pressure detected ({}): {:.1}% headroom remaining. Consider reducing concurrent operations.",
            pressure,
            uma_stats.headroom_pct
        );
        warnings.push(warning_msg.clone());
        warn!(
            tenant_id = %tenant_id,
            stack_name = %stack_name,
            adapter_count = req.adapter_ids.len(),
            pressure = ?pressure,
            headroom_pct = uma_stats.headroom_pct,
            "Stack creation warning: {}", warning_msg
        );
    }

    // Warn if stack would exceed configured adapter limits
    if let Some(max_adapters) = capacity_limits.models_per_tenant {
        if total_after_stack > max_adapters as i64 {
            let warning_msg = format!(
                "Stack would exceed configured adapter limit: {} adapters (limit: {}). Current: {}, Adding: {}",
                total_after_stack,
                max_adapters,
                current_adapters_loaded,
                estimated_new_adapters
            );
            warnings.push(warning_msg.clone());
            warn!(
                tenant_id = %tenant_id,
                stack_name = %stack_name,
                current_adapters = current_adapters_loaded,
                new_adapters = estimated_new_adapters,
                total_after = total_after_stack,
                limit = max_adapters,
                "Stack creation warning: {}", warning_msg
            );
        }
    } else if total_after_stack > 50 {
        // Fallback warning if no limit configured
        let warning_msg = format!(
            "Stack would exceed recommended adapter limit: {} adapters total (recommended max: 50)",
            total_after_stack
        );
        warnings.push(warning_msg.clone());
        warn!(
            tenant_id = %tenant_id,
            stack_name = %stack_name,
            current_adapters = current_adapters_loaded,
            new_adapters = estimated_new_adapters,
            total_after = total_after_stack,
            "Stack creation warning: {}", warning_msg
        );
    }

    let id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let adapter_ids_json = serde_json::to_string(&req.adapter_ids).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new(format!("Failed to serialize adapter IDs: {}", e))
                    .with_code("SERIALIZATION_ERROR"),
            ),
        )
    })?;
    let db_req = adapteros_db::traits::CreateStackRequest {
        tenant_id: tenant_id.clone(),
        name: req.name.clone(),
        description: req.description.clone(),
        adapter_ids: req.adapter_ids.clone(),
        workflow_type: req.workflow_type.as_ref().map(|w| format!("{:?}", w)),
        determinism_mode: req.determinism_mode.clone(),
        routing_determinism_mode: req.routing_determinism_mode.map(|m| m.to_string()),
    };

    let id = match state.db.insert_stack(&db_req).await {
        Ok(id) => id,
        Err(e) => {
            let (status, error_response) = if e.to_string().contains("UNIQUE constraint failed") {
                (
                    StatusCode::CONFLICT,
                    ErrorResponse::new(format!(
                        "Stack name '{}' already exists for tenant",
                        req.name
                    ))
                    .with_code("CONFLICT"),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ErrorResponse::new(format!("Failed to create stack: {}", e))
                        .with_code("DATABASE_ERROR"),
                )
            };
            // Audit log: stack creation failure
            log_failure_or_warn(
                &state.db,
                &claims,
                ACTION_STACK_CREATE,
                resources::ADAPTER,
                None,
                &format!("Failed to create stack '{}': {}", req.name, e),
            )
            .await;
            return Err((status, Json(error_response)));
        }
    };

    // Audit log: stack creation success
    log_success_or_warn(
        &state.db,
        &claims,
        ACTION_STACK_CREATE,
        resources::ADAPTER,
        Some(&id),
    )
    .await;

    info!(stack_id = %id, stack_name = %stack_name, tenant_id = %tenant_id, "Adapter stack created");

    // Return 201 CREATED - use IntoResponse trait
    // Note: We return Response directly to avoid Handler trait inference issues
    // while still setting the correct status code
    let json_response = Json(StackResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: id.clone(),
        tenant_id,
        name: req.name,
        description: req.description,
        adapter_ids: req.adapter_ids,
        workflow_type: req.workflow_type,
        created_at: now.clone(),
        updated_at: now,
        is_active: false,
        is_default: false, // New stacks are NOT default - must be set explicitly
        version: 1,        // New stacks start at version 1
        lifecycle_state: "active".to_string(), // New stacks default to active
        warnings,          // Include warnings in response (memory guardrails)
        determinism_mode: req.determinism_mode,
        routing_determinism_mode: req.routing_determinism_mode,
    });

    // Convert to Response with 201 status code
    Ok((StatusCode::CREATED, json_response).into_response())
}

/// List all adapter stacks
#[utoipa::path(
    get,
    path = "/v1/adapter-stacks",
    responses(
        (status = 200, description = "List of stacks", body = Vec<StackResponse>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_stacks(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<StackResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterView)?;

    let tenant_id = claims.tenant_id.clone();

    // Fetch the default stack ID for this tenant
    let default_stack_id = state.db.get_default_stack(&tenant_id).await.unwrap_or(None);

    let rows = state
        .db
        .list_stacks_for_tenant(&tenant_id)
        .await
        .map_err(ApiError::db_error)?;

    let mut stacks = Vec::new();
    for row in rows {
        let adapter_ids: Vec<String> =
            serde_json::from_str(&row.adapter_ids_json).unwrap_or_else(|_| vec![]);

        let workflow_type = row.workflow_type.and_then(|s| match s.as_str() {
            "Parallel" => Some(WorkflowType::Parallel),
            "UpstreamDownstream" => Some(WorkflowType::UpstreamDownstream),
            "Sequential" => Some(WorkflowType::Sequential),
            _ => None,
        });

        // Check if this stack is the tenant's default
        let is_default = default_stack_id.as_ref() == Some(&row.id);

        stacks.push(StackResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            id: row.id,
            tenant_id: row.tenant_id,
            name: row.name,
            description: row.description,
            adapter_ids,
            workflow_type,
            created_at: row.created_at,
            updated_at: row.updated_at,
            is_active: false,
            is_default,
            version: row.version,
            lifecycle_state: row.lifecycle_state,
            warnings: vec![], // No warnings for existing stacks
            determinism_mode: row.determinism_mode,
            routing_determinism_mode: parse_routing_mode(&row.routing_determinism_mode),
        });
    }

    Ok(Json(stacks))
}

/// Get a specific adapter stack
#[utoipa::path(
    get,
    path = "/v1/adapter-stacks/{id}",
    params(
        ("id" = String, Path, description = "Stack ID")
    ),
    responses(
        (status = 200, description = "Stack details", body = StackResponse),
        (status = 404, description = "Stack not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_stack(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<StackResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterView)?;

    let tenant_id = claims.tenant_id.clone();

    let row = state
        .db
        .get_stack(&tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("Stack"))?;

    // CRITICAL: Validate tenant isolation to prevent cross-tenant access
    validate_tenant_isolation(&claims, &row.tenant_id)?;

    // Check if this stack is the tenant's default
    let default_stack_id = state.db.get_default_stack(&tenant_id).await.unwrap_or(None);
    let is_default = default_stack_id.as_ref() == Some(&row.id);

    let adapter_ids: Vec<String> =
        serde_json::from_str(&row.adapter_ids_json).unwrap_or_else(|_| vec![]);

    let workflow_type = row.workflow_type.and_then(|s| match s.as_str() {
        "Parallel" => Some(WorkflowType::Parallel),
        "UpstreamDownstream" => Some(WorkflowType::UpstreamDownstream),
        "Sequential" => Some(WorkflowType::Sequential),
        _ => None,
    });

    Ok(Json(StackResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: row.id,
        tenant_id: row.tenant_id,
        name: row.name,
        description: row.description,
        adapter_ids,
        workflow_type,
        created_at: row.created_at,
        updated_at: row.updated_at,
        is_active: false,
        is_default,
        version: row.version,
        lifecycle_state: row.lifecycle_state,
        warnings: vec![], // No warnings for existing stacks
        determinism_mode: row.determinism_mode,
        routing_determinism_mode: parse_routing_mode(&row.routing_determinism_mode),
    }))
}

fn parse_routing_mode(raw: &Option<String>) -> Option<RoutingDeterminismMode> {
    raw.as_deref()
        .and_then(|s| RoutingDeterminismMode::from_str(s).ok())
}

/// Request to update an existing adapter stack
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateStackRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_type: Option<WorkflowType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
}

/// Audit action constant for stack update
const ACTION_STACK_UPDATE: &str = "stack.update";

/// Update an existing adapter stack
#[utoipa::path(
    put,
    path = "/v1/adapter-stacks/{id}",
    params(
        ("id" = String, Path, description = "Stack ID")
    ),
    request_body = UpdateStackRequest,
    responses(
        (status = 200, description = "Stack updated", body = StackResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Stack not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn update_stack(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<UpdateStackRequest>,
) -> Result<Json<StackResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;

    let tenant_id = claims.tenant_id.clone();

    // First fetch existing stack
    let existing = state
        .db
        .get_stack(&tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("Stack"))?;

    // CRITICAL: Validate tenant isolation
    validate_tenant_isolation(&claims, &existing.tenant_id)?;

    // Validate new name if provided
    if let Some(ref new_name) = req.name {
        let _ = StackName::parse(new_name).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new(format!("Invalid stack name: {}", e))
                        .with_code("VALIDATION_ERROR"),
                ),
            )
        })?;
    }

    // Check if adapter_ids are being updated before consuming
    let adapter_ids_changed = req.adapter_ids.is_some();

    // Merge with existing values
    let name = req.name.unwrap_or(existing.name.clone());
    let description = req.description.or(existing.description.clone());
    let adapter_ids: Vec<String> = req
        .adapter_ids
        .unwrap_or_else(|| serde_json::from_str(&existing.adapter_ids_json).unwrap_or_default());
    let workflow_type = req
        .workflow_type
        .map(|w| format!("{:?}", w))
        .or(existing.workflow_type.clone());
    let determinism_mode = req.determinism_mode.or(existing.determinism_mode.clone());
    let routing_determinism_mode = req
        .routing_determinism_mode
        .map(|m| m.to_string())
        .or(existing.routing_determinism_mode.clone());

    // Validate adapter base model compatibility if adapter_ids changed
    if adapter_ids_changed {
        let _ =
            validate_stack_base_model_compatibility(&state.db, &tenant_id, &adapter_ids).await?;
    }

    info!(
        tenant_id = %tenant_id,
        stack_id = %id,
        "Updating adapter stack"
    );

    let update_req = adapteros_db::traits::CreateStackRequest {
        tenant_id: tenant_id.clone(),
        name: name.clone(),
        description: description.clone(),
        adapter_ids: adapter_ids.clone(),
        workflow_type: workflow_type.clone(),
        determinism_mode: determinism_mode.clone(),
        routing_determinism_mode: routing_determinism_mode.clone(),
    };

    state
        .db
        .update_stack(&id, &update_req)
        .await
        .map_err(ApiError::db_error)?;

    // Audit log: stack update success
    log_success_or_warn(
        &state.db,
        &claims,
        ACTION_STACK_UPDATE,
        resources::ADAPTER,
        Some(&id),
    )
    .await;

    // Fetch updated stack for response
    let updated = state
        .db
        .get_stack(&tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::internal("Stack disappeared after update"))?;

    let default_stack_id = state.db.get_default_stack(&tenant_id).await.unwrap_or(None);
    let is_default = default_stack_id.as_ref() == Some(&updated.id);

    let wf_type = updated.workflow_type.and_then(|s| match s.as_str() {
        "Parallel" => Some(WorkflowType::Parallel),
        "UpstreamDownstream" => Some(WorkflowType::UpstreamDownstream),
        "Sequential" => Some(WorkflowType::Sequential),
        _ => None,
    });

    Ok(Json(StackResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: updated.id,
        tenant_id: updated.tenant_id,
        name: updated.name,
        description: updated.description,
        adapter_ids,
        workflow_type: wf_type,
        created_at: updated.created_at,
        updated_at: updated.updated_at,
        is_active: false,
        is_default,
        version: updated.version,
        lifecycle_state: updated.lifecycle_state,
        warnings: vec![],
        determinism_mode: updated.determinism_mode,
        routing_determinism_mode: parse_routing_mode(&updated.routing_determinism_mode),
    }))
}

/// Delete an adapter stack
#[utoipa::path(
    delete,
    path = "/v1/adapter-stacks/{id}",
    params(
        ("id" = String, Path, description = "Stack ID")
    ),
    responses(
        (status = 204, description = "Stack deleted"),
        (status = 404, description = "Stack not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn delete_stack(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;

    let tenant_id = claims.tenant_id.clone();

    let deleted = match state.db.delete_stack(&tenant_id, &id).await {
        Ok(deleted) => deleted,
        Err(e) => {
            // Audit log: stack deletion failure
            log_failure_or_warn(
                &state.db,
                &claims,
                ACTION_STACK_DELETE,
                resources::ADAPTER,
                Some(&id),
                &format!("Failed to delete stack: {}", e),
            )
            .await;
            return Err(ApiError::db_error(e).into());
        }
    };

    if !deleted {
        // Audit log: stack not found
        log_failure_or_warn(
            &state.db,
            &claims,
            ACTION_STACK_DELETE,
            resources::ADAPTER,
            Some(&id),
            "Stack not found",
        )
        .await;
        return Err(ApiError::not_found("Stack").into());
    }

    // Audit log: stack deletion success
    log_success_or_warn(
        &state.db,
        &claims,
        ACTION_STACK_DELETE,
        resources::ADAPTER,
        Some(&id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Activate an adapter stack (sets it as the active stack for routing)
#[utoipa::path(
    post,
    path = "/v1/adapter-stacks/{id}/activate",
    params(
        ("id" = String, Path, description = "Stack ID")
    ),
    responses(
        (status = 200, description = "Stack activated"),
        (status = 404, description = "Stack not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn activate_stack(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterLoad)?;

    let tenant_id = claims.tenant_id.clone();

    // First verify the stack exists and parse adapter IDs (including version for telemetry)
    let stack = state
        .db
        .get_stack(&tenant_id, &id)
        .await
        .map_err(|e| {
            warn!(
                "Database error while fetching stack {} for tenant {}: {}",
                id, tenant_id, e
            );
            ApiError::db_error(e)
        })?
        .ok_or_else(|| {
            warn!(
                "Attempted to activate non-existent stack: {} for tenant {}",
                id, tenant_id
            );
            ApiError::not_found("Stack")
        })?;

    // CRITICAL: Validate tenant isolation to prevent cross-tenant access
    validate_tenant_isolation(&claims, &stack.tenant_id)?;

    let name = stack.name.clone();
    let stack_version = stack.version;

    // Parse adapter IDs to ensure they're valid
    let adapter_ids: Vec<String> = serde_json::from_str(&stack.adapter_ids_json).map_err(|e| {
        warn!("Failed to parse adapter_ids_json for stack {}: {}", name, e);
        ApiError::internal(format!("Invalid adapter list in stack '{}': {}", name, e))
    })?;

    // Validate attach mode for each adapter in the stack
    // If an adapter requires dataset scope, check that the stack provides it
    for adapter_id in &adapter_ids {
        if let Some((attach_mode, required_scope_id)) = state
            .db
            .get_adapter_version_attach_mode(&tenant_id, adapter_id)
            .await
            .map_err(|e| {
                warn!(
                    adapter_id = %adapter_id,
                    error = %e,
                    "Failed to get attach mode for adapter"
                );
                ApiError::db_error(e)
            })?
        {
            if attach_mode == "requires_dataset" {
                // Check if stack has dataset context configured in metadata
                let stack_dataset_version_id = stack
                    .metadata_json
                    .as_ref()
                    .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
                    .and_then(|v| {
                        v.get("dataset_version_id")
                            .and_then(|d| d.as_str())
                            .map(|s| s.to_string())
                    })
                    .filter(|s| !s.is_empty());

                match (stack_dataset_version_id, &required_scope_id) {
                    (None, Some(required)) => {
                        warn!(
                            adapter_id = %adapter_id,
                            required_scope = %required,
                            stack_id = %id,
                            "Adapter requires dataset scope but stack has none configured"
                        );
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(
                                ErrorResponse::new(format!(
                                    "Adapter '{}' requires dataset context (dataset_version_id: {}). \
                                     Configure stack metadata with dataset_version_id to activate.",
                                    adapter_id, required
                                ))
                                .with_code("ATTACH_MODE_VIOLATION"),
                            ),
                        ));
                    }
                    (Some(provided), Some(required)) if &provided != required => {
                        warn!(
                            adapter_id = %adapter_id,
                            required_scope = %required,
                            provided_scope = %provided,
                            stack_id = %id,
                            "Adapter requires specific dataset version but stack provides different one"
                        );
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(
                                ErrorResponse::new(format!(
                                    "Adapter '{}' requires dataset version '{}'. \
                                     Stack is configured with '{}'.",
                                    adapter_id, required, provided
                                ))
                                .with_code("ATTACH_MODE_MISMATCH"),
                            ),
                        ));
                    }
                    _ => {} // OK: free mode, or dataset context matches
                }
            }
        }
    }

    // Persist lifecycle transition to Active for this stack
    state
        .db
        .activate_stack(&tenant_id, &id)
        .await
        .map_err(|e| {
            warn!(
                tenant_id = %tenant_id,
                stack_id = %id,
                error = %e,
                "Failed to mark stack active in database"
            );
            ApiError::db_error(e)
        })?;

    // Store the active stack ID in application state
    // This would be used by the routing logic
    let previous_stack = {
        let mut active_stack = state.active_stack.write().map_err(|e| {
            warn!("Failed to acquire write lock for active_stack: {}", e);
            ApiError::internal("Internal synchronization error")
        })?;
        let prev = active_stack.get(&tenant_id).cloned().flatten();
        active_stack.insert(tenant_id.clone(), Some(id.clone()));
        prev
    };

    let new_hash = compute_stack_hash(&state, &tenant_id, &id).await?;

    let old_hash = if let Some(old_id) = &previous_stack {
        Some(compute_stack_hash(&state, &tenant_id, old_id).await?)
    } else {
        None
    };

    let hash_changed = old_hash.as_ref() != Some(&new_hash);

    if hash_changed {
        if let Some(worker_arc) = &state.worker {
            let worker = worker_arc.lock().await;
            let old_ids = if let Some(old_id) = &previous_stack {
                let stack = state
                    .db
                    .get_stack(&tenant_id, old_id)
                    .await
                    .map_err(ApiError::db_error)?
                    .ok_or_else(|| {
                        ApiError::internal(format!("Previous stack {} not found", old_id))
                    })?;
                serde_json::from_str::<Vec<String>>(&stack.adapter_ids_json)
                    .map_err(|e| ApiError::internal(format!("Parse old: {}", e)))?
            } else {
                vec![]
            };

            let add_ids: Vec<_> = adapter_ids
                .iter()
                .filter(|id| !old_ids.contains(id))
                .cloned()
                .collect();
            let remove_ids: Vec<_> = old_ids
                .iter()
                .filter(|id| !adapter_ids.contains(id))
                .cloned()
                .collect();

            let hotswap = worker.hotswap().clone();
            hotswap
                .swap_with_expected_hash(&add_ids, &remove_ids, Some(new_hash))
                .await
                .map_err(ApiError::db_error)?;

            // KV cache zeroization: method not yet available on Worker
            // if let Some(kv_cache) = worker.kv_cache_mut() {
            //     kv_cache.zeroize_all();
            // }

            *worker.last_stack_hash().write() = Some(new_hash);

            if let Some(telemetry) = worker.telemetry() {
                telemetry.log("stack.swap", json!({
                    "old_stack_hash": old_hash.as_ref().map(|h| h.to_short_hex()),
                    "new_stack_hash": new_hash.to_short_hex(),
                    "cache_reset": true,
                    "tenant_id": tenant_id,
                    "stack_id": id,
                    "stack_version": stack_version, // Include version in telemetry
                    "trace_id": tracing::Span::current().id().map(|id| format!("{:x}", id.into_u64())).unwrap_or("unknown".to_string()),
                })).map_err(|e| ApiError::internal(e.to_string()))?;
            }
        }
    }

    info!(
        tenant_id = %tenant_id,
        "Activated adapter stack '{}' (id: {}) with {} adapters. Previous stack: {:?}",
        name,
        id,
        adapter_ids.len(),
        previous_stack
    );

    // Audit log: stack activated
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::STACK_ACTIVATE,
        crate::audit_helper::resources::ADAPTER_STACK,
        Some(&id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    // Notify the router about the stack change via training signal broadcast
    // This enables SSE clients to receive stack activation events in real-time
    let mut payload = std::collections::HashMap::new();
    payload.insert(
        "stack_id".to_string(),
        serde_json::Value::String(id.clone()),
    );
    payload.insert(
        "stack_name".to_string(),
        serde_json::Value::String(name.clone()),
    );
    payload.insert(
        "tenant_id".to_string(),
        serde_json::Value::String(tenant_id.clone()),
    );
    payload.insert(
        "adapter_count".to_string(),
        serde_json::Value::Number(serde_json::Number::from(adapter_ids.len())),
    );
    payload.insert(
        "previous_stack".to_string(),
        previous_stack
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    payload.insert(
        "action".to_string(),
        serde_json::Value::String("activated".to_string()),
    );
    payload.insert(
        "stack_version".to_string(),
        serde_json::Value::Number(serde_json::Number::from(stack_version)),
    );

    let signal = Signal::with_payload(
        SignalType::AdapterStateTransition,
        SignalPriority::High,
        payload,
    );

    // Broadcast to training signal channel for SSE clients
    if let Err(e) = state.training_signal_tx.send(signal) {
        // Log but don't fail the request - SSE is best-effort
        debug!(
            tenant_id = %tenant_id,
            stack_id = %id,
            error = %e,
            "No active SSE subscribers for stack activation signal"
        );
    }

    // CRITICAL FIX: Promote adapters in lifecycle manager when stack is activated
    // This ensures adapters are at least Warm state when they become part of active stack
    // Without this, adapters remain Unloaded/Cold even when stack is active
    if let Some(ref lifecycle) = state.lifecycle_manager {
        let lm = lifecycle.lock().await;

        // Ensure lifecycle manager knows which stack is active (in-memory routing state)
        lm.load_and_activate_stack(&id).await.map_err(|e| {
            warn!(
                tenant_id = %tenant_id,
                stack_id = %id,
                error = %e,
                "Failed to activate stack in lifecycle manager"
            );
            ApiError::internal("Failed to activate stack in lifecycle manager")
        })?;

        let mut promotion_results = Vec::new();
        for adapter_id_str in &adapter_ids {
            if let Some(adapter_idx) = lm.get_adapter_idx(adapter_id_str) {
                // Promote to at least Warm state for active stack usage
                // Multiple promotions are safe (idempotent if already at higher state)
                match lm.promote_adapter(adapter_idx).await {
                    Ok(_) => {
                        debug!(
                            adapter_id = %adapter_id_str,
                            "Promoted adapter for stack activation"
                        );
                        promotion_results.push((adapter_id_str.clone(), true));
                    }
                    Err(e) => {
                        // Log but don't fail - adapter may already be at maximum state
                        debug!(
                            adapter_id = %adapter_id_str,
                            error = %e,
                            "Could not promote adapter (may already be at target state)"
                        );
                        promotion_results.push((adapter_id_str.clone(), false));
                    }
                }
            } else {
                debug!(
                    adapter_id = %adapter_id_str,
                    "Adapter not found in lifecycle manager (may need registration)"
                );
            }
        }

        info!(
            tenant_id = %tenant_id,
            stack_id = %id,
            adapter_count = adapter_ids.len(),
            promoted_count = promotion_results.iter().filter(|(_, success)| *success).count(),
            "Promoted adapters for stack activation via lifecycle manager"
        );
        drop(lm);
    }

    Ok(Json(serde_json::json!({
        "message": format!("Stack '{}' activated for tenant '{}'", name, tenant_id),
        "stack_id": id,
        "tenant_id": tenant_id,
        "adapter_count": adapter_ids.len(),
        "previous_stack": previous_stack,
    })))
}

/// Response for clearing adapters from a stack
#[derive(Serialize, ToSchema)]
pub struct ClearStackAdaptersResponse {
    pub message: String,
    pub stack_id: String,
    pub previous_adapter_count: usize,
    pub adapters_removed: Vec<String>,
}

/// Clear all adapters from a stack
#[utoipa::path(
    post,
    path = "/v1/adapter-stacks/{id}/clear-adapters",
    params(
        ("id" = String, Path, description = "Stack ID")
    ),
    responses(
        (status = 200, description = "Adapters cleared from stack", body = ClearStackAdaptersResponse),
        (status = 404, description = "Stack not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn clear_stack_adapters(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<ClearStackAdaptersResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;

    let tenant_id = claims.tenant_id.clone();

    // First verify the stack exists and get current adapter list
    let stack = state
        .db
        .get_stack(&tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("Stack"))?;

    // CRITICAL: Validate tenant isolation to prevent cross-tenant access
    validate_tenant_isolation(&claims, &stack.tenant_id)?;

    // Parse current adapter IDs
    let previous_adapter_ids: Vec<String> = serde_json::from_str(&stack.adapter_ids_json)
        .map_err(|e| ApiError::internal(format!("Failed to parse adapter IDs: {}", e)))?;

    let previous_adapter_count = previous_adapter_ids.len();

    info!(
        tenant_id = %tenant_id,
        stack_id = %id,
        adapter_count = previous_adapter_count,
        "Clearing adapters from stack"
    );

    // Create update request with empty adapter list
    let update_req = adapteros_db::traits::CreateStackRequest {
        tenant_id: tenant_id.clone(),
        name: stack.name.clone(),
        description: stack.description.clone(),
        adapter_ids: vec![], // Empty adapter list
        workflow_type: stack.workflow_type.clone(),
        determinism_mode: stack.determinism_mode.clone(),
        routing_determinism_mode: stack.routing_determinism_mode.clone(),
    };

    // Update the stack with empty adapters
    state
        .db
        .update_stack(&id, &update_req)
        .await
        .map_err(ApiError::db_error)?;

    info!(
        tenant_id = %tenant_id,
        stack_id = %id,
        removed_count = previous_adapter_count,
        "Cleared adapters from stack"
    );

    // Audit log: stack adapters cleared
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        "stack.clear_adapters",
        crate::audit_helper::resources::ADAPTER_STACK,
        Some(&id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(ClearStackAdaptersResponse {
        message: format!(
            "Cleared {} adapter(s) from stack '{}'",
            previous_adapter_count, stack.name
        ),
        stack_id: id,
        previous_adapter_count,
        adapters_removed: previous_adapter_ids,
    }))
}

/// Deactivate the current adapter stack
#[utoipa::path(
    post,
    path = "/v1/adapter-stacks/deactivate",
    responses(
        (status = 200, description = "Stack deactivated"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn deactivate_stack(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterLoad)?;

    // Hot-swap safeguard: block deactivation while other requests are active
    guard_in_flight_requests(&state.in_flight_requests)?;

    let tenant_id = claims.tenant_id.clone();

    let previous_stack = {
        let mut active = state
            .active_stack
            .write()
            .map_err(|e| ApiError::internal(format!("Lock poisoned: {}", e)))?;
        let prev = active.get(&tenant_id).cloned().flatten();
        active.insert(tenant_id.clone(), None);
        prev
    };

    match previous_stack {
        Some(stack_id) => {
            info!(tenant_id = %tenant_id, "Deactivated adapter stack '{}' for tenant {}", stack_id, tenant_id);

            // Audit log: stack deactivated
            if let Err(e) = crate::audit_helper::log_success(
                &state.db,
                &claims,
                crate::audit_helper::actions::STACK_DEACTIVATE,
                crate::audit_helper::resources::ADAPTER_STACK,
                Some(&stack_id),
            )
            .await
            {
                tracing::warn!(error = %e, "Audit log failed");
            }

            // Notify the router about the stack deactivation via training signal broadcast
            let mut payload = std::collections::HashMap::new();
            payload.insert(
                "stack_id".to_string(),
                serde_json::Value::String(stack_id.clone()),
            );
            payload.insert(
                "tenant_id".to_string(),
                serde_json::Value::String(tenant_id.clone()),
            );
            payload.insert(
                "action".to_string(),
                serde_json::Value::String("deactivated".to_string()),
            );

            let signal = Signal::with_payload(
                SignalType::AdapterStateTransition,
                SignalPriority::High,
                payload,
            );

            // Broadcast to training signal channel for SSE clients
            if let Err(e) = state.training_signal_tx.send(signal) {
                debug!(
                    tenant_id = %tenant_id,
                    stack_id = %stack_id,
                    error = %e,
                    "No active SSE subscribers for stack deactivation signal"
                );
            }

            Ok(Json(serde_json::json!({
                "message": "Active stack deactivated",
                "tenant_id": tenant_id,
                "previous_stack": stack_id,
            })))
        }
        None => {
            info!(tenant_id = %tenant_id, "Deactivate called but no stack was active");
            Ok(Json(serde_json::json!({
                "message": "No stack was active",
                "tenant_id": tenant_id,
            })))
        }
    }
}

async fn compute_stack_hash(
    state: &AppState,
    tenant_id: &str,
    stack_id: &str,
) -> Result<B3Hash, (StatusCode, Json<ErrorResponse>)> {
    let stack = state
        .db
        .get_stack(tenant_id, stack_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("Stack"))?;

    let adapter_ids: Vec<String> = serde_json::from_str(&stack.adapter_ids_json)
        .map_err(|e| ApiError::internal(format!("Parse error: {}", e)))?;

    let mut pairs = vec![];

    for id in &adapter_ids {
        let adapter = state
            .db
            .get_adapter_by_id(tenant_id, id)
            .await
            .map_err(ApiError::db_error)?
            .ok_or_else(|| ApiError::not_found(&format!("Adapter {}", id)))?;

        let hash = adapteros_core::B3Hash::from_hex(&adapter.hash_b3).map_err(ApiError::db_error)?;
        pairs.push((id.clone(), hash));
    }

    // Use canonical compute_stack_hash from adapteros-core
    Ok(adapteros_core::compute_stack_hash(pairs))
}

/// Lifecycle history event response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LifecycleHistoryResponse {
    pub id: String,
    pub entity_id: String,
    pub version: String,
    pub lifecycle_state: String,
    pub previous_lifecycle_state: Option<String>,
    pub reason: Option<String>,
    pub initiated_by: String,
    pub metadata_json: Option<String>,
    pub created_at: String,
}

impl From<LifecycleHistoryEvent> for LifecycleHistoryResponse {
    fn from(event: LifecycleHistoryEvent) -> Self {
        Self {
            id: event.id,
            entity_id: event.entity_id,
            version: event.version,
            lifecycle_state: event.lifecycle_state,
            previous_lifecycle_state: event.previous_lifecycle_state,
            reason: event.reason,
            initiated_by: event.initiated_by,
            metadata_json: event.metadata_json,
            created_at: event.created_at,
        }
    }
}

/// Get version history for an adapter stack
#[utoipa::path(
    get,
    path = "/v1/adapter-stacks/{id}/history",
    params(
        ("id" = String, Path, description = "Stack ID")
    ),
    responses(
        (status = 200, description = "Stack version history", body = Vec<LifecycleHistoryResponse>),
        (status = 404, description = "Stack not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_stack_history(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<Vec<LifecycleHistoryResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterView)?;

    let tenant_id = claims.tenant_id.clone();

    // Verify stack exists and belongs to tenant
    let stack = state
        .db
        .get_stack(&tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| {
            ApiError::not_found(&format!(
                "Stack with id '{}' not found for tenant '{}'",
                id, tenant_id
            ))
        })?;

    // CRITICAL: Validate tenant isolation to prevent cross-tenant access
    validate_tenant_isolation(&claims, &stack.tenant_id)?;

    // Get lifecycle history
    let history = state
        .db
        .get_stack_lifecycle_history(&id)
        .await
        .map_err(ApiError::db_error)?;

    let response: Vec<LifecycleHistoryResponse> = history.into_iter().map(Into::into).collect();

    Ok(Json(response))
}

/// Get policies assigned to an adapter stack with compliance summary
#[utoipa::path(
    get,
    path = "/v1/adapter-stacks/{id}/policies",
    params(
        ("id" = String, Path, description = "Stack ID")
    ),
    responses(
        (status = 200, description = "Stack policies with compliance info", body = crate::types::StackPoliciesResponse),
        (status = 404, description = "Stack not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_stack_policies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<crate::types::StackPoliciesResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::PolicyView)?;

    let tenant_id = claims.tenant_id.clone();

    // Verify stack exists and belongs to tenant
    let stack = state
        .db
        .get_stack(&tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| {
            ApiError::not_found(&format!(
                "Stack with id '{}' not found for tenant '{}'",
                id, tenant_id
            ))
        })?;

    // CRITICAL: Validate tenant isolation to prevent cross-tenant access
    validate_tenant_isolation(&claims, &stack.tenant_id)?;

    // Get policy assignments for this stack
    let assignments = state
        .db
        .get_policy_assignments_for_stack(&id)
        .await
        .map_err(ApiError::db_error)?;

    // Convert to detailed assignment info with policy pack details
    let mut assignment_details = Vec::new();
    for assignment in assignments {
        // Get the policy pack details
        let pack = state
            .db
            .get_policy_pack(&assignment.policy_pack_id)
            .await
            .map_err(ApiError::db_error)?;

        let (policy_type, policy_name, version, status) = if let Some(p) = pack {
            (
                p.policy_type.clone(),
                p.description
                    .clone()
                    .unwrap_or_else(|| assignment.policy_pack_id.clone()),
                p.version.clone(),
                p.status.clone(),
            )
        } else {
            (
                "unknown".to_string(),
                assignment.policy_pack_id.clone(),
                "1.0.0".to_string(),
                "active".to_string(),
            )
        };

        assignment_details.push(crate::types::PolicyAssignmentDetail {
            id: assignment.id,
            policy_pack_id: assignment.policy_pack_id,
            policy_type,
            policy_name,
            version,
            status,
            enforced: assignment.enforced,
            priority: assignment.priority,
            assigned_at: assignment.assigned_at,
            assigned_by: assignment.assigned_by,
            expires_at: assignment.expires_at,
        });
    }

    // Calculate compliance summary
    let compliance_data = state
        .db
        .calculate_stack_compliance(&id, &tenant_id)
        .await
        .map_err(ApiError::db_error)?;

    // Convert db type to API response type
    let compliance = crate::types::StackComplianceSummary {
        overall_score: compliance_data.overall_score,
        status: compliance_data.status,
        by_category: compliance_data
            .by_category
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    crate::types::CategoryComplianceScore {
                        score: v.score,
                        passed: v.passed,
                        failed: v.failed,
                    },
                )
            })
            .collect(),
        last_calculated: compliance_data.last_calculated,
    };

    // Get recent violations (last 24 hours)
    let violations = state
        .db
        .get_recent_stack_violations(&id, 24)
        .await
        .map_err(ApiError::db_error)?;

    let recent_violations: Vec<crate::types::PolicyViolationSummary> = violations
        .into_iter()
        .map(|v| crate::types::PolicyViolationSummary {
            id: v.id,
            policy_pack_id: v.policy_pack_id,
            severity: v.severity,
            message: v.violation_message,
            detected_at: v.detected_at,
            resolved_at: v.resolved_at,
        })
        .collect();

    let now = chrono::Utc::now().to_rfc3339();

    Ok(Json(crate::types::StackPoliciesResponse {
        stack_id: id,
        stack_name: stack.name,
        assignments: assignment_details,
        compliance,
        recent_violations,
        timestamp: now,
    }))
}

/// Stack policy streaming endpoint (SSE)
///
/// Streams real-time policy compliance updates for a specific stack.
/// Useful for live monitoring of policy enforcement and violations.
pub async fn stack_policy_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> axum::response::sse::Sse<
    impl futures_util::stream::Stream<
        Item = Result<axum::response::sse::Event, std::convert::Infallible>,
    >,
> {
    use axum::response::sse::{Event, KeepAlive};
    use futures_util::stream;
    use std::time::Duration;

    // Permission check: PolicyView required
    let has_permission = require_permission(&claims, Permission::PolicyView).is_ok();

    if !has_permission {
        warn!("Permission denied for stack policy stream");
    } else {
        info!(stack_id = %id, "Starting stack policy SSE stream");
    }

    let tenant_id = claims.tenant_id.clone();
    let stream = stream::unfold(
        (state, id, tenant_id, has_permission),
        |(state, id, tenant_id, has_permission)| async move {
            if !has_permission {
                // Return error event once and end stream
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"permission denied\"}")),
                    (state, id, tenant_id, false),
                ));
            }
            // Poll every 2 seconds for policy updates
            tokio::time::sleep(Duration::from_secs(2)).await;

            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            // Check if stack exists and user has access
            let stack_result = state.db.get_stack(&tenant_id, &id).await;

            let stack = match stack_result {
                Ok(Some(s)) => s,
                Ok(None) => {
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data("{\"error\": \"stack not found\"}")),
                        (state, id, tenant_id, has_permission),
                    ));
                }
                Err(e) => {
                    warn!(error = ?e, "Failed to fetch stack for policy stream");
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data(format!("{{\"error\": \"database error: {}\"}}", e))),
                        (state, id, tenant_id, has_permission),
                    ));
                }
            };

            // Get policy assignments
            let assignments = state
                .db
                .get_policy_assignments_for_stack(&id)
                .await
                .unwrap_or_default();

            // Build response data
            let data = serde_json::json!({
                "stack_id": id,
                "stack_name": stack.name,
                "policy_count": assignments.len(),
                "timestamp": timestamp,
            });

            let json = match serde_json::to_string(&data) {
                Ok(j) => j,
                Err(e) => {
                    warn!(error = %e, "Failed to serialize policy stream event");
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data(format!("{{\"error\": \"serialization failed: {}\"}}", e))),
                        (state, id, tenant_id, has_permission),
                    ));
                }
            };

            Some((
                Ok(Event::default().event("stack_policy").data(json)),
                (state, id, tenant_id, has_permission),
            ))
        },
    );

    axum::response::sse::Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keep-alive"),
    )
}
