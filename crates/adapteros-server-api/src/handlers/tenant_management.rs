//! Tenant management handlers
//!
//! This module contains handlers for tenant lifecycle management including
//! update, pause, archive, hydration, and usage operations.

use crate::auth::Claims;
use crate::handlers::event_applier::{apply_event, parse_event, TenantEvent};
use crate::handlers::utils::aos_error_to_response;
use crate::middleware::{require_any_role, require_role};
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::*;
use adapteros_core::tenant_snapshot::TenantStateSnapshot;
use adapteros_core::AosError;
use adapteros_db::sqlx;
use adapteros_db::users::Role;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// Update tenant metadata
pub async fn update_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<UpdateTenantRequest>,
) -> Result<Json<TenantResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Update tenant in database using Db trait methods
    if let Some(ref name) = req.name {
        state
            .db
            .rename_tenant(&tenant_id, name)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update tenant")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    }

    if let Some(itar_flag) = req.itar_flag {
        state
            .db
            .update_tenant_itar_flag(&tenant_id, itar_flag)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update tenant")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    }

    // Fetch updated tenant using Db trait method
    let tenant = state.db.get_tenant(&tenant_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to fetch tenant")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let tenant = tenant.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("tenant not found").with_code("NOT_FOUND")),
        )
    })?;

    let tenant_id_value = tenant.id.clone();

    // Audit log: tenant updated
    crate::audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant_id_value),
    )
    .await;

    Ok(Json(TenantResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: tenant_id_value,
        name: tenant.name,
        itar_flag: tenant.itar_flag,
        created_at: tenant.created_at,
        status: tenant.status.unwrap_or_else(|| "active".to_string()),
        updated_at: tenant.updated_at,
        default_stack_id: tenant.default_stack_id,
        max_adapters: tenant.max_adapters,
        max_training_jobs: tenant.max_training_jobs,
        max_storage_gb: tenant.max_storage_gb,
        rate_limit_rpm: tenant.rate_limit_rpm,
    }))
}

/// Pause tenant (stop new sessions)
pub async fn pause_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Update tenant status to 'paused' using Db trait method
    state.db.pause_tenant(&tenant_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to pause tenant")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    tracing::info!("Tenant {} paused by {}", tenant_id, claims.email);

    // Audit log: tenant paused
    crate::audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_PAUSE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Archive tenant (permanent deactivation)
pub async fn archive_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Mark tenant as archived using Db trait method
    state.db.archive_tenant(&tenant_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to archive tenant")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    tracing::info!("Tenant {} archived by {}", tenant_id, claims.email);

    // Audit log: tenant archived
    crate::audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_ARCHIVE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Get UMA memory stats
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/system/memory",
    responses(
        (status = 200, description = "UMA memory stats", body = UmaMemoryResponse)
    )
)]
pub async fn get_uma_memory(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<UmaMemoryResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Assume state has uma_monitor: Arc<UmaPressureMonitor>
    let stats = state.uma_monitor.get_uma_stats().await;
    let pressure = state.uma_monitor.get_current_pressure();

    let candidates = sqlx::query_as::<_, (String,)>(
        "SELECT adapter_id FROM adapters WHERE current_state IN ('warm', 'cold') AND (pinned_until IS NULL OR pinned_until < datetime('now'))"
    )
    .fetch_all(state.db.pool())
    .await
    .map(|rows| rows.into_iter().map(|(id,)| id).collect())
    .unwrap_or_default();

    let ane_usage =
        if let (Some(allocated_mb), Some(used_mb), Some(available_mb), Some(usage_pct)) = (
            stats.ane_allocated_mb,
            stats.ane_used_mb,
            stats.ane_available_mb,
            stats.ane_usage_percent,
        ) {
            Some(crate::handlers::system_info::AneUsage {
                allocated_mb,
                used_mb,
                available_mb,
                usage_pct,
            })
        } else {
            None
        };

    Ok(Json(UmaMemoryResponse {
        total_mb: stats.total_mb,
        used_mb: stats.used_mb,
        available_mb: stats.available_mb,
        headroom_pct: stats.headroom_pct,
        pressure_level: pressure.to_string(),
        eviction_candidates: candidates,
        timestamp: chrono::Utc::now().to_rfc3339(),
        ane: ane_usage,
    }))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct UmaMemoryResponse {
    pub total_mb: u64,
    pub used_mb: u64,
    pub available_mb: u64,
    pub headroom_pct: f32,
    pub pressure_level: String,
    pub eviction_candidates: Vec<String>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ane: Option<crate::handlers::system_info::AneUsage>,
}

/// Get tenant index hashes
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/tenant/{tenant_id}/indexes/hash",
    responses(
        (status = 200, body = IndexHashesResponse),
    ),
    params(
        ("tenant_id" = String, Path, description = "Tenant ID"),
    ),
    tag = "indexes"
)]
pub async fn get_tenant_index_hashes(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<IndexHashesResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::TenantView)?;

    if state
        .db
        .get_tenant(&tenant_id)
        .await
        .map_err(aos_error_to_response)?
        .is_none()
    {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Tenant not found")),
        ));
    }

    let types = vec![
        "adapter_graph",
        "stacks",
        "router_table",
        "telemetry_secondary",
    ];
    let mut hashes = std::collections::HashMap::new();
    for typ in types {
        if let Some(hash) = state
            .db
            .get_index_hash(&tenant_id, typ)
            .await
            .map_err(aos_error_to_response)?
        {
            hashes.insert(typ.to_string(), hash.to_hex());
        }
    }

    Ok(Json(IndexHashesResponse { tenant_id, hashes }))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct IndexHashesResponse {
    pub tenant_id: String,
    pub hashes: std::collections::HashMap<String, String>,
}

/// Hydrate tenant from bundle
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/tenants/hydrate",
    request_body = HydrateTenantRequest,
    responses(
        (status = 200, description = "Tenant hydrated successfully", body = TenantHydrationResponse),
        (status = 400, description = "Invalid bundle or hash mismatch"),
        (status = 500, description = "Internal server error")
    ),
    tag = "tenants"
)]
pub async fn hydrate_tenant_from_bundle(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<HydrateTenantRequest>,
) -> Result<Json<TenantHydrationResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    let events = state
        .telemetry_bundle_store
        .read()
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(
                    "Failed to acquire lock on telemetry bundle store",
                )),
            )
        })?
        .get_bundle_events(&req.bundle_id)
        .map_err(|e: AosError| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    // Sort events canonical: timestamp asc, then event_type asc
    let mut sorted_events: Vec<&serde_json::Value> = events.iter().collect();
    sorted_events.sort_by(|e1: &&serde_json::Value, e2: &&serde_json::Value| {
        let ts1 = e1
            .get("timestamp")
            .and_then(|v: &serde_json::Value| v.as_i64())
            .unwrap_or(0);
        let ts2 = e2
            .get("timestamp")
            .and_then(|v: &serde_json::Value| v.as_i64())
            .unwrap_or(0);
        ts1.cmp(&ts2).then_with(|| {
            e1.get("event_type")
                .and_then(|v: &serde_json::Value| v.as_str())
                .unwrap_or("")
                .cmp(
                    e2.get("event_type")
                        .and_then(|v: &serde_json::Value| v.as_str())
                        .unwrap_or(""),
                )
        })
    });

    let events_vec: Vec<serde_json::Value> = sorted_events.iter().cloned().cloned().collect();
    let sim_snapshot = TenantStateSnapshot::from_bundle_events(&events_vec);
    let sim_hash = sim_snapshot.compute_hash();

    let typed_events: Vec<TenantEvent> = sorted_events
        .iter()
        .map(|event| {
            parse_event(event).map_err(|err| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(format!("Invalid event: {}", err))),
                )
            })
        })
        .collect::<Result<_, _>>()?;

    if req.dry_run {
        if let Some(expected) = &req.expected_state_hash {
            if expected != &sim_hash.to_hex() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(
                        "Computed state hash does not match expected",
                    )),
                ));
            }
        }
        return Ok(Json(TenantHydrationResponse {
            tenant_id: req.tenant_id.clone(),
            state_hash: sim_hash.to_hex(),
            status: "dry_run_success".to_string(),
            errors: vec![],
        }));
    }

    // Full hydration
    let current_opt = state
        .db
        .get_tenant_snapshot_hash(&req.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    if let Some(current_hash) = current_opt {
        if current_hash != sim_hash {
            return Err((
                StatusCode::CONFLICT,
                Json(ErrorResponse::new(
                    "Tenant state mismatch: cannot hydrate non-idempotently",
                )),
            ));
        }
        // Already hydrated with same bundle, idempotent ok
        tracing::info!(
            "Tenant {} already hydrated with matching state hash {}",
            req.tenant_id,
            sim_hash
        );
        let _tenant = state
            .db
            .get_tenant(&req.tenant_id)
            .await
            .map_err(|e| {
                aos_error_to_response(AosError::Database(format!("Failed to get tenant: {}", e)))
            })?
            .ok_or_else(|| {
                aos_error_to_response(AosError::NotFound(format!(
                    "Tenant {} not found",
                    req.tenant_id
                )))
            })?;
        return Ok(Json(TenantHydrationResponse {
            tenant_id: req.tenant_id.clone(),
            state_hash: sim_hash.to_hex(),
            status: "already_hydrated".to_string(),
            errors: vec![],
        }));
    }

    // New tenant or mismatch (but mismatch already errored), create and apply
    let tenant_exists = state
        .db
        .get_tenant(&req.tenant_id)
        .await
        .map_err(|e| {
            aos_error_to_response(AosError::Database(format!(
                "Failed to check tenant existence: {}",
                e
            )))
        })?
        .is_some();

    if !tenant_exists {
        state
            .db
            .create_tenant(&req.tenant_id, false)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(e.to_string())),
                )
            })?;
    }

    // Apply in transaction
    let mut tx = state.db.pool().begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
    })?;

    for event in &typed_events {
        if let Err(e) = apply_event(&mut tx, &req.tenant_id, event).await {
            tracing::error!(
                identity = ?event.identity_label(),
                error = %e,
                "Failed to apply event in hydration"
            );
            let _ = tx.rollback().await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!(
                    "Hydration failed on event: {}",
                    e
                ))),
            ));
        }
    }

    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
    })?;

    // Build and store snapshot
    let snapshot = state
        .db
        .build_tenant_snapshot(&req.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    let final_hash = snapshot.compute_hash();
    // Verify matches sim
    if final_hash != sim_hash {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(
                "Post-hydration state hash mismatch (internal error)",
            )),
        ));
    }

    state
        .db
        .store_tenant_snapshot_hash(&req.tenant_id, &final_hash)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    // Rebuild indexes
    state
        .db
        .rebuild_all_indexes(&req.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    let _tenant = state
        .db
        .get_tenant(&req.tenant_id)
        .await
        .map_err(|e| {
            aos_error_to_response(AosError::Database(format!("Failed to get tenant: {}", e)))
        })?
        .ok_or_else(|| {
            aos_error_to_response(AosError::NotFound(format!(
                "Tenant {} not found",
                req.tenant_id
            )))
        })?;

    Ok(Json(TenantHydrationResponse {
        tenant_id: req.tenant_id.clone(),
        state_hash: final_hash.to_hex(),
        status: "hydrated".to_string(),
        errors: vec![],
    }))
}

// Define response
#[derive(Serialize, utoipa::ToSchema)]
pub struct TenantHydrationResponse {
    pub tenant_id: String,
    pub state_hash: String,
    pub status: String,
    pub errors: Vec<String>,
}

// Request type
#[derive(Deserialize, ToSchema)]
pub struct HydrateTenantRequest {
    pub bundle_id: String,
    pub tenant_id: String,
    pub dry_run: bool,
    pub expected_state_hash: Option<String>,
}

/// Assign adapters to tenant
pub async fn assign_tenant_adapters(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<AssignAdaptersRequest>,
) -> Result<Json<AssignAdaptersResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    // Create tenant-adapter associations using Db trait method
    for adapter_id in &req.adapter_ids {
        state
            .db
            .assign_adapter_to_tenant(&tenant_id, adapter_id, &claims.sub)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to assign adapter")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    }

    tracing::info!(
        "Assigned {} adapters to tenant {} by {}",
        req.adapter_ids.len(),
        tenant_id,
        claims.email
    );

    Ok(Json(AssignAdaptersResponse {
        tenant_id,
        assigned_adapter_ids: req.adapter_ids,
        assigned_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get tenant resource usage metrics
pub async fn get_tenant_usage(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TenantUsageResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would aggregate usage metrics from workers/sessions
    Ok(Json(TenantUsageResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        tenant_id,
        cpu_usage_pct: 45.2,
        gpu_usage_pct: 85.0,
        memory_used_gb: 8.5,
        memory_total_gb: 16.0,
        inference_count_24h: 1250,
        active_adapters_count: 12,
        avg_latency_ms: Some(125.5),
        estimated_cost_usd: Some(42.50),
    }))
}
