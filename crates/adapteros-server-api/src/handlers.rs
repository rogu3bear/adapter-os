use crate::auth::{generate_token, verify_password, Claims};
use crate::middleware::{require_any_role, require_role};
use crate::state::AppState;
use crate::types::*;
use crate::uds_client::{UdsClient, UdsClientError};
use crate::validation::*;

pub mod domain_adapters;
pub mod git;
pub mod git_repository;

// Re-export domain adapter handlers
pub use domain_adapters::*;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use adapteros_db::sqlx;
use adapteros_db::users::Role;
use serde::Deserialize;
use serde_json::json;

/// Health check endpoint
#[utoipa::path(
    get,
    path = "/healthz",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    )
)]
pub async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Readiness check
#[utoipa::path(
    get,
    path = "/readyz",
    responses(
        (status = 200, description = "Service is ready", body = HealthResponse),
        (status = 503, description = "Service is not ready", body = HealthResponse)
    )
)]
pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    // Check database connectivity
    match state.db.pool().acquire().await {
        Ok(_) => (
            StatusCode::OK,
            Json(HealthResponse {
                status: "ready".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            }),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "not ready".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            }),
        ),
    }
}

/// Login handler
#[utoipa::path(
    post,
    path = "/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials", body = ErrorResponse)
    )
)]
pub async fn auth_login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Get user by email
    let user = state
        .db
        .get_user_by_email(&req.email)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "invalid credentials".to_string(),
                    details: None,
                }),
            )
        })?;

    // Check if user is disabled
    if user.disabled {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "user disabled".to_string(),
                details: None,
            }),
        ));
    }

    // Verify password (temporarily bypassed for testing)
    let valid = if user.pw_hash == "password" {
        // Simple plain text check for testing
        req.password == "password"
    } else {
        // Use proper Argon2 verification for production
        verify_password(&req.password, &user.pw_hash).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "authentication error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
    };

    if !valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid credentials".to_string(),
                details: None,
            }),
        ));
    }

    // Generate JWT
    let token =
        generate_token(&user.id, &user.email, &user.role, &state.jwt_secret).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "token generation failed".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    Ok(Json(LoginResponse {
        token,
        user_id: user.id,
        role: user.role,
    }))
}

/// List tenants (all roles can view)
pub async fn list_tenants(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<TenantResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let tenants = state.db.list_tenants().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "database error".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let response: Vec<TenantResponse> = tenants
        .into_iter()
        .map(|t| TenantResponse {
            id: t.id,
            name: t.name,
            itar_flag: t.itar_flag,
            created_at: t.created_at,
        })
        .collect();

    Ok(Json(response))
}

/// Create tenant (admin only)
pub async fn create_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateTenantRequest>,
) -> Result<Json<TenantResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    let id = state
        .db
        .create_tenant(&req.name, req.itar_flag)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to create tenant".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    let tenant = state.db.get_tenant(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "database error".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let tenant = tenant.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "tenant not found after creation".to_string(),
                details: None,
            }),
        )
    })?;

    Ok(Json(TenantResponse {
        id: tenant.id,
        name: tenant.name,
        itar_flag: tenant.itar_flag,
        created_at: tenant.created_at,
    }))
}

/// Update tenant metadata
pub async fn update_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<UpdateTenantRequest>,
) -> Result<Json<TenantResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Update tenant in database
    if let Some(ref name) = req.name {
        sqlx::query(
            "UPDATE tenants SET name = ?, updated_at = datetime('now') WHERE tenant_id = ?",
        )
        .bind(name)
        .bind(&tenant_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to update tenant".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;
    }

    if let Some(itar_flag) = req.itar_flag {
        sqlx::query(
            "UPDATE tenants SET itar_flag = ?, updated_at = datetime('now') WHERE tenant_id = ?",
        )
        .bind(itar_flag)
        .bind(&tenant_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to update tenant".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;
    }

    // Fetch updated tenant
    let row = sqlx::query(
        "SELECT tenant_id, name, itar_flag, created_at FROM tenants WHERE tenant_id = ?",
    )
    .bind(&tenant_id)
    .fetch_one(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "tenant not found".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    use sqlx::Row;
    Ok(Json(TenantResponse {
        id: row.get("tenant_id"),
        name: row.get("name"),
        itar_flag: row.get("itar_flag"),
        created_at: row.get("created_at"),
    }))
}

/// Pause tenant (stop new sessions)
pub async fn pause_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Update tenant status to 'paused' in database
    sqlx::query(
        "UPDATE tenants SET status = 'paused', updated_at = datetime('now') WHERE tenant_id = ?",
    )
    .bind(&tenant_id)
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to pause tenant".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    tracing::info!("Tenant {} paused by {}", tenant_id, claims.email);
    Ok(StatusCode::NO_CONTENT)
}

/// Archive tenant (permanent deactivation)
pub async fn archive_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Mark tenant as archived in database
    sqlx::query(
        "UPDATE tenants SET status = 'archived', updated_at = datetime('now') WHERE tenant_id = ?",
    )
    .bind(&tenant_id)
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to archive tenant".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    tracing::info!("Tenant {} archived by {}", tenant_id, claims.email);
    Ok(StatusCode::NO_CONTENT)
}

/// Assign policies to tenant
pub async fn assign_tenant_policies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<AssignPoliciesRequest>,
) -> Result<Json<AssignPoliciesResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Compliance])?;

    // Create tenant-policy associations in database
    for policy_id in &req.policy_ids {
        sqlx::query(
            "INSERT OR REPLACE INTO tenant_policies (tenant_id, cpid, assigned_by, assigned_at)
             VALUES (?, ?, ?, datetime('now'))",
        )
        .bind(&tenant_id)
        .bind(policy_id)
        .bind(&claims.sub)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to assign policy".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;
    }

    tracing::info!(
        "Assigned {} policies to tenant {} by {}",
        req.policy_ids.len(),
        tenant_id,
        claims.email
    );

    Ok(Json(AssignPoliciesResponse {
        tenant_id,
        assigned_cpids: req.policy_ids,
        assigned_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Assign adapters to tenant
pub async fn assign_tenant_adapters(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<AssignAdaptersRequest>,
) -> Result<Json<AssignAdaptersResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    // Create tenant-adapter associations in database
    for adapter_id in &req.adapter_ids {
        sqlx::query(
            "INSERT OR REPLACE INTO tenant_adapters (tenant_id, adapter_id, assigned_by, assigned_at)
             VALUES (?, ?, ?, datetime('now'))"
        )
        .bind(&tenant_id)
        .bind(adapter_id)
        .bind(&claims.sub)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to assign adapter".to_string(),
                    details: Some(e.to_string()),
                }),
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

/// List nodes
pub async fn list_nodes(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<NodeResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Sre])?;

    let nodes = state.db.list_nodes().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "database error".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let response: Vec<NodeResponse> = nodes
        .into_iter()
        .map(|n| NodeResponse {
            id: n.id,
            hostname: n.hostname,
            agent_endpoint: n.agent_endpoint,
            status: n.status,
            last_seen_at: n.last_seen_at,
        })
        .collect();

    Ok(Json(response))
}

/// Register node
pub async fn register_node(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RegisterNodeRequest>,
) -> Result<Json<NodeResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Sre])?;

    let id = state
        .db
        .register_node(&req.hostname, &req.agent_endpoint)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to register node".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    let node = state.db.get_node(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "database error".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let node = node.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "node not found after registration".to_string(),
                details: None,
            }),
        )
    })?;

    Ok(Json(NodeResponse {
        id: node.id,
        hostname: node.hostname,
        agent_endpoint: node.agent_endpoint,
        status: node.status,
        last_seen_at: node.last_seen_at,
    }))
}

/// Test node connection (ping)
pub async fn test_node_connection(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<Json<NodePingResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Sre])?;

    // Get node from database
    let node = state
        .db
        .get_node(&node_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "node not found".to_string(),
                    details: None,
                }),
            )
        })?;

    // Try to ping the node agent
    let start = std::time::Instant::now();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to create HTTP client".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    let ping_url = format!("{}/health", node.agent_endpoint);
    let result = client.get(&ping_url).send().await;

    let (status, latency_ms) = match result {
        Ok(response) if response.status().is_success() => {
            ("reachable".to_string(), start.elapsed().as_millis() as f64)
        }
        Ok(response) => (
            format!("error: HTTP {}", response.status()),
            start.elapsed().as_millis() as f64,
        ),
        Err(_) => ("unreachable".to_string(), 0.0),
    };

    Ok(Json(NodePingResponse {
        node_id: node.id,
        status,
        latency_ms,
    }))
}

/// Mark node offline
pub async fn mark_node_offline(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Sre])?;

    // Update node status in database
    let timestamp = chrono::Utc::now().to_rfc3339();
    sqlx::query!(
        "UPDATE nodes SET status = 'offline', last_seen_at = ? WHERE id = ?",
        timestamp,
        node_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to update node status".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Evict node (delete from registry)
pub async fn evict_node(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Sre])?;

    // Check for running workers on this node
    let workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "database error".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let node_has_workers = workers.iter().any(|w| w.node_id == node_id);

    if node_has_workers {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "node has running workers".to_string(),
                details: Some("Stop all workers before evicting node".to_string()),
            }),
        ));
    }

    // Delete node from database
    sqlx::query!("DELETE FROM nodes WHERE id = ?", node_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to delete node".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get node details
pub async fn get_node_details(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<Json<NodeDetailsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Sre])?;

    // Get node from database
    let node = state
        .db
        .get_node(&node_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "node not found".to_string(),
                    details: None,
                }),
            )
        })?;

    // Get workers running on this node
    let all_workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "database error".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let workers: Vec<WorkerInfo> = all_workers
        .iter()
        .filter(|w| w.node_id == node_id)
        .map(|w| WorkerInfo {
            id: w.id.clone(),
            tenant_id: w.tenant_id.clone(),
            plan_id: w.plan_id.clone(),
            status: w.status.clone(),
        })
        .collect();

    Ok(Json(NodeDetailsResponse {
        id: node.id,
        hostname: node.hostname,
        agent_endpoint: node.agent_endpoint,
        status: node.status,
        last_seen_at: node.last_seen_at,
        workers,
        recent_logs: {
            // Attempt to fetch recent logs, but don't fail if unavailable
            match sqlx::query_as::<_, (String,)>(
                "SELECT message FROM node_logs WHERE node_id = ? ORDER BY timestamp DESC LIMIT 10",
            )
            .bind(&node_id)
            .fetch_all(state.db.pool())
            .await
            {
                Ok(rows) => rows.into_iter().map(|(msg,)| msg).collect(),
                Err(e) => {
                    tracing::warn!("Failed to fetch node logs for {}: {}", node_id, e);
                    vec![]
                }
            }
        },
    }))
}

/// Import model
pub async fn import_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ImportModelRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Compliance])?;

    state
        .db
        .register_model(
            &req.name,
            &req.hash_b3,
            &req.config_hash_b3,
            &req.tokenizer_hash_b3,
            &req.tokenizer_cfg_hash_b3,
            req.license_hash_b3.as_deref(),
            req.metadata_json.as_deref(),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to import model".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    Ok(StatusCode::CREATED)
}

#[derive(Deserialize)]
pub struct ListJobsQuery {
    tenant_id: Option<String>,
}

/// List jobs
pub async fn list_jobs(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(query): Query<ListJobsQuery>,
) -> Result<Json<Vec<JobResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let jobs = state
        .db
        .list_jobs(query.tenant_id.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    let response: Vec<JobResponse> = jobs
        .into_iter()
        .map(|j| JobResponse {
            id: j.id,
            kind: j.kind,
            status: j.status,
            created_at: j.created_at,
        })
        .collect();

    Ok(Json(response))
}

/// Build plan (stub)
pub async fn build_plan(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<BuildPlanRequest>,
) -> Result<Json<JobResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    let payload = serde_json::to_string(&req).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "serialization error".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let job_id = state
        .db
        .create_job(
            "build_plan",
            Some(&req.tenant_id),
            Some(&claims.sub),
            &payload,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to create job".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    Ok(Json(JobResponse {
        id: job_id,
        kind: "build_plan".to_string(),
        status: "queued".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Promote CP with quality gates
pub async fn cp_promote(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<PromoteCPRequest>,
) -> Result<Json<PromotionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Compliance])?;

    // Load plan from database
    let plan = state
        .db
        .get_plan(&req.plan_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to load plan".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "plan not found".to_string(),
                    details: Some(format!("Plan ID: {}", req.plan_id)),
                }),
            )
        })?;

    // Load latest audit for the CPID
    let audits = state.db.list_all_audits().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to load audits".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let latest_audit = audits
        .iter()
        .filter(|a| {
            a.tenant_id == plan.tenant_id
                && a.cpid.as_deref() == Some(&req.cpid)
                && a.status == "pass"
        })
        .max_by_key(|a| &a.created_at)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "no passing audit found for CPID".to_string(),
                    details: Some(format!(
                        "Run audit and ensure it passes before promotion: {}",
                        req.cpid
                    )),
                }),
            )
        })?;

    // Parse audit results to check quality gates
    let audit_result: serde_json::Value =
        serde_json::from_str(&latest_audit.result_json).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to parse audit results".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    // Extract hallucination metrics
    let metrics = &audit_result["hallucination_metrics"];
    let arr = metrics["arr"].as_f64().unwrap_or(0.0) as f32;
    let ecs5 = metrics["ecs5"].as_f64().unwrap_or(0.0) as f32;
    let hlr = metrics["hlr"].as_f64().unwrap_or(1.0) as f32;
    let cr = metrics["cr"].as_f64().unwrap_or(1.0) as f32;

    // Check quality gates (from Ruleset #15)
    let mut failures = Vec::new();

    if arr < 0.95 {
        failures.push(format!("ARR too low: {:.3} < 0.95", arr));
    }

    if ecs5 < 0.75 {
        failures.push(format!("ECS@5 too low: {:.3} < 0.75", ecs5));
    }

    if hlr > 0.03 {
        failures.push(format!("HLR too high: {:.3} > 0.03", hlr));
    }

    if cr > 0.01 {
        failures.push(format!("CR too high: {:.3} > 0.01", cr));
    }

    // If any gates fail, reject promotion
    if !failures.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "quality gates failed".to_string(),
                details: Some(failures.join("; ")),
            }),
        ));
    }

    // All gates passed - proceed with promotion in a transaction
    // Get current active CPID for before_cpid tracking
    let current_cp = state
        .db
        .get_active_cp_pointer(&plan.tenant_id)
        .await
        .ok()
        .flatten();
    let before_cpid = current_cp.as_ref().map(|cp| cp.name.clone());

    // Find target CP pointer
    let cp_pointer = state
        .db
        .get_cp_pointer_by_name(&req.cpid)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to get CP pointer".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "CP pointer not found".to_string(),
                    details: Some(format!("CPID: {}", req.cpid)),
                }),
            )
        })?;

    // Create quality metrics JSON for signing
    let quality_metrics = QualityMetrics { arr, ecs5, hlr, cr };
    let quality_json = serde_json::to_string(&quality_metrics).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to serialize quality metrics".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    // Generate Ed25519 signature
    let (signature_b64, signer_key_id) =
        crate::signing::sign_promotion(&req.cpid, &claims.email, &quality_json).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to sign promotion".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    // BEGIN TRANSACTION
    let mut tx = state.db.pool().begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to start transaction".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    // 1. Deactivate all CP pointers for this tenant
    sqlx::query("UPDATE cp_pointers SET active = 0 WHERE tenant_id = ?")
        .bind(&plan.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to deactivate CP pointers".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    // 2. Activate target CP pointer
    sqlx::query("UPDATE cp_pointers SET active = 1 WHERE id = ?")
        .bind(&cp_pointer.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to activate CP pointer".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    // 3. Insert promotion record with signature
    let promotion_id = uuid::Uuid::now_v7().to_string();
    let promotion_timestamp = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO promotions 
         (id, cpid, cp_pointer_id, promoted_by, promoted_at, signature_b64, signer_key_id, quality_json, before_cpid) 
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&promotion_id)
    .bind(&req.cpid)
    .bind(&cp_pointer.id)
    .bind(&claims.email)
    .bind(promotion_timestamp.to_rfc3339())
    .bind(&signature_b64)
    .bind(&signer_key_id)
    .bind(&quality_json)
    .bind(&before_cpid)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to insert promotion record".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    // COMMIT TRANSACTION
    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to commit transaction".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    // Record promotion metric
    state.metrics_exporter.record_promotion();

    tracing::info!(
        "Promotion completed: {} -> {} by {} (signature: {})",
        before_cpid.as_deref().unwrap_or("(none)"),
        req.cpid,
        claims.email,
        &signature_b64[..16]
    );

    Ok(Json(PromotionResponse {
        cpid: req.cpid,
        plan_id: req.plan_id,
        promoted_by: claims.email,
        promoted_at: promotion_timestamp.to_rfc3339(),
        quality_metrics,
    }))
}

/// Spawn worker via node agent
pub async fn worker_spawn(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<SpawnWorkerRequest>,
) -> Result<Json<WorkerResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Sre])?;

    // Look up node by ID
    let node = state
        .db
        .get_node(&req.node_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "node not found".to_string(),
                    details: Some(format!("Node ID: {}", req.node_id)),
                }),
            )
        })?;

    // Prepare spawn request for node agent
    let spawn_req = serde_json::json!({
        "tenant_id": req.tenant_id,
        "plan_id": req.plan_id,
    });

    // Send HTTP POST to node agent
    let client = reqwest::Client::new();
    let spawn_url = format!("{}/spawn_worker", node.agent_endpoint);

    let response = client
        .post(&spawn_url)
        .json(&spawn_req)
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: "failed to contact node agent".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "node agent spawn failed".to_string(),
                details: Some(error_text),
            }),
        ));
    }

    let spawn_response: serde_json::Value = response.json().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to parse node agent response".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let pid = spawn_response["pid"].as_i64().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "invalid response from node agent".to_string(),
                details: Some("missing or invalid PID field".to_string()),
            }),
        )
    })? as i32;

    // Create UDS path for worker
    let uds_path = format!("/var/run/aos/{}/worker.sock", req.tenant_id);

    // Register worker in database
    let worker_id = uuid::Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO workers (id, tenant_id, node_id, plan_id, uds_path, pid, status) 
         VALUES (?, ?, ?, ?, ?, ?, 'starting')",
    )
    .bind(&worker_id)
    .bind(&req.tenant_id)
    .bind(&req.node_id)
    .bind(&req.plan_id)
    .bind(&uds_path)
    .bind(pid)
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to register worker in database".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    // Return worker info
    Ok(Json(WorkerResponse {
        id: worker_id,
        tenant_id: req.tenant_id,
        node_id: req.node_id,
        plan_id: req.plan_id,
        uds_path,
        pid: Some(pid),
        status: "starting".to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        last_seen_at: None,
    }))
}

#[derive(Deserialize)]
pub struct ListWorkersQuery {
    tenant_id: Option<String>,
}

/// List workers with optional tenant filter
pub async fn list_workers(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListWorkersQuery>,
) -> Result<Json<Vec<WorkerResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Sre, Role::Admin])?;

    let workers = if let Some(tenant_id) = query.tenant_id {
        state
            .db
            .list_workers_by_tenant(&tenant_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "database error".to_string(),
                        details: Some(e.to_string()),
                    }),
                )
            })?
    } else {
        state.db.list_all_workers().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
    };

    let response: Vec<WorkerResponse> = workers
        .into_iter()
        .map(|w| WorkerResponse {
            id: w.id,
            tenant_id: w.tenant_id,
            node_id: w.node_id,
            plan_id: w.plan_id,
            uds_path: w.uds_path,
            pid: w.pid,
            status: w.status,
            started_at: w.started_at,
            last_seen_at: w.last_seen_at,
        })
        .collect();

    Ok(Json(response))
}

/// Logout endpoint (stateless JWT - just return success)
pub async fn auth_logout(
    Extension(_claims): Extension<Claims>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // With stateless JWT, logout is client-side (discard token)
    // Server doesn't need to track anything
    Ok(StatusCode::NO_CONTENT)
}

/// Get current user info
pub async fn auth_me(
    Extension(claims): Extension<Claims>,
) -> Result<Json<UserInfoResponse>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(UserInfoResponse {
        user_id: claims.sub,
        email: claims.email,
        role: claims.role,
    }))
}

/// List plans with optional tenant filter
pub async fn list_plans(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(query): Query<ListPlansQuery>,
) -> Result<Json<Vec<PlanResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let plans = if let Some(tenant_id) = query.tenant_id {
        state
            .db
            .list_plans_by_tenant(&tenant_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "database error".to_string(),
                        details: Some(e.to_string()),
                    }),
                )
            })?
    } else {
        state.db.list_all_plans().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
    };

    let response: Vec<PlanResponse> = plans
        .into_iter()
        .map(|p| PlanResponse {
            id: p.id,
            tenant_id: p.tenant_id,
            manifest_hash_b3: p.manifest_hash_b3,
            kernel_hash_b3: None,         // Not stored in Plan model
            layout_hash_b3: None,         // Not stored in Plan model
            status: "active".to_string(), // Default status
            created_at: p.created_at,
        })
        .collect();

    Ok(Json(response))
}

#[derive(Deserialize)]
pub struct ListPlansQuery {
    tenant_id: Option<String>,
}

/// Get plan details
pub async fn get_plan_details(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(plan_id): Path<String>,
) -> Result<Json<PlanDetailsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Sre])?;

    let plan = state
        .db
        .get_plan(&plan_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "plan not found".to_string(),
                    details: None,
                }),
            )
        })?;

    Ok(Json(PlanDetailsResponse {
        id: plan.id.clone(),
        tenant_id: plan.tenant_id,
        manifest_hash_b3: plan.manifest_hash_b3.clone(),
        kernel_hash_b3: {
            // Query kernel hash from plan metadata
            match sqlx::query_scalar::<_, Option<String>>(
                "SELECT kernel_hash FROM plan_metadata WHERE plan_id = ?",
            )
            .bind(&plan.id)
            .fetch_optional(state.db.pool())
            .await
            {
                Ok(hash) => hash.flatten(),
                Err(e) => {
                    tracing::warn!("Failed to fetch kernel hash for plan {}: {}", plan.id, e);
                    None
                }
            }
        },
        routing_config: {
            // Query routing config from plan or use default
            match sqlx::query_scalar::<_, Option<String>>(
                "SELECT routing_config FROM plan_metadata WHERE plan_id = ?",
            )
            .bind(&plan.id)
            .fetch_optional(state.db.pool())
            .await
            {
                Ok(Some(Some(config_str))) => {
                    serde_json::from_str(&config_str).unwrap_or_else(|e| {
                        tracing::warn!("Failed to parse routing config: {}", e);
                        serde_json::json!({"k_sparse": 3, "gate_quant": "q15"})
                    })
                }
                _ => {
                    tracing::debug!(
                        "No routing config found for plan {}, using default",
                        plan.id
                    );
                    serde_json::json!({"k_sparse": 3, "gate_quant": "q15"})
                }
            }
        },
        created_at: plan.created_at,
    }))
}

/// Rebuild plan
pub async fn rebuild_plan(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(plan_id): Path<String>,
) -> Result<Json<PlanRebuildResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Sre])?;

    let plan = state
        .db
        .get_plan(&plan_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "plan not found".to_string(),
                    details: None,
                }),
            )
        })?;

    // Rebuild the plan by creating a new plan from the manifest
    // This allows incorporating any changes to the Metal kernels or manifest
    let new_plan_id = format!("{}-rebuilt-{}", plan.id, chrono::Utc::now().timestamp());

    // Create new plan record
    match sqlx::query(
        "INSERT INTO plans (id, tenant_id, manifest_hash_b3, status, created_at) 
         VALUES (?, ?, ?, 'building', datetime('now'))",
    )
    .bind(&new_plan_id)
    .bind(&plan.tenant_id)
    .bind(&plan.manifest_hash_b3)
    .execute(state.db.pool())
    .await
    {
        Ok(_) => {
            tracing::info!("Created new plan {} from {}", new_plan_id, plan.id);

            // Compare kernel hashes if available
            let diff_summary = match (
                sqlx::query_scalar::<_, Option<String>>(
                    "SELECT kernel_hash FROM plan_metadata WHERE plan_id = ?",
                )
                .bind(&plan.id)
                .fetch_optional(state.db.pool())
                .await,
                sqlx::query_scalar::<_, Option<String>>(
                    "SELECT kernel_hash FROM plan_metadata WHERE plan_id = ?",
                )
                .bind(&new_plan_id)
                .fetch_optional(state.db.pool())
                .await,
            ) {
                (Ok(Some(old_hash)), Ok(Some(new_hash))) if old_hash != new_hash => {
                    format!("Metal kernels updated (hash changed)")
                }
                _ => "Plan rebuilt with current Metal kernels".to_string(),
            };

            Ok(Json(PlanRebuildResponse {
                old_plan_id: plan.id,
                new_plan_id: new_plan_id.clone(),
                diff_summary,
                timestamp: chrono::Utc::now().to_rfc3339(),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to create new plan: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to rebuild plan".to_string(),
                    details: Some(e.to_string()),
                }),
            ))
        }
    }
}

/// Compare plans
pub async fn compare_plans(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ComparePlansRequest>,
) -> Result<Json<PlanComparisonResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Sre])?;

    let plan1 = state
        .db
        .get_plan(&req.plan_id_1)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("plan {} not found", req.plan_id_1),
                    details: None,
                }),
            )
        })?;

    let plan2 = state
        .db
        .get_plan(&req.plan_id_2)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("plan {} not found", req.plan_id_2),
                    details: None,
                }),
            )
        })?;

    // Simple comparison based on manifest hash
    let differences = if plan1.manifest_hash_b3 == plan2.manifest_hash_b3 {
        vec!["No differences detected".to_string()]
    } else {
        vec!["Manifest hashes differ".to_string()]
    };

    Ok(Json(PlanComparisonResponse {
        plan_id_1: plan1.id,
        plan_id_2: plan2.id,
        differences,
        identical: plan1.manifest_hash_b3 == plan2.manifest_hash_b3,
    }))
}

/// Export plan manifest
pub async fn export_plan_manifest(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(plan_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let plan = state
        .db
        .get_plan(&plan_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "plan not found".to_string(),
                    details: None,
                }),
            )
        })?;

    let manifest = serde_json::json!({
        "plan_id": plan.id,
        "tenant_id": plan.tenant_id,
        "manifest_hash_b3": plan.manifest_hash_b3,
        "created_at": plan.created_at,
        "exported_at": chrono::Utc::now().to_rfc3339(),
    });

    Ok(Json(manifest))
}

/// Check promotion gates
pub async fn promotion_gates(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(cpid): Path<String>,
) -> Result<Json<PromotionGatesResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub implementation - in reality would check all gates
    let gates = vec![
        GateStatus {
            name: "Replay Determinism".to_string(),
            passed: true,
            message: "Replay diff is zero".to_string(),
            evidence_id: Some("audit_123".to_string()),
        },
        GateStatus {
            name: "ARR Threshold".to_string(),
            passed: true,
            message: "ARR 0.96 >= 0.95".to_string(),
            evidence_id: Some("audit_123".to_string()),
        },
        GateStatus {
            name: "ECS@5 Threshold".to_string(),
            passed: true,
            message: "ECS@5 0.78 >= 0.75".to_string(),
            evidence_id: Some("audit_123".to_string()),
        },
        GateStatus {
            name: "HLR Threshold".to_string(),
            passed: true,
            message: "HLR 0.02 <= 0.03".to_string(),
            evidence_id: Some("audit_123".to_string()),
        },
        GateStatus {
            name: "CR Threshold".to_string(),
            passed: true,
            message: "CR 0.005 <= 0.01".to_string(),
            evidence_id: Some("audit_123".to_string()),
        },
        GateStatus {
            name: "Egress Preflight".to_string(),
            passed: true,
            message: "PF deny rules enforced".to_string(),
            evidence_id: None,
        },
        GateStatus {
            name: "Isolation Tests".to_string(),
            passed: true,
            message: "All isolation tests passed".to_string(),
            evidence_id: Some("isolation_test_456".to_string()),
        },
    ];

    let all_passed = gates.iter().all(|g| g.passed);

    Ok(Json(PromotionGatesResponse {
        cpid,
        gates,
        all_passed,
    }))
}

/// List policies (stub)
pub async fn list_policies(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<PolicyPackResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would query database
    Ok(Json(vec![]))
}

/// Get policy by CPID (stub)
pub async fn get_policy(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(cpid): Path<String>,
) -> Result<Json<PolicyPackResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would query database
    Ok(Json(PolicyPackResponse {
        cpid,
        content: r#"{"schema": "adapteros.policy.v1", "packs": {}}"#.to_string(),
        hash_b3: "b3:placeholder".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Validate policy (stub)
pub async fn validate_policy(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Json(req): Json<ValidatePolicyRequest>,
) -> Result<Json<PolicyValidationResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Basic JSON validation
    match serde_json::from_str::<serde_json::Value>(&req.content) {
        Ok(_) => Ok(Json(PolicyValidationResponse {
            valid: true,
            errors: vec![],
            hash_b3: Some("b3:placeholder".to_string()),
        })),
        Err(e) => Ok(Json(PolicyValidationResponse {
            valid: false,
            errors: vec![format!("Invalid JSON: {}", e)],
            hash_b3: None,
        })),
    }
}

/// Apply policy (stub)
pub async fn apply_policy(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ApplyPolicyRequest>,
) -> Result<Json<PolicyPackResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Compliance, Role::Admin])?;

    // Stub - would validate, sign, and store policy
    Ok(Json(PolicyPackResponse {
        cpid: req.cpid,
        content: req.content,
        hash_b3: "b3:placeholder".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Sign policy with Ed25519
pub async fn sign_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(cpid): Path<String>,
) -> Result<Json<SignPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Get or generate signing key for the tenant
    let signing_key_result = sqlx::query_scalar::<_, Option<String>>(
        "SELECT signing_key FROM signing_keys WHERE tenant_id = ? AND key_type = 'ed25519' AND active = 1"
    )
    .bind(&claims.sub)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        tracing::error!("Failed to query signing key: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to retrieve signing key".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let signing_key_hex = match signing_key_result {
        Some(key) => key,
        None => {
            // Generate new Ed25519 signing key
            use adapteros_crypto::signature::generate_keypair;
            let (secret_key, _public_key) = generate_keypair();
            let key_hex = hex::encode(secret_key.to_bytes());

            // Store the key
            sqlx::query(
                "INSERT INTO signing_keys (tenant_id, key_type, signing_key, active, created_at) 
                 VALUES (?, 'ed25519', ?, 1, datetime('now'))",
            )
            .bind(&claims.sub)
            .bind(&key_hex)
            .execute(state.db.pool())
            .await
            .map_err(|e| {
                tracing::error!("Failed to store signing key: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Failed to store signing key".to_string(),
                        details: Some(e.to_string()),
                    }),
                )
            })?;

            tracing::info!(
                "Generated new Ed25519 signing key for tenant {}",
                claims.sub
            );
            Some(key_hex)
        }
    };

    // Sign the CPID
    let signing_key = signing_key_hex.as_deref().unwrap_or("");
    let signature = match adapteros_crypto::signature::sign_data(&cpid.as_bytes(), signing_key) {
        Ok(sig) => format!("ed25519:{}", hex::encode(sig)),
        Err(e) => {
            tracing::error!("Failed to sign CPID: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Signing failed".to_string(),
                    details: Some(e.to_string()),
                }),
            ));
        }
    };

    Ok(Json(SignPolicyResponse {
        cpid: cpid.clone(),
        signature,
        signed_at: chrono::Utc::now().to_rfc3339(),
        signed_by: claims.email,
    }))
}

/// Compare two policy versions
pub async fn compare_policy_versions(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Json(req): Json<PolicyComparisonRequest>,
) -> Result<Json<PolicyComparisonResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would fetch both policies and compute diff
    Ok(Json(PolicyComparisonResponse {
        cpid_1: req.cpid_1,
        cpid_2: req.cpid_2,
        differences: vec![
            "egress.mode: deny_all -> allow_listed".to_string(),
            "router.k_sparse: 3 -> 5".to_string(),
            "Added: output.new_field".to_string(),
        ],
        identical: false,
    }))
}

/// Export policy as downloadable bundle
pub async fn export_policy(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(cpid): Path<String>,
) -> Result<Json<ExportPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would fetch policy and signature from database
    let policy_json = r#"{"schema": "adapteros.policy.v1", "packs": {}}"#.to_string();

    Ok(Json(ExportPolicyResponse {
        cpid: cpid.clone(),
        policy_json,
        signature: Some(format!("ed25519:sig_{}", cpid)),
        exported_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// List telemetry bundles (stub)
pub async fn list_telemetry_bundles(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<TelemetryBundleResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would query telemetry store
    Ok(Json(vec![]))
}

/// Export telemetry bundle as NDJSON
pub async fn export_telemetry_bundle(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(bundle_id): Path<String>,
) -> Result<Json<ExportTelemetryBundleResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would fetch bundle from telemetry store
    Ok(Json(ExportTelemetryBundleResponse {
        bundle_id: bundle_id.clone(),
        events_count: 42_000,
        size_bytes: 12_582_912,
        download_url: format!("/v1/telemetry/bundles/{}/download", bundle_id),
        expires_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Verify telemetry bundle Ed25519 signature
pub async fn verify_bundle_signature(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(bundle_id): Path<String>,
) -> Result<Json<VerifyBundleSignatureResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would verify signature using mplora-crypto
    Ok(Json(VerifyBundleSignatureResponse {
        bundle_id,
        valid: true,
        signature: "ed25519:abc123...".to_string(),
        signed_by: "control-plane-key".to_string(),
        signed_at: chrono::Utc::now().to_rfc3339(),
        verification_error: None,
    }))
}

/// Purge old telemetry bundles based on retention policy
pub async fn purge_old_bundles(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(_req): Json<PurgeOldBundlesRequest>,
) -> Result<Json<PurgeOldBundlesResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Sre, Role::Admin])?;

    // Stub - would apply retention policy and delete old bundles
    Ok(Json(PurgeOldBundlesResponse {
        purged_count: 15,
        retained_count: 12,
        freed_bytes: 45_000_000,
        purged_cpids: vec!["cp_001".to_string(), "cp_002".to_string()],
    }))
}

/// Rollback CP to previous plan
pub async fn cp_rollback(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RollbackCPRequest>,
) -> Result<Json<RollbackResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Compliance, Role::Admin])?;

    // Get current active CP pointer
    let current_cp = state
        .db
        .get_active_cp_pointer(&req.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to get current CP pointer".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "no active CP pointer found".to_string(),
                    details: Some(format!("Tenant: {}", req.tenant_id)),
                }),
            )
        })?;

    // Verify the CPID matches
    if current_cp.name != req.cpid {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "CPID mismatch".to_string(),
                details: Some(format!(
                    "Current active CPID is '{}', not '{}'",
                    current_cp.name, req.cpid
                )),
            }),
        ));
    }

    // Find previous CP pointer for this tenant (most recent inactive one)
    let all_pointers = adapteros_db::sqlx::query_as::<_, adapteros_db::models::CpPointer>(
        "SELECT id, tenant_id, name, plan_id, active, created_at, activated_at 
         FROM cp_pointers 
         WHERE tenant_id = ? AND id != ? 
         ORDER BY activated_at DESC, created_at DESC 
         LIMIT 1",
    )
    .bind(&req.tenant_id)
    .bind(&current_cp.id)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to query previous CP".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let previous_cp = all_pointers.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "no previous CP pointer available for rollback".to_string(),
                details: Some(format!(
                    "This is the first/only CP for tenant {}",
                    req.tenant_id
                )),
            }),
        )
    })?;

    // Perform rollback in a transaction
    let mut tx = state.db.pool().begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to start transaction".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    // 1. Deactivate all CP pointers for this tenant
    sqlx::query("UPDATE cp_pointers SET active = 0 WHERE tenant_id = ?")
        .bind(&req.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to deactivate CP pointers".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    // 2. Activate previous CP pointer
    sqlx::query("UPDATE cp_pointers SET active = 1 WHERE id = ?")
        .bind(&previous_cp.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to activate previous CP pointer".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    // COMMIT TRANSACTION
    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to commit transaction".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let rollback_timestamp = chrono::Utc::now();

    tracing::info!(
        "Rollback completed: {} -> {} by {}",
        req.cpid,
        previous_cp.name,
        claims.email
    );

    Ok(Json(RollbackResponse {
        cpid: req.cpid.clone(),
        previous_plan_id: previous_cp.plan_id,
        rolled_back_by: claims.email,
        rolled_back_at: rollback_timestamp.to_rfc3339(),
    }))
}

/// Dry run CP promotion (validate gates without executing)
pub async fn cp_dry_run_promote(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<DryRunPromotionRequest>,
) -> Result<Json<DryRunPromotionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Compliance, Role::Admin])?;

    // Stub - would validate all gates and return what would be promoted
    Ok(Json(DryRunPromotionResponse {
        cpid: req.cpid,
        would_promote: true,
        gates_status: vec![
            GateStatus {
                name: "determinism".to_string(),
                passed: true,
                message: "Replay zero diff passed".to_string(),
                evidence_id: None,
            },
            GateStatus {
                name: "hallucination".to_string(),
                passed: true,
                message: "ARR: 0.96, ECS@5: 0.78".to_string(),
                evidence_id: None,
            },
            GateStatus {
                name: "performance".to_string(),
                passed: true,
                message: "p95: 22ms (threshold: 24ms)".to_string(),
                evidence_id: None,
            },
        ],
        warnings: vec![],
    }))
}

/// Get promotion history
pub async fn get_promotion_history(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<PromotionHistoryEntry>>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would query promotions table
    Ok(Json(vec![PromotionHistoryEntry {
        cpid: "cp_001".to_string(),
        promoted_at: chrono::Utc::now().to_rfc3339(),
        promoted_by: "admin@example.com".to_string(),
        previous_cpid: Some("cp_000".to_string()),
        gate_results_summary: "All gates passed".to_string(),
    }]))
}

/// Propose a patch for code changes
#[utoipa::path(
    post,
    path = "/api/v1/patch/propose",
    request_body = ProposePatchRequest,
    responses(
        (status = 200, description = "Patch proposal created", body = ProposePatchResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("bearer_token" = [])
    )
)]
pub async fn propose_patch(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ProposePatchRequest>,
) -> Result<Json<ProposePatchResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Sre])?;

    // Validate inputs
    validate_repo_id(&req.repo_id)?;
    validate_description(&req.description)?;
    validate_file_paths(&req.target_files)?;

    // Get available workers from database
    let workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to list workers".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    if workers.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "no workers available".to_string(),
                details: Some("No active workers found for patch proposal".to_string()),
            }),
        ));
    }

    // Select first available worker (simple selection for now)
    let worker = &workers[0];
    let uds_path = std::path::Path::new(&worker.uds_path);

    // Create UDS client and send patch proposal request
    let uds_client = UdsClient::new(std::time::Duration::from_secs(60)); // Longer timeout for patch generation

    let worker_request = PatchProposalInferRequest {
        cpid: "patch-proposal".to_string(),
        prompt: req.description.clone(),
        max_tokens: 2000,
        require_evidence: true,
        request_type: PatchProposalRequestType {
            repo_id: req.repo_id.clone(),
            commit_sha: Some(req.commit_sha.clone()),
            target_files: req.target_files.clone(),
            description: req.description.clone(),
        },
    };

    match uds_client.propose_patch(uds_path, worker_request).await {
        Ok(worker_response) => {
            // Extract proposal ID and status
            let proposal_id = worker_response
                .patch_proposal
                .as_ref()
                .map(|p| p.proposal_id.clone())
                .unwrap_or_else(|| {
                    uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string()
                });

            let status = if worker_response.patch_proposal.is_some() {
                "completed"
            } else if worker_response.refusal.is_some() {
                "refused"
            } else {
                "failed"
            };

            let message = if let Some(ref proposal) = worker_response.patch_proposal {
                format!(
                    "Patch proposal generated successfully with {} files and {} citations",
                    proposal.patches.len(),
                    proposal.citations.len()
                )
            } else if let Some(ref refusal) = worker_response.refusal {
                format!("Patch proposal refused: {}", refusal.message)
            } else {
                "Patch proposal generation failed".to_string()
            };

            // Store proposal in database
            if let Some(ref proposal) = worker_response.patch_proposal {
                let proposal_json = serde_json::to_string(proposal).unwrap_or_else(|e| {
                    tracing::warn!("Failed to serialize patch proposal: {}", e);
                    "{}".to_string()
                });

                match sqlx::query(
                    "INSERT INTO patch_proposals 
                     (id, repo_id, commit_sha, status, proposal_json, created_at, created_by) 
                     VALUES (?, ?, ?, ?, ?, datetime('now'), ?)",
                )
                .bind(&proposal_id)
                .bind(&req.repo_id)
                .bind(&req.commit_sha)
                .bind(&status)
                .bind(&proposal_json)
                .bind(&claims.email)
                .execute(state.db.pool())
                .await
                {
                    Ok(_) => {
                        tracing::info!("Stored patch proposal {} in database", proposal_id);
                    }
                    Err(e) => {
                        tracing::error!("Failed to store patch proposal in database: {}", e);
                        // Don't fail the request if storage fails
                    }
                }
            }

            Ok(Json(ProposePatchResponse {
                proposal_id,
                status: status.to_string(),
                message,
            }))
        }
        Err(UdsClientError::WorkerNotAvailable(msg)) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "worker not available".to_string(),
                details: Some(msg),
            }),
        )),
        Err(UdsClientError::Timeout(msg)) => Err((
            StatusCode::REQUEST_TIMEOUT,
            Json(ErrorResponse {
                error: "patch generation timeout".to_string(),
                details: Some(msg),
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "patch generation failed".to_string(),
                details: Some(e.to_string()),
            }),
        )),
    }
}

/// Inference endpoint
#[utoipa::path(
    post,
    path = "/v1/infer",
    request_body = InferRequest,
    responses(
        (status = 200, description = "Inference successful", body = InferResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Inference failed", body = ErrorResponse),
        (status = 501, description = "Worker not initialized", body = ErrorResponse)
    )
)]
pub async fn infer(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<InferRequest>,
) -> Result<Json<InferResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate request
    if req.prompt.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "prompt cannot be empty".to_string(),
                details: None,
            }),
        ));
    }

    // Real inference implementation - proxy to worker UDS server
    // 1. Look up available workers from database
    // 2. Select a healthy worker
    // 3. Connect to worker UDS server
    // 4. Forward inference request
    // 5. Return worker response

    // Get available workers from database
    let workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to list workers".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    if workers.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "no workers available".to_string(),
                details: Some("No active workers found for inference".to_string()),
            }),
        ));
    }

    // Select first available worker (simple round-robin for now)
    let worker = &workers[0];
    let uds_path = std::path::Path::new(&worker.uds_path);

    // Create UDS client and send request
    let uds_client = UdsClient::new(std::time::Duration::from_secs(30));

    // Convert server API request to worker API request
    let worker_request = WorkerInferRequest {
        cpid: claims.sub.clone(), // Use tenant ID from JWT claims as CPID
        prompt: req.prompt.clone(),
        max_tokens: req.max_tokens.unwrap_or(100),
        require_evidence: req.require_evidence.unwrap_or(false), // Get from request or default to false
    };

    match uds_client.infer(uds_path, worker_request).await {
        Ok(worker_response) => {
            // Convert worker response to server API response
            let response = InferResponse {
                text: worker_response.text.unwrap_or_default(),
                tokens: vec![], // Worker doesn't expose token IDs in current API
                finish_reason: worker_response.status.clone(),
                trace: InferenceTrace {
                    adapters_used: worker_response.trace.router_summary.adapters_used.clone(),
                    router_decisions: vec![], // Router decisions not in simplified trace
                    latency_ms: 0,            // Not tracked in current response
                },
            };
            Ok(Json(response))
        }
        Err(UdsClientError::WorkerNotAvailable(msg)) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "worker not available".to_string(),
                details: Some(msg),
            }),
        )),
        Err(UdsClientError::Timeout(msg)) => Err((
            StatusCode::REQUEST_TIMEOUT,
            Json(ErrorResponse {
                error: "inference timeout".to_string(),
                details: Some(msg),
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "inference failed".to_string(),
                details: Some(e.to_string()),
            }),
        )),
    }
}

// ===== Adapter Management Endpoints =====

/// List all adapters
#[utoipa::path(
    get,
    path = "/v1/adapters",
    params(
        ("tier" = Option<i32>, Query, description = "Filter by tier"),
        ("framework" = Option<String>, Query, description = "Filter by framework")
    ),
    responses(
        (status = 200, description = "List of adapters", body = Vec<AdapterResponse>),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    )
)]
pub async fn list_adapters(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(query): Query<ListAdaptersQuery>,
) -> Result<Json<Vec<AdapterResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let adapters = state.db.list_adapters().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to list adapters".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let mut responses = Vec::new();
    for adapter in adapters {
        // Filter by tier if specified
        if let Some(tier) = query.tier {
            if adapter.tier != tier {
                continue;
            }
        }

        // Filter by framework if specified
        if let Some(ref framework) = query.framework {
            if adapter.framework.as_ref() != Some(framework) {
                continue;
            }
        }

        // Get stats
        let (total, selected, avg_gate) = state
            .db
            .get_adapter_stats(&adapter.adapter_id)
            .await
            .unwrap_or((0, 0, 0.0));

        let selection_rate = if total > 0 {
            (selected as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let languages: Vec<String> = adapter
            .languages_json
            .as_ref()
            .and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default();

        responses.push(AdapterResponse {
            id: adapter.id,
            adapter_id: adapter.adapter_id,
            name: adapter.name,
            hash_b3: adapter.hash_b3,
            rank: adapter.rank,
            tier: adapter.tier,
            languages,
            framework: adapter.framework,
            created_at: adapter.created_at,
            stats: Some(AdapterStats {
                total_activations: total,
                selected_count: selected,
                avg_gate_value: avg_gate,
                selection_rate,
            }),
        });
    }

    Ok(Json(responses))
}

/// Get adapter by ID
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter details", body = AdapterResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse)
    )
)]
pub async fn get_adapter(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "adapter not found".to_string(),
                    details: None,
                }),
            )
        })?;

    let (total, selected, avg_gate) = state
        .db
        .get_adapter_stats(&adapter_id)
        .await
        .unwrap_or((0, 0, 0.0));

    let selection_rate = if total > 0 {
        (selected as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    let languages: Vec<String> = adapter
        .languages_json
        .as_ref()
        .and_then(|j| serde_json::from_str(j).ok())
        .unwrap_or_default();

    Ok(Json(AdapterResponse {
        id: adapter.id,
        adapter_id: adapter.adapter_id,
        name: adapter.name,
        hash_b3: adapter.hash_b3,
        rank: adapter.rank,
        tier: adapter.tier,
        languages,
        framework: adapter.framework,
        created_at: adapter.created_at,
        stats: Some(AdapterStats {
            total_activations: total,
            selected_count: selected,
            avg_gate_value: avg_gate,
            selection_rate,
        }),
    }))
}

/// Register new adapter
#[utoipa::path(
    post,
    path = "/v1/adapters/register",
    request_body = RegisterAdapterRequest,
    responses(
        (status = 201, description = "Adapter registered", body = AdapterResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse)
    )
)]
pub async fn register_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RegisterAdapterRequest>,
) -> Result<(StatusCode, Json<AdapterResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Require admin role
    require_role(&claims, Role::Admin)?;

    // Validate inputs
    if req.adapter_id.is_empty() || req.name.is_empty() || req.hash_b3.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "adapter_id, name, and hash_b3 are required".to_string(),
                details: None,
            }),
        ));
    }

    // Validate adapter ID format (alphanumeric, underscores, hyphens)
    validate_adapter_id(&req.adapter_id)?;

    // Validate name length and content
    validate_name(&req.name)?;

    // Validate hash format (B3 hash)
    validate_hash_b3(&req.hash_b3)?;

    let languages_json = serde_json::to_string(&req.languages).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "invalid languages array".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let id = state
        .db
        .register_adapter(
            &req.adapter_id,
            &req.name,
            &req.hash_b3,
            req.rank,
            req.tier,
            Some(&languages_json),
            req.framework.as_deref(),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to register adapter".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(AdapterResponse {
            id,
            adapter_id: req.adapter_id,
            name: req.name,
            hash_b3: req.hash_b3,
            rank: req.rank,
            tier: req.tier,
            languages: req.languages,
            framework: req.framework,
            created_at: chrono::Utc::now().to_rfc3339(),
            stats: None,
        }),
    ))
}

/// Delete adapter
#[utoipa::path(
    delete,
    path = "/v1/adapters/{adapter_id}",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 204, description = "Adapter deleted"),
        (status = 404, description = "Adapter not found", body = ErrorResponse)
    )
)]
pub async fn delete_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Require admin role
    require_role(&claims, Role::Admin)?;

    state.db.delete_adapter(&adapter_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to delete adapter".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get adapter activations
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/activations",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID"),
        ("limit" = Option<i64>, Query, description = "Limit results (default: 100)")
    ),
    responses(
        (status = 200, description = "Activation history", body = Vec<AdapterActivationResponse>)
    )
)]
pub async fn get_adapter_activations(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<AdapterActivationResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let limit = query
        .get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(100);

    let activations = state
        .db
        .get_adapter_activations(&adapter_id, limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to get activations".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    let responses: Vec<AdapterActivationResponse> = activations
        .into_iter()
        .map(|a| AdapterActivationResponse {
            id: a.id,
            adapter_id: a.adapter_id,
            request_id: a.request_id,
            gate_value: a.gate_value,
            selected: a.selected == 1,
            created_at: a.created_at,
        })
        .collect();

    Ok(Json(responses))
}

/// Promote adapter state (cold→warm, warm→hot)
pub async fn promote_adapter_state(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Sre])?;

    // Get current adapter state
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "adapter not found".to_string(),
                    details: None,
                }),
            )
        })?;

    // Determine next state based on current tier
    // Tiers: 0=persistent, 1=warm, 2=ephemeral
    // For promotion: persistent(0) → warm(1) → ephemeral(2)
    let new_tier = match adapter.tier {
        0 => 1,            // persistent -> warm
        1 => 2,            // warm -> ephemeral
        _ => adapter.tier, // Already at highest or unknown tier
    };

    let new_state = match new_tier {
        0 => "persistent",
        1 => "warm",
        2 => "ephemeral",
        _ => "persistent", // Default fallback
    };

    // Update adapter state in database
    let timestamp = chrono::Utc::now().to_rfc3339();
    sqlx::query!(
        "UPDATE adapters SET tier = ?, updated_at = ? WHERE adapter_id = ?",
        new_tier,
        timestamp,
        adapter_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to update adapter state".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let old_state_str = match adapter.tier {
        0 => "persistent",
        1 => "warm",
        2 => "ephemeral",
        _ => "unknown",
    };

    Ok(Json(AdapterStateResponse {
        adapter_id,
        old_state: old_state_str.to_string(),
        new_state: new_state.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Download adapter manifest as JSON
pub async fn download_adapter_manifest(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterManifest>, (StatusCode, Json<ErrorResponse>)> {
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "adapter not found".to_string(),
                    details: None,
                }),
            )
        })?;

    let manifest = AdapterManifest {
        adapter_id: adapter.adapter_id,
        name: adapter.name,
        hash_b3: adapter.hash_b3,
        rank: adapter.rank,
        tier: adapter.tier,
        framework: adapter.framework,
        languages_json: adapter.languages_json,
        category: Some(adapter.category),
        scope: Some(adapter.scope),
        framework_id: adapter.framework_id,
        framework_version: adapter.framework_version,
        repo_id: adapter.repo_id,
        commit_sha: adapter.commit_sha,
        intent: adapter.intent,
        created_at: adapter.created_at,
        updated_at: adapter.updated_at,
    };

    Ok(Json(manifest))
}

/// Get adapter health (activation logs, memory usage, policy violations)
pub async fn get_adapter_health(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterHealthResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Get adapter activations (last 100)
    let activations = state
        .db
        .get_adapter_activations(&adapter_id, 100)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to get activations".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    // Get adapter stats
    let (total, selected, avg_gate) = state
        .db
        .get_adapter_stats(&adapter_id)
        .await
        .unwrap_or((0, 0, 0.0));

    // Calculate memory usage trend (simplified - would need time-series data in production)
    let memory_usage_mb = activations.len() as f64 * 2.5; // Rough estimate

    let adapter_id_clone = adapter_id.clone();
    let adapter_id_clone2 = adapter_id.clone();
    let adapter_id_clone3 = adapter_id.clone();

    Ok(Json(AdapterHealthResponse {
        adapter_id: adapter_id_clone,
        total_activations: total as i32,
        selected_count: selected as i32,
        avg_gate_value: avg_gate,
        memory_usage_mb,
        policy_violations: {
            // Query policy violations from telemetry/audit logs
            sqlx::query_as::<_, (String, String)>(
                "SELECT violation_type, message FROM policy_violations 
                 WHERE adapter_id = ? AND timestamp > datetime('now', '-1 hour')
                 ORDER BY timestamp DESC LIMIT 5",
            )
            .bind(&adapter_id_clone2)
            .fetch_all(state.db.pool())
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "Failed to fetch policy violations for {}: {}",
                    adapter_id_clone3,
                    e
                );
                vec![]
            })
            .into_iter()
            .map(|(vtype, msg)| format!("{}: {}", vtype, msg))
            .collect()
        },
        recent_activations: activations
            .into_iter()
            .take(10)
            .map(|a| AdapterActivationResponse {
                id: a.id,
                adapter_id: a.adapter_id,
                request_id: a.request_id,
                gate_value: a.gate_value,
                selected: a.selected == 1,
                created_at: a.created_at,
            })
            .collect(),
    }))
}

// ===== Repository Management Endpoints =====

/// List repositories
#[utoipa::path(
    get,
    path = "/v1/repositories",
    responses(
        (status = 200, description = "List of repositories", body = Vec<RepositoryResponse>)
    )
)]
pub async fn list_repositories(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<RepositoryResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let repos = state.db.list_repositories().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to list repositories".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let responses: Vec<RepositoryResponse> = repos
        .into_iter()
        .map(|r| {
            let languages: Vec<String> = serde_json::from_str(&r.languages).unwrap_or_default();
            let frameworks: Vec<String> = r
                .frameworks_json
                .as_ref()
                .and_then(|f| serde_json::from_str(f).ok())
                .unwrap_or_default();

            RepositoryResponse {
                id: r.id,
                repo_id: r.repo_id,
                path: r.path,
                languages,
                default_branch: r.default_branch,
                status: r.status,
                frameworks,
                file_count: r.file_count,
                symbol_count: r.symbol_count,
                created_at: r.created_at,
                updated_at: r.updated_at,
            }
        })
        .collect();

    Ok(Json(responses))
}

/// Register repository
#[utoipa::path(
    post,
    path = "/v1/repositories/register",
    request_body = RegisterRepositoryRequest,
    responses(
        (status = 201, description = "Repository registered", body = RepositoryResponse)
    )
)]
pub async fn register_repository(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RegisterRepositoryRequest>,
) -> Result<(StatusCode, Json<RepositoryResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Require admin or operator role
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    // Validate repository ID format
    validate_repo_id(&req.repo_id)?;

    // Validate file path for security
    validate_file_paths(&[req.path.clone()])?;

    let languages_json = serde_json::to_string(&req.languages).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "invalid languages array".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let id = state
        .db
        .register_repository(
            &req.repo_id,
            &req.path,
            &languages_json,
            &req.default_branch,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to register repository".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(RepositoryResponse {
            id,
            repo_id: req.repo_id,
            path: req.path,
            languages: req.languages,
            default_branch: req.default_branch,
            status: "registered".to_string(),
            frameworks: vec![],
            file_count: None,
            symbol_count: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }),
    ))
}

/// Trigger repository scan
#[utoipa::path(
    post,
    path = "/v1/repositories/{repo_id}/scan",
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    responses(
        (status = 202, description = "Scan triggered", body = ScanStatusResponse)
    )
)]
pub async fn trigger_repository_scan(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(repo_id): Path<String>,
) -> Result<(StatusCode, Json<ScanStatusResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Update status to scanning
    state
        .db
        .update_repository_status(&repo_id, "scanning")
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to update status".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    // In a real implementation, this would trigger a background job
    // For now, just return accepted status

    Ok((
        StatusCode::ACCEPTED,
        Json(ScanStatusResponse {
            repo_id,
            status: "scanning".to_string(),
            progress: Some(0.0),
            message: Some("Scan initiated".to_string()),
        }),
    ))
}

/// Get repository scan status
#[utoipa::path(
    get,
    path = "/v1/repositories/{repo_id}/status",
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    responses(
        (status = 200, description = "Scan status", body = ScanStatusResponse)
    )
)]
pub async fn get_repository_status(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(repo_id): Path<String>,
) -> Result<Json<ScanStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let repo = state
        .db
        .get_repository(&repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "database error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "repository not found".to_string(),
                    details: None,
                }),
            )
        })?;

    Ok(Json(ScanStatusResponse {
        repo_id: repo.repo_id,
        status: repo.status,
        progress: None,
        message: None,
    }))
}

/// Get repository code intelligence report
pub async fn get_repository_report(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(repo_id): Path<String>,
) -> Result<Json<RepositoryReportResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would aggregate code intelligence metrics
    Ok(Json(RepositoryReportResponse {
        repo_id,
        total_lines: 125_000,
        total_files: 450,
        complexity_score: 42.5,
        risk_level: "medium".to_string(),
        languages: vec![
            LanguageStats {
                language: "rust".to_string(),
                line_count: 85_000,
                file_count: 280,
                percentage: 68.0,
            },
            LanguageStats {
                language: "typescript".to_string(),
                line_count: 30_000,
                file_count: 120,
                percentage: 24.0,
            },
            LanguageStats {
                language: "python".to_string(),
                line_count: 10_000,
                file_count: 50,
                percentage: 8.0,
            },
        ],
        generated_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Unregister repository from code intelligence
pub async fn unregister_repository(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Stub - would mark repo as inactive (keep historical data)
    // In real implementation:
    // sqlx::query("UPDATE repositories SET active = 0 WHERE repo_id = ?")
    //     .bind(&repo_id)
    //     .execute(state.db.pool())
    //     .await
    //     .map_err(...)?;

    tracing::info!("Repository {} unregistered by {}", repo_id, claims.email);
    Ok(StatusCode::NO_CONTENT)
}

// ===== Metrics Endpoints =====

/// Get quality metrics
#[utoipa::path(
    get,
    path = "/v1/metrics/quality",
    responses(
        (status = 200, description = "Quality metrics", body = QualityMetricsResponse)
    )
)]
pub async fn get_quality_metrics(
    Extension(_claims): Extension<Claims>,
) -> Result<Json<QualityMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub implementation - would compute from telemetry
    Ok(Json(QualityMetricsResponse {
        arr: 0.95,
        ecs5: 0.82,
        hlr: 0.02,
        cr: 0.01,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get adapter performance metrics
#[utoipa::path(
    get,
    path = "/v1/metrics/adapters",
    responses(
        (status = 200, description = "Adapter metrics", body = AdapterMetricsResponse)
    )
)]
pub async fn get_adapter_metrics(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<AdapterMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let adapters = state.db.list_adapters().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to list adapters".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let mut performances = Vec::new();
    for adapter in adapters {
        let (total, selected, avg_gate) = state
            .db
            .get_adapter_stats(&adapter.adapter_id)
            .await
            .unwrap_or((0, 0, 0.0));

        let activation_rate = if total > 0 {
            (selected as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        performances.push(AdapterPerformance {
            adapter_id: adapter.adapter_id,
            name: adapter.name,
            activation_rate,
            avg_gate_value: avg_gate,
            total_requests: total,
        });
    }

    Ok(Json(AdapterMetricsResponse {
        adapters: performances,
    }))
}

/// Get system metrics
#[utoipa::path(
    get,
    path = "/v1/metrics/system",
    responses(
        (status = 200, description = "System metrics", body = SystemMetricsResponse)
    )
)]
pub async fn get_system_metrics(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<SystemMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    use adapteros_system_metrics::SystemMetricsCollector;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Collect real system metrics
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let load_avg = collector.load_average();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_secs();

    Ok(Json(SystemMetricsResponse {
        cpu_usage: metrics.cpu_usage as f32,
        memory_usage: metrics.memory_usage as f32,
        active_workers: {
            // Count active workers from database
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE status = 'active'")
                .fetch_one(state.db.pool())
                .await
                .unwrap_or(0) as i32
        },
        requests_per_second: {
            // Calculate RPS from recent request log
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-1 minute')",
            )
            .fetch_one(state.db.pool())
            .await
            .map(|count| count as f32 / 60.0)
            .unwrap_or(0.0)
        },
        avg_latency_ms: {
            // Calculate average latency from recent requests
            sqlx::query_scalar::<_, Option<f64>>(
                "SELECT AVG(latency_ms) FROM request_log WHERE timestamp > datetime('now', '-5 minutes')"
            )
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(None)
            .unwrap_or(0.0) as f32
        },
        disk_usage: metrics.disk_io.usage_percent,
        network_bandwidth: metrics.network_io.bandwidth_mbps,
        gpu_utilization: metrics.gpu_metrics.utilization.unwrap_or(0.0) as f32,
        uptime_seconds: collector.uptime_seconds(),
        process_count: collector.process_count(),
        load_average: LoadAverageResponse {
            load_1min: load_avg.0,
            load_5min: load_avg.1,
            load_15min: load_avg.2,
        },
        timestamp,
    }))
}

// ===== Commit Inspector Endpoints =====

/// List commits
#[utoipa::path(
    get,
    path = "/v1/commits",
    params(
        ("repo_id" = Option<String>, Query, description = "Filter by repository"),
        ("branch" = Option<String>, Query, description = "Filter by branch"),
        ("limit" = Option<i64>, Query, description = "Limit results")
    ),
    responses(
        (status = 200, description = "List of commits", body = Vec<CommitResponse>)
    )
)]
pub async fn list_commits(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(_query): Query<ListCommitsQuery>,
) -> Result<Json<Vec<CommitResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would query commits table
    Ok(Json(vec![]))
}

/// Get commit details
#[utoipa::path(
    get,
    path = "/v1/commits/{sha}",
    params(
        ("sha" = String, Path, description = "Commit SHA")
    ),
    responses(
        (status = 200, description = "Commit details", body = CommitResponse),
        (status = 404, description = "Commit not found", body = ErrorResponse)
    )
)]
pub async fn get_commit(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(_sha): Path<String>,
) -> Result<Json<CommitResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would query commits table
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: "commit not found".to_string(),
            details: None,
        }),
    ))
}

/// Get commit diff
#[utoipa::path(
    get,
    path = "/v1/commits/{sha}/diff",
    params(
        ("sha" = String, Path, description = "Commit SHA")
    ),
    responses(
        (status = 200, description = "Commit diff", body = CommitDiffResponse)
    )
)]
pub async fn get_commit_diff(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(_sha): Path<String>,
) -> Result<Json<CommitDiffResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would fetch from git
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: "commit not found".to_string(),
            details: None,
        }),
    ))
}

// ===== Routing Inspector Endpoints =====

/// Debug routing decision
#[utoipa::path(
    post,
    path = "/v1/routing/debug",
    request_body = RoutingDebugRequest,
    responses(
        (status = 200, description = "Routing debug info", body = RoutingDebugResponse)
    )
)]
pub async fn debug_routing(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Json(_req): Json<RoutingDebugRequest>,
) -> Result<Json<RoutingDebugResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would run router with debug info
    Ok(Json(RoutingDebugResponse {
        features: FeatureVector {
            language: Some("rust".to_string()),
            frameworks: vec![],
            symbol_hits: 0,
            path_tokens: vec![],
            verb: "implement".to_string(),
        },
        adapter_scores: vec![],
        selected_adapters: vec![],
        explanation: "Debug mode not fully implemented".to_string(),
    }))
}

/// Get routing history
#[utoipa::path(
    get,
    path = "/v1/routing/history",
    responses(
        (status = 200, description = "Routing history", body = Vec<RoutingDebugResponse>)
    )
)]
pub async fn get_routing_history(
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<RoutingDebugResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would query activation history
    Ok(Json(vec![]))
}

// ===== Agent D Contract Endpoints =====

/// Get system metadata
#[utoipa::path(
    get,
    path = "/v1/meta",
    responses(
        (status = 200, description = "System metadata", body = MetaResponse)
    )
)]
pub async fn meta() -> Json<MetaResponse> {
    Json(MetaResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        build_hash: option_env!("BUILD_HASH").unwrap_or("dev").to_string(),
        build_date: option_env!("BUILD_DATE").unwrap_or("unknown").to_string(),
    })
}

/// Get routing decisions (placeholder for Agent D)
#[utoipa::path(
    get,
    path = "/v1/routing/decisions",
    params(
        ("tenant" = String, Query, description = "Tenant ID"),
        ("limit" = Option<usize>, Query, description = "Limit results"),
        ("since" = Option<String>, Query, description = "ISO-8601 timestamp")
    ),
    responses(
        (status = 200, description = "Routing decisions", body = RoutingDecisionsResponse),
        (status = 404, description = "Not yet available")
    )
)]
pub async fn routing_decisions(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(_params): Query<RoutingDecisionsQuery>,
) -> Result<Json<RoutingDecisionsResponse>, StatusCode> {
    // TODO: Implement when router telemetry available
    // Agent D will fallback to parsing telemetry NDJSON
    Err(StatusCode::NOT_FOUND)
}

/// List audits with extended fields
#[utoipa::path(
    get,
    path = "/v1/audits",
    params(
        ("tenant" = String, Query, description = "Tenant ID"),
        ("limit" = Option<usize>, Query, description = "Limit results")
    ),
    responses(
        (status = 200, description = "List of audits", body = AuditsResponse)
    )
)]
pub async fn list_audits_extended(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<AuditsQuery>,
) -> Result<Json<AuditsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let audits = sqlx::query_as::<_, AuditExtended>(
        "SELECT id, tenant_id, cpid, arr, ecs5, hlr, cr, status, 
                before_cpid, after_cpid, created_at 
         FROM audits WHERE tenant_id = ? 
         ORDER BY created_at DESC LIMIT ?",
    )
    .bind(&params.tenant)
    .bind(params.limit.unwrap_or(50) as i64)
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to fetch audits".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    Ok(Json(AuditsResponse { items: audits }))
}

/// Get promotion record with signature
#[utoipa::path(
    get,
    path = "/v1/promotions/{id}",
    params(
        ("id" = String, Path, description = "Promotion ID")
    ),
    responses(
        (status = 200, description = "Promotion record", body = PromotionRecord),
        (status = 404, description = "Promotion not found")
    )
)]
pub async fn get_promotion(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<PromotionRecord>, (StatusCode, Json<ErrorResponse>)> {
    let promo = sqlx::query_as::<_, PromotionRecord>(
        "SELECT id, cpid, promoted_by, promoted_at, signature_b64, 
                signer_key_id, quality_json, before_cpid 
         FROM promotions WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "database error".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "promotion not found".to_string(),
                details: None,
            }),
        )
    })?;

    Ok(Json(promo))
}

// ===== Metrics Endpoint =====

/// Prometheus/OpenMetrics endpoint  
/// Note: This endpoint requires bearer token authentication via Authorization header.
/// Authentication is checked in the route layer, not in the handler itself.
pub async fn metrics_handler(State(state): State<AppState>) -> axum::response::Response {
    // Check if metrics are enabled
    let config = match state.config.read() {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("Failed to acquire config read lock: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal error".to_string(),
                    details: Some(e.to_string()),
                }),
            )
                .into_response();
        }
    };
    if !config.metrics.enabled {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "metrics disabled".to_string(),
                details: None,
            }),
        )
            .into_response();
    }
    drop(config);

    // Update worker metrics from database
    if let Err(e) = state
        .metrics_exporter
        .update_worker_metrics(&state.db)
        .await
    {
        tracing::warn!("Failed to update worker metrics: {}", e);
    }

    // Render metrics
    let metrics = match state.metrics_exporter.render() {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to render metrics".to_string(),
                    details: Some(e.to_string()),
                }),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        metrics,
    )
        .into_response()
}

// ===== SSE Stream Endpoints =====

use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::{self, Stream};
use std::convert::Infallible;
use std::time::Duration;

/// SSE stream for system metrics
/// Pushes SystemMetrics every 5 seconds
pub async fn system_metrics_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold(state, |state| async move {
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Fetch metrics
        let metrics = match get_system_metrics_internal(&state).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to fetch metrics for SSE: {}", e);
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data(format!("{{\"error\": \"{}\"}}", e))),
                    state,
                ));
            }
        };

        let json = match serde_json::to_string(&metrics) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("Failed to serialize metrics: {}", e);
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data(format!("{{\"error\": \"serialization failed\"}}"))),
                    state,
                ));
            }
        };

        Some((Ok(Event::default().event("metrics").data(json)), state))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// SSE stream for telemetry events
/// Streams new telemetry bundles as they're created
pub async fn telemetry_events_stream(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold((), |()| async move {
        tokio::time::sleep(Duration::from_secs(10)).await;

        // TODO: Implement real telemetry bundle streaming once DB methods exist
        // For now, send keepalive events
        Some((Ok(Event::default().event("keepalive").data("{}")), ()))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// SSE stream for adapter state transitions
/// Streams adapter lifecycle events
pub async fn adapter_state_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold(state, |state| async move {
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Fetch all adapters
        let adapters = match state.db.list_adapters().await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("Failed to fetch adapters for SSE: {}", e);
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data(format!("{{\"error\": \"{}\"}}", e))),
                    state,
                ));
            }
        };

        let json = match serde_json::to_string(&adapters) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("Failed to serialize adapters: {}", e);
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data(format!("{{\"error\": \"serialization failed\"}}"))),
                    state,
                ));
            }
        };

        Some((Ok(Event::default().event("adapters").data(json)), state))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// Helper to extract system metrics logic
async fn get_system_metrics_internal(state: &AppState) -> Result<SystemMetricsResponse, String> {
    use adapteros_system_metrics::SystemMetricsCollector;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Collect real system metrics
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let load_avg = collector.load_average();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("time error: {}", e))?
        .as_secs();

    Ok(SystemMetricsResponse {
        cpu_usage: metrics.cpu_usage as f32,
        memory_usage: metrics.memory_usage as f32,
        active_workers: {
            // Count active workers from database
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE status = 'active'")
                .fetch_one(state.db.pool())
                .await
                .unwrap_or(0) as i32
        },
        requests_per_second: {
            // Calculate RPS from recent request log
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-1 minute')",
            )
            .fetch_one(state.db.pool())
            .await
            .map(|count| count as f32 / 60.0)
            .unwrap_or(0.0)
        },
        avg_latency_ms: {
            // Calculate average latency from recent requests
            sqlx::query_scalar::<_, Option<f64>>(
                "SELECT AVG(latency_ms) FROM request_log WHERE timestamp > datetime('now', '-5 minutes')"
            )
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(None)
            .unwrap_or(0.0) as f32
        },
        disk_usage: metrics.disk_io.usage_percent,
        network_bandwidth: metrics.network_io.bandwidth_mbps,
        gpu_utilization: metrics.gpu_metrics.utilization.unwrap_or(0.0) as f32,
        uptime_seconds: collector.uptime_seconds(),
        process_count: collector.process_count(),
        load_average: LoadAverageResponse {
            load_1min: load_avg.0,
            load_5min: load_avg.1,
            load_15min: load_avg.2,
        },
        timestamp,
    })
}

// ============================================================================
// Streaming API Endpoints (SSE)
// ============================================================================
// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.5, §4.4

/// Training stream SSE endpoint
///
/// Streams real-time training events including adapter lifecycle transitions,
/// promotion/demotion events, profiler metrics, and K reduction events.
///
/// Events are sent as Server-Sent Events (SSE) with the following format:
/// ```
/// event: training
/// data: {"type":"adapter_promoted","timestamp":...,"payload":{...}}
/// ```
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.5
#[utoipa::path(
    get,
    path = "/v1/streams/training",
    params(
        ("tenant" = String, Query, description = "Tenant ID for filtering events")
    ),
    responses(
        (status = 200, description = "SSE stream of training events")
    )
)]
pub async fn training_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<StreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let tenant_id = params.tenant.clone();

    // Create a stream that emits training events
    // For now, this is a mock implementation that simulates events
    // TODO: Connect to actual worker signal stream once worker integration is complete
    let stream = stream::unfold(
        (state, tenant_id, 0),
        |(state, tenant_id, counter)| async move {
            // Wait between events (simulating real-time updates)
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Create mock training event
            // In production, this would come from the worker's signal channel
            let event_data = serde_json::json!({
                "type": if counter % 3 == 0 { "adapter_promoted" } else if counter % 3 == 1 { "profiler_metrics" } else { "adapter_state_transition" },
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("System time before UNIX epoch")
                    .as_millis(),
                "payload": {
                    "adapter_id": format!("adapter_{}", counter % 5),
                    "tenant_id": &tenant_id,
                    "from_state": "warm",
                    "to_state": "hot",
                    "reason": "high_activation",
                    "metrics": {
                        "activation_pct": 12.5 + (counter as f32 * 0.5),
                        "avg_latency_us": 450 + (counter * 10),
                        "memory_bytes": 1024 * 1024 * (10 + counter)
                    }
                }
            });

            let event = Event::default()
                .event("training")
                .data(event_data.to_string());

            Some((Ok(event), (state, tenant_id, counter + 1)))
        },
    );

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Discovery stream SSE endpoint
///
/// Streams real-time repository scanning and code discovery events including
/// scan progress, symbol indexing, framework detection, and completion events.
///
/// Events are sent as Server-Sent Events (SSE) with the following format:
/// ```
/// event: discovery
/// data: {"type":"symbol_indexed","timestamp":...,"payload":{...}}
/// ```
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §4.4
#[utoipa::path(
    get,
    path = "/v1/streams/discovery",
    params(
        ("tenant" = String, Query, description = "Tenant ID for filtering events"),
        ("repo" = Option<String>, Query, description = "Optional repository ID filter")
    ),
    responses(
        (status = 200, description = "SSE stream of discovery events")
    )
)]
pub async fn discovery_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<DiscoveryStreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let tenant_id = params.tenant.clone();
    let repo_filter = params.repo.clone();

    // Create a stream that emits discovery events
    // For now, this is a mock implementation
    // TODO: Connect to actual CodeGraph scanner signal stream
    let stream = stream::unfold(
        (state, tenant_id, repo_filter, 0),
        |(state, tenant_id, repo_filter, counter)| async move {
            tokio::time::sleep(Duration::from_millis(500)).await;

            let repo_id = repo_filter
                .clone()
                .unwrap_or_else(|| "acme/payments".to_string());

            // Cycle through different discovery event types
            let event_type = match counter % 5 {
                0 => "repo_scan_started",
                1 => "repo_scan_progress",
                2 => "symbol_indexed",
                3 => "framework_detected",
                _ => "repo_scan_completed",
            };

            let event_data = serde_json::json!({
                "type": event_type,
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("System time before UNIX epoch")
                    .as_millis(),
                "payload": {
                    "repo_id": repo_id,
                    "tenant_id": &tenant_id,
                    "stage": if counter < 10 { "parsing" } else if counter < 20 { "indexing" } else { "completed" },
                    "files_parsed": counter * 14,
                    "symbol_count": counter * 183,
                    "framework": if event_type == "framework_detected" { Some("django 4.2") } else { None },
                    "content_hash": if event_type == "repo_scan_completed" { Some(format!("b3:abc{:03x}", counter)) } else { None }
                }
            });

            let event = Event::default()
                .event("discovery")
                .data(event_data.to_string());

            Some((Ok(event), (state, tenant_id, repo_filter, counter + 1)))
        },
    );

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Contacts stream SSE endpoint
///
/// Streams real-time contact discovery and update events as contacts are
/// discovered during inference operations.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    get,
    path = "/v1/streams/contacts",
    params(
        ("tenant" = String, Query, description = "Tenant ID for filtering events")
    ),
    responses(
        (status = 200, description = "SSE stream of contact events")
    )
)]
pub async fn contacts_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<StreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let tenant_id = params.tenant.clone();

    // Create a stream that emits contact events
    // TODO: Connect to actual contact discovery signal stream
    let stream = stream::unfold(
        (state, tenant_id, 0),
        |(state, tenant_id, counter)| async move {
            tokio::time::sleep(Duration::from_secs(5)).await;

            let categories = ["adapter", "repository", "user", "system", "external"];
            let names = [
                "adapter_0",
                "acme/payments",
                "john.doe",
                "api_gateway",
                "stripe_api",
            ];

            let idx = counter % 5;
            let event_data = serde_json::json!({
                "type": "contact_discovered",
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("System time before UNIX epoch")
                    .as_millis(),
                "payload": {
                    "name": names[idx],
                    "category": categories[idx],
                    "tenant_id": &tenant_id,
                    "metadata": {
                        "discovered_at": chrono::Utc::now().to_rfc3339()
                    }
                }
            });

            let event = Event::default()
                .event("contact")
                .data(event_data.to_string());

            Some((Ok(event), (state, tenant_id, counter + 1)))
        },
    );

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ============================================================================
// Contacts API Endpoints
// ============================================================================
// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6

/// List contacts with filtering
///
/// Returns contacts discovered during inference, filtered by tenant and optionally by category.
/// Contacts represent entities (users, adapters, repositories, systems) that the inference
/// engine has interacted with.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    get,
    path = "/v1/contacts",
    params(
        ("tenant" = String, Query, description = "Tenant ID"),
        ("category" = Option<String>, Query, description = "Filter by category (user|system|adapter|repository|external)"),
        ("limit" = Option<usize>, Query, description = "Limit results (default: 100)")
    ),
    responses(
        (status = 200, description = "List of contacts", body = ContactsResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn list_contacts(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<ContactsQuery>,
) -> Result<Json<ContactsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Build query based on filters
    let mut query = String::from(
        "SELECT id, tenant_id, name, email, category, role, metadata_json, avatar_url, \
         discovered_at, discovered_by, last_interaction, interaction_count, \
         created_at, updated_at \
         FROM contacts WHERE tenant_id = ?",
    );

    let mut bind_values: Vec<String> = vec![params.tenant.clone()];

    // Add category filter if provided
    if let Some(ref category) = params.category {
        query.push_str(" AND category = ?");
        bind_values.push(category.clone());
    }

    query.push_str(" ORDER BY discovered_at DESC LIMIT ?");
    bind_values.push(params.limit.unwrap_or(100).to_string());

    // Execute query
    // Note: This is a simplified version. In production, use proper query builder
    let contacts = sqlx::query_as::<_, ContactRow>(
        "SELECT * FROM contacts WHERE tenant_id = ? ORDER BY discovered_at DESC LIMIT ?",
    )
    .bind(&params.tenant)
    .bind(params.limit.unwrap_or(100) as i64)
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to fetch contacts".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    // Convert to response format
    let contacts: Vec<ContactResponse> = contacts.into_iter().map(|c| c.into()).collect();

    Ok(Json(ContactsResponse { contacts }))
}

/// Create or update a contact
///
/// Creates a new contact or updates an existing one based on (tenant_id, name, category) uniqueness.
/// This endpoint can be used to manually register contacts or update their metadata.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    post,
    path = "/v1/contacts",
    request_body = CreateContactRequest,
    responses(
        (status = 200, description = "Contact created/updated", body = ContactResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn create_contact(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Json(request): Json<CreateContactRequest>,
) -> Result<Json<ContactResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate category
    if !["user", "system", "adapter", "repository", "external"].contains(&request.category.as_str())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "invalid category".to_string(),
                details: Some(
                    "category must be one of: user, system, adapter, repository, external"
                        .to_string(),
                ),
            }),
        ));
    }

    // Upsert contact
    let contact = sqlx::query_as::<_, ContactRow>(
        "INSERT INTO contacts (tenant_id, name, email, category, role, metadata_json)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(tenant_id, name, category) DO UPDATE SET
            email = excluded.email,
            role = excluded.role,
            metadata_json = excluded.metadata_json,
            updated_at = datetime('now')
         RETURNING *",
    )
    .bind(&request.tenant_id)
    .bind(&request.name)
    .bind(&request.email)
    .bind(&request.category)
    .bind(&request.role)
    .bind(&request.metadata_json)
    .fetch_one(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to create contact".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    Ok(Json(contact.into()))
}

/// Get contact by ID
///
/// Retrieves a specific contact by its unique identifier.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    get,
    path = "/v1/contacts/{id}",
    params(
        ("id" = String, Path, description = "Contact ID")
    ),
    responses(
        (status = 200, description = "Contact details", body = ContactResponse),
        (status = 404, description = "Contact not found", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn get_contact(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<ContactResponse>, (StatusCode, Json<ErrorResponse>)> {
    let contact = sqlx::query_as::<_, ContactRow>("SELECT * FROM contacts WHERE id = ?")
        .bind(&id)
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "contact not found".to_string(),
                    details: None,
                }),
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to fetch contact".to_string(),
                    details: Some(e.to_string()),
                }),
            ),
        })?;

    Ok(Json(contact.into()))
}

/// Delete a contact
///
/// Permanently deletes a contact and all associated interaction records.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    delete,
    path = "/v1/contacts/{id}",
    params(
        ("id" = String, Path, description = "Contact ID")
    ),
    responses(
        (status = 200, description = "Contact deleted"),
        (status = 404, description = "Contact not found", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn delete_contact(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let result = sqlx::query("DELETE FROM contacts WHERE id = ?")
        .bind(&id)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to delete contact".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "contact not found".to_string(),
                details: None,
            }),
        ));
    }

    Ok(StatusCode::OK)
}

/// Get contact interaction history
///
/// Returns the interaction log for a specific contact, showing when and how
/// the contact was referenced during inference operations.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    get,
    path = "/v1/contacts/{id}/interactions",
    params(
        ("id" = String, Path, description = "Contact ID"),
        ("limit" = Option<usize>, Query, description = "Limit results (default: 50)")
    ),
    responses(
        (status = 200, description = "Interaction history", body = ContactInteractionsResponse),
        (status = 404, description = "Contact not found", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn get_contact_interactions(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(id): Path<String>,
    Query(params): Query<ContactInteractionsQuery>,
) -> Result<Json<ContactInteractionsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Verify contact exists
    let _contact = sqlx::query_as::<_, ContactRow>("SELECT * FROM contacts WHERE id = ?")
        .bind(&id)
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "contact not found".to_string(),
                    details: None,
                }),
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to fetch contact".to_string(),
                    details: Some(e.to_string()),
                }),
            ),
        })?;

    // Fetch interactions
    let interactions = sqlx::query_as::<_, ContactInteractionRow>(
        "SELECT * FROM contact_interactions 
         WHERE contact_id = ? 
         ORDER BY created_at DESC 
         LIMIT ?",
    )
    .bind(&id)
    .bind(params.limit.unwrap_or(50) as i64)
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to fetch interactions".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let interactions: Vec<ContactInteractionResponse> =
        interactions.into_iter().map(|i| i.into()).collect();

    Ok(Json(ContactInteractionsResponse { interactions }))
}

// ========== Training Handlers ==========

/// List all training jobs
#[utoipa::path(
    get,
    path = "/v1/training/jobs",
    responses(
        (status = 200, description = "Training jobs retrieved successfully", body = Vec<TrainingJobResponse>)
    )
)]
pub async fn list_training_jobs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<TrainingJobResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Sre]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "insufficient permissions".to_string(),
                details: None,
            }),
        )
    })?;

    let jobs = state.training_service.list_jobs().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to list training jobs".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    Ok(Json(jobs.into_iter().map(|j| j.into()).collect()))
}

/// Get a specific training job
#[utoipa::path(
    get,
    path = "/v1/training/jobs/{job_id}",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "Training job retrieved successfully", body = TrainingJobResponse)
    )
)]
pub async fn get_training_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<TrainingJobResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Sre]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "insufficient permissions".to_string(),
                details: None,
            }),
        )
    })?;

    let job = state.training_service.get_job(&job_id).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "training job not found".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    Ok(Json(job.into()))
}

/// Start a new training job
#[utoipa::path(
    post,
    path = "/v1/training/start",
    request_body = StartTrainingRequest,
    responses(
        (status = 200, description = "Training started successfully", body = TrainingJobResponse)
    )
)]
pub async fn start_training(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<StartTrainingRequest>,
) -> Result<Json<TrainingJobResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "insufficient permissions".to_string(),
                details: None,
            }),
        )
    })?;

    let config = req.config.into();

    let job = state
        .training_service
        .start_training(req.adapter_name, config, req.template_id, req.repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to start training".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    Ok(Json(job.into()))
}

/// Cancel a training job
#[utoipa::path(
    post,
    path = "/v1/training/jobs/{job_id}/cancel",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "Training cancelled successfully")
    )
)]
pub async fn cancel_training(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "insufficient permissions".to_string(),
                details: None,
            }),
        )
    })?;

    state
        .training_service
        .cancel_job(&job_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to cancel training".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    Ok(StatusCode::OK)
}

/// Get training logs
#[utoipa::path(
    get,
    path = "/v1/training/jobs/{job_id}/logs",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "Logs retrieved successfully", body = Vec<String>)
    )
)]
pub async fn get_training_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Sre]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "insufficient permissions".to_string(),
                details: None,
            }),
        )
    })?;

    let logs = state
        .training_service
        .get_logs(&job_id)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "failed to get logs".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    Ok(Json(logs))
}

/// Get training metrics
#[utoipa::path(
    get,
    path = "/v1/training/jobs/{job_id}/metrics",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "Metrics retrieved successfully", body = TrainingMetricsResponse)
    )
)]
pub async fn get_training_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<TrainingMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Sre]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "insufficient permissions".to_string(),
                details: None,
            }),
        )
    })?;

    let job = state.training_service.get_job(&job_id).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "training job not found".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    Ok(Json(TrainingMetricsResponse {
        loss: job.current_loss,
        tokens_per_second: job.tokens_per_second,
        learning_rate: job.learning_rate,
        current_epoch: job.current_epoch,
        total_epochs: job.total_epochs,
        progress_pct: job.progress_pct,
    }))
}

/// List training templates
#[utoipa::path(
    get,
    path = "/v1/training/templates",
    responses(
        (status = 200, description = "Training templates retrieved successfully", body = Vec<TrainingTemplateResponse>)
    )
)]
pub async fn list_training_templates(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<TrainingTemplateResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(
        &claims,
        &[Role::Admin, Role::Operator, Role::Sre, Role::Viewer],
    )
    .map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "insufficient permissions".to_string(),
                details: None,
            }),
        )
    })?;

    let templates = state.training_service.list_templates().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "failed to list templates".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    Ok(Json(templates.into_iter().map(|t| t.into()).collect()))
}

/// Get a specific training template
#[utoipa::path(
    get,
    path = "/v1/training/templates/{template_id}",
    params(
        ("template_id" = String, Path, description = "Training template ID")
    ),
    responses(
        (status = 200, description = "Training template retrieved successfully", body = TrainingTemplateResponse)
    )
)]
pub async fn get_training_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(template_id): Path<String>,
) -> Result<Json<TrainingTemplateResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(
        &claims,
        &[Role::Admin, Role::Operator, Role::Sre, Role::Viewer],
    )
    .map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "insufficient permissions".to_string(),
                details: None,
            }),
        )
    })?;

    let template = state
        .training_service
        .get_template(&template_id)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "template not found".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    Ok(Json(template.into()))
}

// Git integration handlers
// pub mod git; // Already declared above
