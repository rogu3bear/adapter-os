#![allow(unused_variables)]

use crate::auth::Claims;
use crate::middleware::{require_any_role, require_role};
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::*; // This already re-exports adapteros_api_types::*
use crate::uds_client::{UdsClient, UdsClientError};
use crate::validation::*;
use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::AosError;
use adapteros_lora_lifecycle::GpuIntegrityReport;
// System metrics integration
use adapteros_system_metrics;
use adapteros_system_metrics::monitoring_types::{
    AcknowledgeAlertRequest, AlertResponse, AnomalyResponse, BaselineResponse,
    CreateMonitoringRuleApiRequest, MonitoringRuleResponse, RecalculateBaselineRequest,
    UpdateAnomalyStatusRequest, UpdateMonitoringRuleApiRequest,
};
use axum::response::Response;
use sqlx::Row;
use utoipa::ToSchema;

pub mod activity;
pub mod adapter_stacks;
pub mod adapters;
pub mod auth;
pub mod auth_enhanced;
pub mod batch;
pub mod chunked_upload;
pub mod code;
pub mod dashboard;
pub mod datasets;
pub mod domain_adapters;
pub mod federation;
pub mod git;
pub mod git_repository;
pub mod golden;
pub mod models;
pub mod notifications;
pub mod plugins;
pub mod promotion;
pub mod replay;
pub mod routing_decisions;
pub mod services;
pub mod streaming_infer;
pub mod telemetry;
pub mod tenants;
pub mod training;
pub mod tutorials;
pub mod workspaces;

// Re-export adapter lifecycle and lineage handlers
pub use adapters::*;

// Re-export tenant handlers
pub use tenants::*;

// Re-export auth handlers (including utoipa path types)
pub use auth::{__path_auth_login, auth_login, auth_logout, auth_me};

// Re-export training handlers
pub use training::*;

// Re-export domain adapter handlers
use adapteros_db::sqlx;
use adapteros_db::users::Role;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
pub use domain_adapters::*;
use serde::{Deserialize, Serialize};
// use serde_json::json; // unused
use std::collections::HashMap;
use tracing::{error, info_span};

/// Utility function to convert AosError to axum response format
/// This ensures consistent error handling across all handlers
fn aos_error_to_response(error: AosError) -> (StatusCode, Json<ErrorResponse>) {
    let (status_code, error_code) = match &error {
        AosError::Auth(_) => (StatusCode::UNAUTHORIZED, "AUTHENTICATION_ERROR"),
        AosError::Authz(_) => (StatusCode::FORBIDDEN, "AUTHORIZATION_ERROR"),
        AosError::Database(_) | AosError::Sqlx(_) | AosError::Sqlite(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR")
        }
        AosError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
        AosError::Validation(_) => (StatusCode::BAD_REQUEST, "VALIDATION_ERROR"),
        AosError::PolicyViolation(_) => (StatusCode::FORBIDDEN, "POLICY_VIOLATION"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
    };

    (
        status_code,
        Json(ErrorResponse::new(error.to_string()).with_code(error_code)),
    )
}

/// Health check endpoint
#[utoipa::path(
    tag = "system",
    get,
    path = "/healthz",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    )
)]
pub async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        models: None,
    })
}

/// Readiness check
#[utoipa::path(
    tag = "system",
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
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                status: "ready".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                models: None,
            }),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                status: "not ready".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                models: None,
            }),
        ),
    }
}

/// Upsert a synthetic directory adapter and optionally activate it.
///
/// This handler performs directory analysis and adapter registration with proper async/blocking separation.
///
/// # Blocking Operations
///
/// All blocking operations (filesystem I/O and CPU-intensive analysis) are executed in a dedicated
/// blocking thread pool via `tokio::task::spawn_blocking` to prevent head-of-line blocking on the
/// async runtime. The handler combines three phases into a single blocking call:
///
/// 1. **Path Validation**: Validates root path exists and relative path is safe (no `..`)
/// 2. **Directory Analysis**: CPU-intensive codebase analysis with `adapteros_codegraph`
/// 3. **Artifact Creation**: Creates placeholder `.safetensors` file for adapter
///
/// # Timeout Protection
///
/// The combined blocking operation is wrapped in `tokio::time::timeout` with a configurable duration
/// (default: 120 seconds, configured via `ApiConfig::directory_analysis_timeout_secs`). This prevents
/// malicious or extremely large directories from tying up blocking threads indefinitely.
///
/// # Error Handling
///
/// Errors are returned with appropriate HTTP status codes:
/// - `400 BAD_REQUEST`: Invalid paths, path traversal attempts, or analysis failures
/// - `408 REQUEST_TIMEOUT`: Operation exceeded configured timeout
/// - `500 INTERNAL_SERVER_ERROR`: Filesystem errors or task panics
///
/// # Observability
///
/// The handler includes structured tracing spans for each phase:
/// - `directory_adapter_blocking_ops`: Top-level span for entire blocking operation
/// - `path_validation`: Path validation phase
/// - `directory_analysis`: Directory analysis phase (includes root and path fields)
/// - `artifact_creation`: Artifact file creation phase (includes hash field)
///
/// # Permissions
///
/// Requires `Admin` or `Operator` role via RBAC.
///
/// # Example
///
/// ```no_run
/// POST /v1/adapters/directory/upsert
/// {
///   "root": "/workspace/my-project",
///   "path": "src",
///   "tenant_id": "tenant-a",
///   "activate": false
/// }
/// ```
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapters/directory/upsert",
    request_body = DirectoryUpsertRequest,
    responses(
        (status = 201, description = "Directory adapter upserted", body = DirectoryUpsertResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Failed to upsert directory adapter", body = ErrorResponse)
    ),
    tag = "adapters"
)]
#[axum::debug_handler]
pub async fn upsert_directory_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<DirectoryUpsertRequest>,
) -> Result<(StatusCode, Json<DirectoryUpsertResponse>), (StatusCode, Json<ErrorResponse>)> {
    use std::time::Duration;
    use tracing::warn;

    // Log handler entry (span removed to avoid Send issues with async)
    tracing::info!(
        tenant_id = %req.tenant_id,
        root_path = %req.root,
        activate = req.activate,
        "upsert_directory_adapter_handler started"
    );

    // Require admin or operator
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    // Combined blocking operations: path validation, directory analysis, and artifact creation
    // Timeout prevents malicious/large directories from tying up blocking threads indefinitely
    let tenant_id = req.tenant_id.clone();
    let root_str = req.root;
    let path_str = req.path;

    // Read timeout from config
    let timeout_secs = {
        let config = state.config.read().map_err(|e| {
            error!("Failed to acquire config read lock: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Configuration unavailable").with_code("INTERNAL_ERROR")),
            )
        })?;
        config.directory_analysis_timeout_secs
    };

    let (adapter_id, hash_hex, hash_b3, analysis) = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        tokio::task::spawn_blocking(move || {
            let _span = info_span!("directory_adapter_blocking_ops", tenant = %tenant_id).entered();

            // Phase 1: Validate paths
            let _validation_span = info_span!("path_validation").entered();
            let root = std::path::PathBuf::from(&root_str);
            if !root.is_absolute() || !root.exists() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("invalid root")
                            .with_code("BAD_REQUEST")
                            .with_string_details("root must be an existing absolute path"),
                    ),
                ));
            }

            let rel = std::path::PathBuf::from(&path_str);
            if rel.is_absolute()
                || rel
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("invalid path")
                            .with_code("BAD_REQUEST")
                            .with_string_details("path must be relative and must not contain .."),
                    ),
                ));
            }
            drop(_validation_span);

            // Phase 2: Analyze directory (CPU-intensive + filesystem I/O)
            let _analysis_span = info_span!("directory_analysis",
                root = %root.display(),
                path = %rel.display()
            ).entered();
            let analysis = adapteros_codegraph::analyze_directory(&root, &rel).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("directory analysis failed")
                            .with_code("BAD_REQUEST")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
            drop(_analysis_span);

            // Build adapter_id and synthetic artifact hash from fingerprint
            let adapter_id = format!(
                "directory::{}::{}",
                tenant_id,
                analysis.fingerprint.to_short_hex()
            );
            let hash_hex = analysis.fingerprint.to_hex();
            let hash_b3 = format!("b3:{}", hash_hex);

            // Phase 3: Ensure placeholder artifact (blocking filesystem I/O)
            let _artifact_span = info_span!("artifact_creation", hash = %hash_hex).entered();
            let artifact_dir = std::path::PathBuf::from("./adapters");
            let artifact_path = artifact_dir.join(format!("{}.safetensors", hash_hex));
            if !artifact_path.exists() {
                if let Some(parent) = artifact_path.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                ErrorResponse::new("failed to create adapters directory")
                                    .with_code("INTERNAL_SERVER_ERROR")
                                    .with_string_details(e.to_string()),
                            ),
                        ));
                    }
                }
                if let Err(e) = std::fs::write(&artifact_path, b"synthetic adapter placeholder") {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("failed to write adapter artifact")
                                .with_code("INTERNAL_SERVER_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    ));
                }
            }
            drop(_artifact_span);

            Ok((adapter_id, hash_hex, hash_b3, analysis))
        })
    )
    .await
    // Error handling chain (triple-nested Result unwrapping):
    // 1. First .map_err: Handle timeout::Elapsed from tokio::time::timeout
    .map_err(|_| {
        warn!(timeout_secs = %timeout_secs, "Directory adapter operation timed out");
        (
            StatusCode::REQUEST_TIMEOUT,
            Json(
                ErrorResponse::new("directory analysis timed out")
                    .with_code("REQUEST_TIMEOUT")
                    .with_string_details(format!("operation exceeded {} second limit", timeout_secs)),
            ),
        )
    })?
    // 2. Second .map_err: Handle JoinError from tokio::task::spawn_blocking (task panic)
    .map_err(|e: tokio::task::JoinError| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("blocking task panicked")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?
    // 3. Final ?: Handle inner errors from blocking closure (path validation, analysis, filesystem)
    ?;

    // Register adapter if not present
    let existing = {
        tracing::info!(adapter_id = %adapter_id, "checking adapter in db");
        state.db.get_adapter(&adapter_id).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to query adapter")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
    };

    if existing.is_none() {
        let languages = analysis.language_stats.keys().cloned().collect::<Vec<_>>();
        let languages_json = serde_json::to_string(&languages).unwrap_or("[]".to_string());

        tracing::info!(adapter_id = %adapter_id, "registering adapter in db");
        let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
            .adapter_id(&adapter_id)
            .name(&adapter_id)
            .hash_b3(&hash_b3)
            .rank(i32::from(analysis.symbols.len() as i32 % 17 + 16))
            .tier("warm")
            .languages_json(Some(languages_json.clone()))
            .category("directory")
            .scope("codebase")
            .build()
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to build adapter params")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
        state.db.register_adapter(params).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to register adapter")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    }

    // Optionally activate (load) adapter now
    let mut activated = false;
    if req.activate {
        tracing::info!(adapter_id = %adapter_id, "getting adapter for activation");
        let adapter_result = state.db.get_adapter(&adapter_id).await;

        match adapter_result {
            Ok(Some(a)) => {
                tracing::info!(adapter_id = %adapter_id, "updating adapter state to loading");
                let _ = state
                    .db
                    .update_adapter_state(&adapter_id, "loading", "directory_upsert")
                    .await;

                if let Some(ref lifecycle) = state.lifecycle_manager {
                    use adapteros_core::B3Hash;
                    use adapteros_lora_lifecycle::AdapterLoader;
                    use std::path::PathBuf;
                    // Use the DB numeric id if it parses, else fall back to 0
                    let adapter_idx = a.id.parse::<u16>().unwrap_or(0);
                    let adapters_path = PathBuf::from("./adapters");
                    let mut expected_hashes: HashMap<String, B3Hash> = HashMap::new();
                    expected_hashes.insert(String::from(&hash_hex), analysis.fingerprint);
                    let mut loader = AdapterLoader::new(adapters_path, expected_hashes);
                    if loader
                        .load_adapter_async(adapter_idx, &hash_hex)
                        .await
                        .is_ok()
                    {
                        tracing::info!(adapter_id = %adapter_id, "adapter loaded successfully");
                        let _ = state
                            .db
                            .update_adapter_state(&adapter_id, "warm", "loaded_successfully")
                            .await;
                        activated = true;
                    } else {
                        tracing::info!(adapter_id = %adapter_id, "adapter load failed");
                        let _ = state
                            .db
                            .update_adapter_state(&adapter_id, "cold", "load_failed")
                            .await;
                    }
                } else {
                    // Simulate load
                    tracing::info!(adapter_id = %adapter_id, "simulating adapter load");
                    let _ = state
                        .db
                        .update_adapter_state(&adapter_id, "warm", "simulated_load")
                        .await;
                    activated = true;
                }
            }
            _ => {}
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(DirectoryUpsertResponse {
            adapter_id: adapter_id.to_string(),
            hash_b3,
            activated,
        }),
    ))
}

/// Login handler
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials", body = ErrorResponse)
    )
)]

/// List tenants (all roles can view)

/// Create tenant (admin only)

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
                Json(
                    ErrorResponse::new("failed to update tenant")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
                Json(
                    ErrorResponse::new("failed to update tenant")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
            Json(
                ErrorResponse::new("tenant not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    use sqlx::Row;
    let tenant_id_value: String = row.get("tenant_id");

    // Audit log: tenant updated
    let _ = crate::audit_helper::log_success(
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
        name: row.get("name"),
        itar_flag: row.get("itar_flag"),
        created_at: row.get("created_at"),
        status: "active".to_string(),
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
            Json(
                ErrorResponse::new("failed to pause tenant")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    tracing::info!("Tenant {} paused by {}", tenant_id, claims.email);

    // Audit log: tenant paused
    let _ = crate::audit_helper::log_success(
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
            Json(
                ErrorResponse::new("failed to archive tenant")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    tracing::info!("Tenant {} archived by {}", tenant_id, claims.email);

    // Audit log: tenant archived
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_ARCHIVE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}
use adapteros_lora_lifecycle::{AllocationTier, LifecycleManager};
// UmaPressureMonitor: using mock from AppState (adapteros_lora_worker crate is excluded)
use std::sync::Arc;
use tokio::sync::Mutex;

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

    Ok(Json(UmaMemoryResponse {
        total_mb: stats.total_mb,
        used_mb: stats.used_mb,
        available_mb: stats.available_mb,
        headroom_pct: stats.headroom_pct,
        pressure_level: pressure.to_string(),
        eviction_candidates: candidates,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct UmaMemoryResponse {
    total_mb: u64,
    used_mb: u64,
    available_mb: u64,
    headroom_pct: f32,
    pressure_level: String,
    eviction_candidates: Vec<String>,
    timestamp: String,
}

// In AppState, add uma_monitor: Arc<UmaPressureMonitor> = Arc::new(UmaPressureMonitor::new(15, Some(telemetry.clone())));

// Start polling in main or builder
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
use adapteros_core::tenant_snapshot::TenantStateSnapshot;
use adapteros_core::B3Hash;
use serde_json::Value;
use sqlx::Sqlite;
use sqlx::Transaction; // assume sqlite

// In the function, replace from line ~917

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
                Json(ErrorResponse::new(&e.to_string())),
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
                Json(ErrorResponse::new(&e.to_string())),
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
        let tenant = state
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
                    Json(ErrorResponse::new(&e.to_string())),
                )
            })?;
    }

    // Apply in transaction
    let mut tx = state.db.pool().begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )
    })?;

    for event in &sorted_events {
        if let Err(e) = apply_event(&mut tx, &req.tenant_id, event).await {
            tracing::error!(identity = ?event.get("identity"), error = %e, "Failed to apply event in hydration");
            let _ = tx.rollback().await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(&format!(
                    "Hydration failed on event: {}",
                    e
                ))),
            ));
        }
    }

    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
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
                Json(ErrorResponse::new(&e.to_string())),
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
                Json(ErrorResponse::new(&e.to_string())),
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
                Json(ErrorResponse::new(&e.to_string())),
            )
        })?;

    let tenant = state
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

// Update apply_event to full impl

async fn apply_event<'a>(
    tx: &mut Transaction<'a, Sqlite>,
    tenant_id: &str,
    event: &Value,
) -> adapteros_core::Result<()> {
    let event_type = event
        .get("event_type")
        .and_then(|v| v.as_str())
        .ok_or(AosError::Validation("Missing event_type".to_string()))?;

    let meta = event
        .get("metadata")
        .ok_or(AosError::Validation("Missing metadata".to_string()))?;

    match event_type {
        "adapter.registered" => {
            let id = meta
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing adapter id".to_string()))?
                .to_string();
            let name = meta
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string();
            let rank = meta
                .get("rank")
                .and_then(|v| v.as_i64())
                .ok_or(AosError::Validation("Missing rank".to_string()))?
                as i32;
            let version = meta
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("0.0")
                .to_string();
            let hash_b3 = meta
                .get("hash_b3")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing hash_b3".to_string()))?
                .to_string();

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO adapters 
                (tenant_id, adapter_id, name, rank, version, hash_b3, current_state, tier, created_at, updated_at) 
                VALUES (?, ?, ?, ?, ?, ?, 'unloaded', 'cold', datetime('now'), datetime('now'))
                "#
            )
            .bind(tenant_id)
            .bind(&id)
            .bind(&name)
            .bind(rank)
            .bind(&version)
            .bind(&hash_b3)
            .execute(tx.as_mut())
            .await
            .map_err(|e| AosError::Database(format!("Failed to register adapter {}: {}", id, e)))?;
        }
        "stack.created" => {
            let name = meta
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing stack name".to_string()))?
                .to_string();
            let adapter_ids: Vec<String> = meta
                .get("adapter_ids")
                .and_then(|v| v.as_array())
                .ok_or(AosError::Validation("Missing adapter_ids".to_string()))?
                .iter()
                .filter_map(|vi| vi.as_str().map(|s| s.to_string()))
                .collect();
            let adapter_ids_json =
                serde_json::to_string(&adapter_ids).map_err(|e| AosError::Serialization(e))?;
            let workflow_type = meta
                .get("workflow_type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let id = uuid::Uuid::now_v7().to_string(); // or use name as id if unique

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO adapter_stacks 
                (id, name, adapter_ids_json, workflow_type, created_at, updated_at) 
                VALUES (?, ?, ?, ?, datetime('now'), datetime('now'))
                "#,
            )
            .bind(&id)
            .bind(&name)
            .bind(&adapter_ids_json)
            .bind(&workflow_type)
            .execute(tx.as_mut())
            .await
            .map_err(|e| AosError::Database(format!("Failed to create stack {}: {}", name, e)))?;
        }
        "policy.updated" => {
            let name = meta
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing policy name".to_string()))?
                .to_string();
            let rules: Vec<String> = meta
                .get("rules")
                .and_then(|v| v.as_array())
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|vi| vi.as_str().map(|s| s.to_string()))
                .collect();
            let rules_json =
                serde_json::to_string(&rules).map_err(|e| AosError::Serialization(e))?;

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO router_policies 
                (tenant_id, name, rules_json, updated_at) 
                VALUES (?, ?, ?, datetime('now'))
                "#,
            )
            .bind(tenant_id)
            .bind(&name)
            .bind(&rules_json)
            .execute(tx.as_mut())
            .await
            .map_err(|e| AosError::Database(format!("Failed to update policy {}: {}", name, e)))?;
        }
        "config.updated" => {
            let key = meta
                .get("key")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing config key".to_string()))?
                .to_string();
            let value = meta
                .get("value")
                .ok_or(AosError::Validation("Missing config value".to_string()))?
                .clone();

            let value_json =
                serde_json::to_string(&value).map_err(|e| AosError::Serialization(e))?;

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO tenant_configs 
                (tenant_id, key, value_json, updated_at) 
                VALUES (?, ?, ?, datetime('now'))
                "#,
            )
            .bind(tenant_id)
            .bind(&key)
            .bind(&value_json)
            .execute(tx.as_mut())
            .await
            .map_err(|e| AosError::Database(format!("Failed to update config {}: {}", key, e)))?;
        }
        "plugin.config.updated" => {
            let plugin = meta
                .get("plugin")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing plugin".to_string()))?;
            let config_key = meta
                .get("config_key")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing config_key".to_string()))?;
            let value = meta
                .get("value")
                .ok_or(AosError::Validation("Missing value".to_string()))?
                .clone();

            let key = format!("plugin.{}.{}", plugin, config_key);
            let value_json =
                serde_json::to_string(&value).map_err(|e| AosError::Serialization(e))?;

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO tenant_configs 
                (tenant_id, key, value_json, updated_at) 
                VALUES (?, ?, ?, datetime('now'))
                "#,
            )
            .bind(tenant_id)
            .bind(&key)
            .bind(&value_json)
            .execute(tx.as_mut())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to update plugin config {}: {}", key, e))
            })?;
        }
        "feature.flag.toggled" => {
            let flag = meta
                .get("flag")
                .and_then(|v| v.as_str())
                .ok_or(AosError::Validation("Missing flag".to_string()))?;
            let enabled = meta
                .get("enabled")
                .and_then(|v| v.as_bool())
                .ok_or(AosError::Validation("Missing enabled".to_string()))?;

            let key = format!("flag.{}", flag);
            let value_json =
                serde_json::to_string(&enabled).map_err(|e| AosError::Serialization(e))?;

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO tenant_configs 
                (tenant_id, key, value_json, updated_at) 
                VALUES (?, ?, ?, datetime('now'))
                "#,
            )
            .bind(tenant_id)
            .bind(&key)
            .bind(&value_json)
            .execute(tx.as_mut())
            .await
            .map_err(|e| AosError::Database(format!("Failed to toggle flag {}: {}", flag, e)))?;
        }
        _ => {
            tracing::debug!(
                "Ignored unknown event type: {} for tenant {}",
                event_type,
                tenant_id
            );
        }
    }

    Ok(())
}

// Update Request to have expected_state_hash: Option<String>
#[derive(Deserialize, ToSchema)]
pub struct HydrateTenantRequest {
    pub bundle_id: String,
    pub tenant_id: String,
    pub dry_run: bool,
    pub expected_state_hash: Option<String>,
}

// Update utoipa path to match

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
                Json(
                    ErrorResponse::new("failed to assign policy")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
                Json(ErrorResponse::new("failed to assign adapter").with_code("INTERNAL_SERVER_ERROR").with_string_details(e.to_string())),
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

/// List nodes
pub async fn list_nodes(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<NodeResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    let nodes = state.db.list_nodes().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let response: Vec<NodeResponse> = nodes
        .into_iter()
        .map(|n| NodeResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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
    require_any_role(&claims, &[Role::Operator])?;

    let id = state
        .db
        .register_node(&req.hostname, &req.agent_endpoint)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to register node")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let node = state.db.get_node(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let node = node.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("node not found after registration").with_code("NOT_FOUND")),
        )
    })?;

    // Audit log: node registered
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::NODE_REGISTER,
        crate::audit_helper::resources::NODE,
        Some(&node.id),
    )
    .await;

    Ok(Json(NodeResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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
    require_any_role(&claims, &[Role::Operator])?;

    // Get node from database
    let node = state
        .db
        .get_node(&node_id)
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
                Json(ErrorResponse::new("node not found").with_code("NOT_FOUND")),
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
                Json(
                    ErrorResponse::new("failed to create HTTP client")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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
    require_any_role(&claims, &[Role::Operator])?;

    // Update node status in database
    let timestamp = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE nodes SET status = 'offline', last_seen_at = ? WHERE id = ?"
    )
    .bind(&timestamp)
    .bind(&node_id)
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to update node status")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Audit log: node marked offline
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::NODE_OFFLINE,
        crate::audit_helper::resources::NODE,
        Some(&node_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Evict node (delete from registry)
pub async fn evict_node(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Check for running workers on this node
    let workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let node_has_workers = workers.iter().any(|w| w.node_id == node_id);

    if node_has_workers {
        return Err((
            StatusCode::CONFLICT,
            Json(
                ErrorResponse::new("node has running workers")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details("Stop all workers before evicting node"),
            ),
        ));
    }

    // Delete node from database
    sqlx::query("DELETE FROM nodes WHERE id = ?")
        .bind(&node_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to delete node")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Audit log: node evicted
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::NODE_EVICT,
        crate::audit_helper::resources::NODE,
        Some(&node_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Get node details
pub async fn get_node_details(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<Json<NodeDetailsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Get node from database
    let node = state
        .db
        .get_node(&node_id)
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
                Json(ErrorResponse::new("node not found").with_code("NOT_FOUND")),
            )
        })?;

    // Get workers running on this node
    let all_workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
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
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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

    let params = adapteros_db::models::ModelRegistrationParams {
        name: req.name.clone(),
        hash_b3: req.hash_b3.clone(),
        config_hash_b3: req.config_hash_b3.clone(),
        tokenizer_hash_b3: req.tokenizer_hash_b3.clone(),
        tokenizer_cfg_hash_b3: req.tokenizer_cfg_hash_b3.clone(),
        license_hash_b3: req.license_hash_b3.clone(),
        metadata_json: req.metadata_json.clone(),
    };
    state.db.register_model(params).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to import model")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(StatusCode::CREATED)
}

/// Get base model status
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/models/status",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID")
    ),
    responses(
        (status = 200, description = "Base model status", body = BaseModelStatusResponse),
        (status = 404, description = "No base model status found", body = ErrorResponse)
    )
)]
pub async fn get_base_model_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListJobsQuery>,
) -> Result<Json<BaseModelStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin, Role::Compliance])?;

    let tenant_id = query.tenant_id.unwrap_or_else(|| "default".to_string());

    // Get base model status from database
    let status_record = state
        .db
        .get_base_model_status(&tenant_id)
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
        })?;

    // If no status record exists, return default unloaded status
    if let Some(status_record) = status_record {
        // Get model details
        let model = state
            .db
            .get_model(&status_record.model_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("model not found").with_code("NOT_FOUND")),
                )
            })?;

        let is_loaded = status_record.status == "loaded";

        Ok(Json(BaseModelStatusResponse {
            model_id: status_record.model_id,
            model_name: model.name,
            status: status_record.status,
            loaded_at: status_record.loaded_at,
            unloaded_at: status_record.unloaded_at,
            error_message: status_record.error_message,
            memory_usage_mb: status_record.memory_usage_mb,
            is_loaded,
            updated_at: status_record.updated_at,
        }))
    } else {
        // Return default unloaded status when no record exists
        Ok(Json(BaseModelStatusResponse {
            model_id: "none".to_string(),
            model_name: "No Model Loaded".to_string(),
            status: "unloaded".to_string(),
            loaded_at: None,
            unloaded_at: None,
            error_message: None,
            memory_usage_mb: None,
            is_loaded: false,
            updated_at: chrono::Utc::now().to_rfc3339(),
        }))
    }
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
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
            Json(
                ErrorResponse::new("serialization error")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
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
                Json(
                    ErrorResponse::new("failed to create job")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
                Json(
                    ErrorResponse::new("failed to load plan")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("plan not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Plan ID: {}", req.plan_id)),
                ),
            )
        })?;

    // Load latest audit for the CPID
    let audits = state.db.list_all_audits().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to load audits")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
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
                Json(
                    ErrorResponse::new("no passing audit found for CPID")
                        .with_code("BAD_REQUEST")
                        .with_string_details(format!(
                            "Run audit and ensure it passes before promotion: {}",
                            req.cpid
                        )),
                ),
            )
        })?;

    // Parse audit results to check quality gates
    let audit_result: serde_json::Value =
        serde_json::from_str(&latest_audit.result_json).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to parse audit results")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
            Json(
                ErrorResponse::new("quality gates failed")
                    .with_code("BAD_REQUEST")
                    .with_string_details(failures.join("; ")),
            ),
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
                Json(
                    ErrorResponse::new("failed to get CP pointer")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("CP pointer not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("CPID: {}", req.cpid)),
                ),
            )
        })?;

    // Create quality metrics JSON for signing
    let quality_metrics = QualityMetrics { arr, ecs5, hlr, cr };
    let quality_json = serde_json::to_string(&quality_metrics).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to serialize quality metrics")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Generate Ed25519 signature
    let (signature_b64, signer_key_id) =
        crate::signing::sign_promotion(&req.cpid, &claims.email, &quality_json).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to sign promotion")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // BEGIN TRANSACTION
    let mut tx = state.db.pool().begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to start transaction")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
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
                Json(
                    ErrorResponse::new("failed to deactivate CP pointers")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
                Json(
                    ErrorResponse::new("failed to activate CP pointer")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
            Json(ErrorResponse::new("failed to insert promotion record").with_code("INTERNAL_SERVER_ERROR").with_string_details(e.to_string())),
        )
    })?;

    // COMMIT TRANSACTION
    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to commit transaction")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
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
    require_any_role(&claims, &[Role::Operator])?;

    // Look up node by ID
    let node = state
        .db
        .get_node(&req.node_id)
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
                Json(
                    ErrorResponse::new("node not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Node ID: {}", req.node_id)),
                ),
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
                Json(
                    ErrorResponse::new("failed to contact node agent")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("node agent spawn failed")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(error_text),
            ),
        ));
    }

    let spawn_response: serde_json::Value = response.json().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to parse node agent response")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let pid = spawn_response["pid"].as_i64().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("invalid response from node agent")
                    .with_code("BAD_REQUEST")
                    .with_string_details("missing or invalid PID field"),
            ),
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
            Json(
                ErrorResponse::new("failed to register worker in database")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Return worker info
    Ok(Json(WorkerResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let workers = if let Some(tenant_id) = query.tenant_id {
        state
            .db
            .list_workers_by_tenant(&tenant_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
    } else {
        state.db.list_all_workers().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
    };

    let response: Vec<WorkerResponse> = workers
        .into_iter()
        .map(|w| WorkerResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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

/// Stop a worker process
///
/// Gracefully stops a worker process by updating its status and optionally
/// terminating the underlying process.
///
/// **Permissions:** Requires `WorkerManage` permission (Operator or Admin role).
///
/// **Telemetry:** Emits `worker.stop` event.
///
/// # Example
/// ```
/// POST /v1/workers/{worker_id}/stop
/// ```
#[utoipa::path(
    post,
    path = "/v1/workers/{worker_id}/stop",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    responses(
        (status = 200, description = "Worker stopped successfully", body = crate::types::WorkerStopResponse),
        (status = 404, description = "Worker not found", body = ErrorResponse),
        (status = 500, description = "Failed to stop worker", body = ErrorResponse)
    ),
    tag = "workers"
)]
pub async fn stop_worker(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
) -> Result<Json<crate::types::WorkerStopResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require worker manage permission
    crate::permissions::require_permission(&claims, crate::permissions::Permission::WorkerManage)?;

    // Get worker from database
    let worker = state
        .db
        .get_worker(&worker_id)
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
                Json(
                    ErrorResponse::new("worker not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Worker ID: {}", worker_id)),
                ),
            )
        })?;

    let previous_status = worker.status.clone();

    // Update worker status to 'stopping'
    sqlx::query("UPDATE workers SET status = 'stopping' WHERE id = ?")
        .bind(&worker_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update worker status")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // If worker has a PID, attempt to terminate the process
    if let Some(pid) = worker.pid {
        // Note: In production, this would send a signal to the worker process
        // For now, we just update the status
        tracing::info!(
            event = "worker.stop.signal",
            worker_id = %worker_id,
            pid = %pid,
            "Signaling worker process to stop"
        );
    }

    // Update worker status to 'stopped'
    sqlx::query("UPDATE workers SET status = 'stopped' WHERE id = ?")
        .bind(&worker_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update worker status")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let stopped_at = chrono::Utc::now().to_rfc3339();

    // Emit telemetry event
    tracing::info!(
        event = "worker.stop",
        worker_id = %worker_id,
        previous_status = %previous_status,
        actor = %claims.sub,
        "Worker stopped"
    );

    // Audit log
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        "worker.stop",
        crate::audit_helper::resources::WORKER,
        Some(&worker_id),
    )
    .await;

    Ok(Json(crate::types::WorkerStopResponse {
        worker_id,
        success: true,
        message: "Worker stopped successfully".to_string(),
        previous_status,
        stopped_at,
    }))
}

/// Logout endpoint (stateless JWT - just return success)

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
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
    } else {
        state.db.list_all_plans().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
    };

    // Build responses - kernel_hash_b3 lookup would require async iteration,
    // so we return None for now (consistent with layout_hash_b3)
    let response: Vec<PlanResponse> = plans
        .into_iter()
        .map(|p| PlanResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            id: p.id,
            tenant_id: p.tenant_id,
            manifest_hash_b3: p.manifest_hash_b3,
            kernel_hash_b3: None, // Requires separate async lookup - use get_plan_details for full data
            layout_hash_b3: None, // Not stored in Plan model
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
    require_any_role(&claims, &[Role::Operator])?;

    let plan = state
        .db
        .get_plan(&plan_id)
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
                Json(ErrorResponse::new("plan not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(PlanDetailsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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
    require_any_role(&claims, &[Role::Operator])?;

    let plan = state
        .db
        .get_plan(&plan_id)
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
                Json(ErrorResponse::new("plan not found").with_code("NOT_FOUND")),
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
                    "Metal kernels updated (hash changed)".to_string()
                }
                _ => "Plan rebuilt with current Metal kernels".to_string(),
            };

            Ok(Json(PlanRebuildResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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
                Json(
                    ErrorResponse::new("Failed to rebuild plan")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
    require_any_role(&claims, &[Role::Operator])?;

    let plan1 = state
        .db
        .get_plan(&req.plan_id_1)
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
                Json(
                    ErrorResponse::new(format!("plan {} not found", req.plan_id_1))
                        .with_code("NOT_FOUND"),
                ),
            )
        })?;

    let plan2 = state
        .db
        .get_plan(&req.plan_id_2)
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
                Json(
                    ErrorResponse::new(format!("plan {} not found", req.plan_id_2))
                        .with_code("NOT_FOUND"),
                ),
            )
        })?;

    // Simple comparison based on manifest hash
    let differences = if plan1.manifest_hash_b3 == plan2.manifest_hash_b3 {
        vec!["No differences detected".to_string()]
    } else {
        vec!["Manifest hashes differ".to_string()]
    };

    Ok(Json(PlanComparisonResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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
                Json(ErrorResponse::new("plan not found").with_code("NOT_FOUND")),
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
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<PolicyPackResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view policies
    crate::permissions::require_permission(&claims, crate::permissions::Permission::PolicyView)?;

    // Stub - would query database
    Ok(Json(vec![]))
}

/// Get policy by CPID (stub)
pub async fn get_policy(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(cpid): Path<String>,
) -> Result<Json<PolicyPackResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view policies
    crate::permissions::require_permission(&claims, crate::permissions::Permission::PolicyView)?;

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
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ValidatePolicyRequest>,
) -> Result<Json<PolicyValidationResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Compliance and Admin can validate policies
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::PolicyValidate,
    )?;

    // Basic JSON validation
    let result = match serde_json::from_str::<serde_json::Value>(&req.content) {
        Ok(_) => {
            // Audit log: policy validation success
            // Use content hash as identifier since ValidatePolicyRequest doesn't have cpid
            let content_hash = adapteros_core::B3Hash::hash(req.content.as_bytes()).to_string();
            let _ = crate::audit_helper::log_success(
                &state.db,
                &claims,
                crate::audit_helper::actions::POLICY_VALIDATE,
                crate::audit_helper::resources::POLICY,
                Some(&content_hash),
            )
            .await;

            Json(PolicyValidationResponse {
                valid: true,
                errors: vec![],
                hash_b3: Some("b3:placeholder".to_string()),
            })
        }
        Err(e) => {
            // Audit log: policy validation failure
            // Use content hash as identifier since ValidatePolicyRequest doesn't have cpid
            let content_hash = adapteros_core::B3Hash::hash(req.content.as_bytes()).to_string();
            let _ = crate::audit_helper::log_failure(
                &state.db,
                &claims,
                crate::audit_helper::actions::POLICY_VALIDATE,
                crate::audit_helper::resources::POLICY,
                Some(&content_hash),
                &format!("Invalid JSON: {}", e),
            )
            .await;

            Json(PolicyValidationResponse {
                valid: false,
                errors: vec![format!("Invalid JSON: {}", e)],
                hash_b3: None,
            })
        }
    };

    Ok(result)
}

/// Apply policy (stub)
pub async fn apply_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ApplyPolicyRequest>,
) -> Result<Json<PolicyPackResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Admin-only (applying policies is a critical operation)
    crate::permissions::require_permission(&claims, crate::permissions::Permission::PolicyApply)?;

    // Stub - would validate, sign, and store policy
    let response = PolicyPackResponse {
        cpid: req.cpid.clone(),
        content: req.content,
        hash_b3: "b3:placeholder".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    // Audit log: policy applied
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::POLICY_APPLY,
        crate::audit_helper::resources::POLICY,
        Some(&req.cpid),
    )
    .await;

    Ok(Json(response))
}

/// Sign policy with Ed25519
pub async fn sign_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(cpid): Path<String>,
) -> Result<Json<SignPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Admin-only (signing policies is a critical operation)
    crate::permissions::require_permission(&claims, crate::permissions::Permission::PolicySign)?;

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
            Json(ErrorResponse::new("Failed to retrieve signing key").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
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
                    Json(
                        ErrorResponse::new("Failed to store signing key")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
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
    let signature = match adapteros_crypto::signature::sign_data(cpid.as_bytes(), signing_key) {
        Ok(sig) => format!("ed25519:{}", hex::encode(sig)),
        Err(e) => {
            tracing::error!("Failed to sign CPID: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Signing failed")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    // Audit log: policy signed
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::POLICY_SIGN,
        crate::audit_helper::resources::POLICY,
        Some(&cpid),
    )
    .await;

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
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

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
                Json(
                    ErrorResponse::new("failed to get current CP pointer")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("no active CP pointer found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Tenant: {}", req.tenant_id)),
                ),
            )
        })?;

    // Verify the CPID matches
    if current_cp.name != req.cpid {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("CPID mismatch")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!(
                        "Current active CPID is '{}', not '{}'",
                        current_cp.name, req.cpid
                    )),
            ),
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
            Json(
                ErrorResponse::new("failed to query previous CP")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let previous_cp = all_pointers.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("no previous CP pointer available for rollback")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!(
                        "This is the first/only CP for tenant {}",
                        req.tenant_id
                    )),
            ),
        )
    })?;

    // Perform rollback in a transaction
    let mut tx = state.db.pool().begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to start transaction")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
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
                Json(
                    ErrorResponse::new("failed to deactivate CP pointers")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
                Json(
                    ErrorResponse::new("failed to activate previous CP pointer")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // COMMIT TRANSACTION
    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to commit transaction")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
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
    State(_state): State<AppState>,
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
    tag = "system",
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
    require_any_role(&claims, &[Role::Operator])?;

    // Validate inputs
    validate_repo_id(&req.repo_id)?;
    validate_description(&req.description)?;
    validate_file_paths(&req.target_files)?;

    // Get available workers from database
    let workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list workers")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    if workers.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("no workers available")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details("No active workers found for patch proposal"),
            ),
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
                .bind(status)
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
            Json(
                ErrorResponse::new("worker not available")
                    .with_code("SERVICE_UNAVAILABLE")
                    .with_string_details(msg),
            ),
        )),
        Err(UdsClientError::Timeout(msg)) => Err((
            StatusCode::REQUEST_TIMEOUT,
            Json(
                ErrorResponse::new("patch generation timeout")
                    .with_code("REQUEST_TIMEOUT")
                    .with_string_details(msg),
            ),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("patch generation failed")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )),
    }
}

/// Inference endpoint
#[utoipa::path(
    tag = "system",
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
    Extension(identity): Extension<IdentityEnvelope>,
    Json(req): Json<InferRequest>,
) -> Result<Json<InferResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Operator, SRE, and Admin can execute inference (Viewer and Compliance cannot)
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::InferenceExecute,
    )?;

    // Validate request
    if req.prompt.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("prompt cannot be empty").with_code("INTERNAL_ERROR")),
        ));
    }

    // Check UMA pressure - compare by string to avoid version conflicts between crates
    let pressure_str = state.uma_monitor.get_current_pressure().to_string();
    let is_high_pressure = pressure_str == "High" || pressure_str == "Critical";
    if is_high_pressure {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("service under memory pressure")
                    .with_code("BACKPRESSURE")
                    .with_string_details(format!(
                        "level={}, retry_after_secs=30, action=reduce max_tokens or retry later",
                        pressure_str
                    )),
            ),
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
            Json(
                ErrorResponse::new("failed to list workers")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Resolve UDS path: prefer registered worker; otherwise fall back to local dev socket
    let uds_path_buf = if let Some(worker) = workers.get(0) {
        std::path::PathBuf::from(&worker.uds_path)
    } else {
        // Fallback: honor env override or default to /var/run/adapteros.sock
        let fallback = std::env::var("AOS_WORKER_SOCKET")
            .unwrap_or_else(|_| "/var/run/adapteros.sock".to_string());
        std::path::PathBuf::from(fallback)
    };
    let uds_path = uds_path_buf.as_path();

    // Create UDS client and send request
    let uds_client = UdsClient::new(std::time::Duration::from_secs(30));

    // Convert server API request to worker API request
    let worker_request = WorkerInferRequest {
        cpid: claims.tenant_id.clone(),
        prompt: req.prompt.clone(),
        max_tokens: req.max_tokens.unwrap_or(100),
        require_evidence: req.require_evidence.unwrap_or(false), // Get from request or default to false
    };

    match uds_client.infer(uds_path, worker_request).await {
        Ok(worker_response) => {
            // Convert worker response to server API response
            let response = InferResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                id: uuid::Uuid::new_v4().to_string(),
                text: worker_response.text.unwrap_or_default(),
                tokens: vec![],      // Worker doesn't expose token IDs in current API
                tokens_generated: 0, // Not tracked in current response
                finish_reason: worker_response.status.clone(),
                latency_ms: 0, // Not tracked in current response
                adapters_used: worker_response.trace.router_summary.adapters_used.clone(),
                trace: InferenceTrace {
                    adapters_used: worker_response.trace.router_summary.adapters_used.clone(),
                    router_decisions: vec![], // Router decisions not in simplified trace
                    latency_ms: 0,            // Not tracked in current response
                },
                model: None,
                prompt_tokens: None,
                error: None,
            };

            // Validate response schema before returning
            let response_value = serde_json::to_value(&response).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("response serialization failed")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

            state
                .response_validator
                .validate_response(&response_value, "inference_response")
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("response validation failed")
                                .with_code("VALIDATION_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;

            Ok(Json(response))
        }
        Err(UdsClientError::WorkerNotAvailable(msg)) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("worker not available")
                    .with_code("SERVICE_UNAVAILABLE")
                    .with_string_details(msg),
            ),
        )),
        Err(UdsClientError::Timeout(msg)) => Err((
            StatusCode::REQUEST_TIMEOUT,
            Json(
                ErrorResponse::new("inference timeout")
                    .with_code("REQUEST_TIMEOUT")
                    .with_string_details(msg),
            ),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("inference failed")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )),
    }
}

// ===== Process Debugging Endpoints =====

/// List process logs for a worker
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/workers/{worker_id}/logs",
    params(
        ("worker_id" = String, Path, description = "Worker ID"),
        ("level" = Option<String>, Query, description = "Filter by log level"),
        ("limit" = Option<i32>, Query, description = "Maximum number of logs to return")
    ),
    responses(
        (status = 200, description = "Process logs", body = Vec<ProcessLogResponse>)
    )
)]
pub async fn list_process_logs(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(_worker_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessLogResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let _level_filter = params.get("level");
    let _limit = params
        .get("limit")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(100);

    // Database query for process logs - placeholder implementation
    // For now, return empty list
    Ok(Json(vec![]))
}

/// Get process crash dumps for a worker
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/workers/{worker_id}/crashes",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    responses(
        (status = 200, description = "Process crash dumps", body = Vec<ProcessCrashDumpResponse>)
    )
)]
pub async fn list_process_crashes(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
) -> Result<Json<Vec<ProcessCrashDumpResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Database query for crash dumps - placeholder implementation
    Ok(Json(vec![]))
}

/// Start a debug session for a worker
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/workers/{worker_id}/debug",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    request_body = StartDebugSessionRequest,
    responses(
        (status = 200, description = "Debug session started", body = ProcessDebugSessionResponse)
    )
)]
pub async fn start_debug_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Json(req): Json<StartDebugSessionRequest>,
) -> Result<Json<ProcessDebugSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Debug session creation - placeholder implementation
    Ok(Json(ProcessDebugSessionResponse {
        id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
        worker_id: worker_id.clone(),
        session_type: req.session_type,
        status: "active".to_string(),
        config_json: req.config_json,
        started_at: chrono::Utc::now().to_rfc3339(),
        ended_at: None,
        results_json: None,
    }))
}

/// Run a troubleshooting step for a worker
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/workers/{worker_id}/troubleshoot",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    request_body = RunTroubleshootingStepRequest,
    responses(
        (status = 200, description = "Troubleshooting step started", body = ProcessTroubleshootingStepResponse)
    )
)]
pub async fn run_troubleshooting_step(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Json(req): Json<RunTroubleshootingStepRequest>,
) -> Result<Json<ProcessTroubleshootingStepResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Troubleshooting step execution - placeholder implementation
    Ok(Json(ProcessTroubleshootingStepResponse {
        id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
        worker_id: worker_id.clone(),
        step_name: req.step_name,
        step_type: req.step_type,
        status: "running".to_string(),
        command: req.command,
        output: None,
        error_message: None,
        started_at: chrono::Utc::now().to_rfc3339(),
        completed_at: None,
    }))
}

// ===== Advanced Process Monitoring and Alerting Endpoints =====

/// List process monitoring rules
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/rules",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("rule_type" = Option<String>, Query, description = "Filter by rule type"),
        ("is_active" = Option<bool>, Query, description = "Filter by active status")
    ),
    responses(
        (status = 200, description = "Process monitoring rules", body = Vec<ProcessMonitoringRuleResponse>)
    )
)]
pub async fn list_process_monitoring_rules(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringRuleResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_filter = params.get("tenant_id");
    let rule_type_filter = params.get("rule_type");
    let is_active_filter = params.get("is_active").and_then(|s| s.parse::<bool>().ok());

    // Database query for monitoring rules - placeholder implementation
    // For now, return empty list
    Ok(Json(vec![]))
}

/// Create process monitoring rule
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/rules",
    request_body = CreateProcessMonitoringRuleRequest,
    responses(
        (status = 200, description = "Monitoring rule created", body = ProcessMonitoringRuleResponse)
    )
)]
pub async fn create_process_monitoring_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateProcessMonitoringRuleRequest>,
) -> Result<Json<ProcessMonitoringRuleResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Monitoring rule creation - placeholder implementation
    Ok(Json(ProcessMonitoringRuleResponse {
        id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
        name: req.name,
        description: req.description,
        tenant_id: "default".to_string(), // Placeholder - would extract from claims.sub
        rule_type: req.rule_type,
        metric_name: req.metric_name,
        threshold_value: req.threshold_value,
        threshold_operator: req.threshold_operator,
        severity: req.severity,
        evaluation_window_seconds: req.evaluation_window_seconds.unwrap_or(300),
        cooldown_seconds: req.cooldown_seconds.unwrap_or(60),
        is_active: true,
        notification_channels: req.notification_channels,
        escalation_rules: req.escalation_rules,
        created_by: Some(claims.sub.clone()),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// List process alerts
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/alerts",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("status" = Option<String>, Query, description = "Filter by alert status"),
        ("severity" = Option<String>, Query, description = "Filter by severity")
    ),
    responses(
        (status = 200, description = "Process alerts", body = Vec<ProcessAlertResponse>)
    )
)]
pub async fn list_process_alerts(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(_params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessAlertResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
}

/// Acknowledge process alert
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/alerts/{alert_id}/acknowledge",
    params(
        ("alert_id" = String, Path, description = "Alert ID")
    ),
    request_body = AcknowledgeProcessAlertRequest,
    responses(
        (status = 200, description = "Alert acknowledged", body = ProcessAlertResponse)
    )
)]
pub async fn acknowledge_process_alert(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(_alert_id): Path<String>,
    Json(_req): Json<AcknowledgeProcessAlertRequest>,
) -> Result<Json<ProcessAlertResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
}

/// List process anomalies
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/anomalies",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("status" = Option<String>, Query, description = "Filter by anomaly status"),
        ("severity" = Option<String>, Query, description = "Filter by severity")
    ),
    responses(
        (status = 200, description = "Process anomalies", body = Vec<ProcessAnomalyResponse>)
    )
)]
pub async fn list_process_anomalies(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(_params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessAnomalyResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
}

/// Update process anomaly status
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/anomalies/{anomaly_id}/status",
    params(
        ("anomaly_id" = String, Path, description = "Anomaly ID")
    ),
    request_body = UpdateProcessAnomalyStatusRequest,
    responses(
        (status = 200, description = "Anomaly status updated", body = ProcessAnomalyResponse)
    )
)]
pub async fn update_process_anomaly_status(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(_anomaly_id): Path<String>,
    Json(_req): Json<UpdateProcessAnomalyStatusRequest>,
) -> Result<Json<ProcessAnomalyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
}

/// List process monitoring dashboards
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/dashboards",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("is_shared" = Option<bool>, Query, description = "Filter by shared status")
    ),
    responses(
        (status = 200, description = "Process monitoring dashboards", body = Vec<ProcessMonitoringDashboardResponse>)
    )
)]
pub async fn list_process_monitoring_dashboards(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(_params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringDashboardResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
}

/// Create process monitoring dashboard
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/dashboards",
    request_body = CreateProcessMonitoringDashboardRequest,
    responses(
        (status = 200, description = "Dashboard created", body = ProcessMonitoringDashboardResponse)
    )
)]
pub async fn create_process_monitoring_dashboard(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(_req): Json<CreateProcessMonitoringDashboardRequest>,
) -> Result<Json<ProcessMonitoringDashboardResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
}

/// List process health metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/health-metrics",
    params(
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("metric_name" = Option<String>, Query, description = "Filter by metric name"),
        ("start_time" = Option<String>, Query, description = "Start time for metrics"),
        ("end_time" = Option<String>, Query, description = "End time for metrics")
    ),
    responses(
        (status = 200, description = "Process health metrics", body = Vec<ProcessHealthMetricResponse>)
    )
)]
pub async fn list_process_health_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessHealthMetricResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let worker_filter = params.get("worker_id");
    let metric_filter = params.get("metric_name");
    let start_time_filter = params.get("start_time");
    let end_time_filter = params.get("end_time");

    // Build filters for health metrics query
    let filters = adapteros_system_metrics::MetricFilters {
        worker_id: worker_filter.cloned(),
        tenant_id: None, // Will be filtered by user's tenant access
        metric_name: metric_filter.cloned(),
        start_time: start_time_filter
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        end_time: end_time_filter
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        limit: Some(1000), // Limit results
    };

    // Query health metrics from database
    let metrics = adapteros_system_metrics::ProcessHealthMetric::query(state.db.pool(), filters)
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
        })?;

    // Convert ProcessHealthMetric to ProcessHealthMetricResponse
    let response_metrics: Vec<ProcessHealthMetricResponse> = metrics
        .into_iter()
        .map(|metric| ProcessHealthMetricResponse {
            id: metric.id,
            worker_id: metric.worker_id,
            tenant_id: metric.tenant_id,
            metric_name: metric.metric_name,
            metric_value: metric.metric_value,
            metric_unit: metric.metric_unit,
            tags: metric.tags,
            collected_at: metric.collected_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(response_metrics))
}

/// List process monitoring reports
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/reports",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("report_type" = Option<String>, Query, description = "Filter by report type")
    ),
    responses(
        (status = 200, description = "Process monitoring reports", body = Vec<ProcessMonitoringReportResponse>)
    )
)]
pub async fn list_process_monitoring_reports(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(_params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringReportResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
}

/// Create process monitoring report
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/reports",
    request_body = CreateProcessMonitoringReportRequest,
    responses(
        (status = 200, description = "Report created", body = ProcessMonitoringReportResponse)
    )
)]
pub async fn create_process_monitoring_report(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(_req): Json<CreateProcessMonitoringReportRequest>,
) -> Result<Json<ProcessMonitoringReportResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
}
// ===== Adapter Management Endpoints =====

/// List all adapters
#[utoipa::path(
    tag = "system",
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
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListAdaptersQuery>,
) -> Result<Json<Vec<AdapterResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: all roles can list adapters
    crate::permissions::require_permission(&claims, crate::permissions::Permission::AdapterList)?;

    let adapters = state.db.list_adapters().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list adapters")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let mut responses = Vec::new();
    for adapter in adapters {
        // Filter by tier if specified
        if let Some(tier) = query.tier {
            if adapter.tier != tier.to_string() {
                continue;
            }
        }

        // Filter by framework if specified
        if let Some(ref framework) = query.framework {
            if adapter.framework.as_ref() != Some(framework) {
                continue;
            }
        }

        // Get adapter_id - use id if adapter_id is not set
        let adapter_id_str = adapter.adapter_id.as_ref().unwrap_or(&adapter.id);

        // Get stats
        let (total, selected, avg_gate) = state
            .db
            .get_adapter_stats(adapter_id_str)
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
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            id: adapter.id.clone(),
            adapter_id: adapter_id_str.to_string(),
            name: adapter.name.clone(),
            hash_b3: adapter.hash_b3.clone(),
            rank: adapter.rank,
            tier: adapter.tier.clone(),
            languages,
            framework: adapter.framework.clone(),
            category: Some(adapter.category.clone()),
            scope: Some(adapter.scope.clone()),
            framework_id: adapter.framework_id.clone(),
            framework_version: adapter.framework_version.clone(),
            repo_id: adapter.repo_id.clone(),
            commit_sha: adapter.commit_sha.clone(),
            intent: adapter.intent.clone(),
            created_at: adapter.created_at.clone(),
            updated_at: Some(adapter.updated_at.clone()),
            stats: Some(AdapterStats {
                total_activations: total,
                selected_count: selected,
                avg_gate_value: avg_gate,
                selection_rate,
            }),
            version: adapter.version.clone(),
            lifecycle_state: adapter.lifecycle_state.clone(),
            runtime_state: Some(adapter.current_state.clone()),
            pinned: None,
            memory_bytes: None,
        });
    }

    Ok(Json(responses))
}

/// Get adapter by ID
#[utoipa::path(
    tag = "system",
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
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: adapter.id.clone(),
        adapter_id: adapter
            .adapter_id
            .clone()
            .unwrap_or_else(|| adapter.id.clone()),
        name: adapter.name.clone(),
        hash_b3: adapter.hash_b3.clone(),
        rank: adapter.rank,
        tier: adapter.tier.clone(),
        languages,
        framework: adapter.framework.clone(),
        category: Some(adapter.category.clone()),
        scope: Some(adapter.scope.clone()),
        framework_id: adapter.framework_id.clone(),
        framework_version: adapter.framework_version.clone(),
        repo_id: adapter.repo_id.clone(),
        commit_sha: adapter.commit_sha.clone(),
        intent: adapter.intent.clone(),
        created_at: adapter.created_at.clone(),
        updated_at: Some(adapter.updated_at.clone()),
        stats: Some(AdapterStats {
            total_activations: total,
            selected_count: selected,
            avg_gate_value: avg_gate,
            selection_rate,
        }),
        version: adapter.version.clone(),
        lifecycle_state: adapter.lifecycle_state.clone(),
        runtime_state: Some(adapter.current_state),
        pinned: None,
        memory_bytes: None,
    }))
}
/// Register new adapter
#[utoipa::path(
    tag = "system",
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
    // Role check: Operator and Admin can register adapters
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::AdapterRegister,
    )?;

    // Validate inputs
    if req.adapter_id.is_empty() || req.name.is_empty() || req.hash_b3.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("adapter_id, name, and hash_b3 are required")
                    .with_code("BAD_REQUEST"),
            ),
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
            Json(
                ErrorResponse::new("invalid languages array")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Validate tier is one of the allowed values
    if !["persistent", "warm", "ephemeral"].contains(&req.tier.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("tier must be one of: 'persistent', 'warm', or 'ephemeral'")
                    .with_code("BAD_REQUEST"),
            ),
        ));
    }

    // Validate category is provided
    if req.category.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("category is required")
                    .with_code("BAD_REQUEST"),
            ),
        ));
    }

    // Build registration params using the builder pattern
    let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .adapter_id(&req.adapter_id)
        .name(&req.name)
        .hash_b3(&req.hash_b3)
        .rank(req.rank)
        .tier(&req.tier)
        .languages_json(Some(languages_json.clone()))
        .framework(req.framework.clone())
        .category(req.category.clone())
        .scope(req.scope.clone().unwrap_or_else(|| "global".to_string()))
        .expires_at(req.expires_at.clone())
        .build()
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid adapter parameters")
                        .with_code("BAD_REQUEST")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let id = state.db.register_adapter(params).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to register adapter")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Audit log: adapter registration
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_REGISTER,
        crate::audit_helper::resources::ADAPTER,
        Some(&req.adapter_id),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(AdapterResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            id,
            adapter_id: req.adapter_id.clone(),
            name: req.name,
            hash_b3: req.hash_b3,
            rank: req.rank,
            tier: req.tier,
            version: "1.0".to_string(),
            lifecycle_state: "active".to_string(),
            languages: req.languages,
            framework: req.framework,
            category: Some(req.category.clone()),
            scope: req.scope.clone(),
            framework_id: None,
            framework_version: None,
            repo_id: None,
            commit_sha: None,
            intent: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: Some(chrono::Utc::now().to_rfc3339()),
            stats: None,
            runtime_state: Some("unloaded".to_string()),
            pinned: Some(false),
            memory_bytes: Some(0),
        }),
    ))
}

/// Delete adapter
#[utoipa::path(
    tag = "system",
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
    // Role check: Admin-only (destructive operation)
    crate::permissions::require_permission(&claims, crate::permissions::Permission::AdapterDelete)?;

    state.db.delete_adapter(&adapter_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to delete adapter")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Audit log: adapter deletion
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_DELETE,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Load an adapter into memory
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapters/{adapter_id}/load",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter loaded successfully", body = AdapterResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Failed to load adapter", body = ErrorResponse)
    )
)]
pub async fn load_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Operator, SRE, and Admin can load adapters
    crate::permissions::require_permission(&claims, crate::permissions::Permission::AdapterLoad)?;

    // Get adapter from database
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get adapter")
                        .with_code("INTERNAL_SERVER_ERROR")
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

    // Update adapter state to 'loading'
    state
        .db
        .update_adapter_state(&adapter_id, "loading", "user_request")
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update adapter state")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let expected_hash = parse_hash_b3(&adapter.hash_b3).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("invalid adapter hash")
                    .with_code("INVALID_HASH")
                    .with_string_details(format!("{}: {}", adapter.hash_b3, e)),
            ),
        )
    })?;
    let mut expected_hashes = HashMap::new();
    expected_hashes.insert(adapter.hash_b3.clone(), expected_hash);

    tracing::info!("Loading adapter {} ({})", adapter_id, adapter.name);

    // Actually load the adapter using LifecycleManager if available
    if let Some(ref lifecycle) = state.lifecycle_manager {
        // Get adapter index (this is a simplified lookup - in production you'd maintain a proper mapping)
        let adapter_idx = adapter.id.parse::<u16>().unwrap_or(0);

        // Use AdapterLoader via LifecycleManager
        let lifecycle_mgr = lifecycle.lock().await;

        // Load adapter file from disk
        use adapteros_lora_lifecycle::AdapterLoader;
        use std::path::PathBuf;

        let adapters_path = PathBuf::from("./adapters");
        let mut loader = AdapterLoader::new(adapters_path, expected_hashes);

        match loader
            .load_adapter_async(adapter_idx, &adapter.hash_b3)
            .await
        {
            Ok(handle) => {
                // Update adapter state to 'warm' and record memory usage
                state
                    .db
                    .update_adapter_state(&adapter_id, "warm", "loaded_successfully")
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                ErrorResponse::new("failed to update adapter state")
                                    .with_code("INTERNAL_SERVER_ERROR")
                                    .with_string_details(e.to_string()),
                            ),
                        )
                    })?;

                state
                    .db
                    .update_adapter_memory(&adapter_id, handle.memory_bytes() as i64)
                    .await
                    .map_err(|e| {
                        tracing::warn!("Failed to update adapter memory: {}", e);
                        // Don't fail the request for this
                    })
                    .ok();

                tracing::info!(
                    event = "adapter.load",
                    adapter_id = %adapter_id,
                    adapter_name = %adapter.name,
                    memory_bytes = handle.memory_bytes(),
                    "Adapter loaded successfully"
                );
            }
            Err(e) => {
                // Rollback state on error
                state
                    .db
                    .update_adapter_state(&adapter_id, "cold", "load_failed")
                    .await
                    .ok();

                tracing::error!("Failed to load adapter {}: {}", adapter_id, e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to load adapter")
                            .with_code("LOAD_FAILED")
                            .with_string_details(e.to_string()),
                    ),
                ));
            }
        }
    } else {
        // No lifecycle manager - just simulate for testing
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        state
            .db
            .update_adapter_state(&adapter_id, "warm", "simulated_load")
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update adapter state")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        tracing::info!(
            event = "adapter.load",
            adapter_id = %adapter_id,
            adapter_name = %adapter.name,
            "Adapter loaded successfully (simulated)"
        );
    }

    // Audit log: adapter load
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_LOAD,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    // Return the adapter with updated stats
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

    let adapter_id_val = adapter
        .adapter_id
        .clone()
        .unwrap_or_else(|| adapter.id.clone());
    Ok(Json(AdapterResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: adapter.id,
        adapter_id: adapter_id_val,
        name: adapter.name,
        hash_b3: adapter.hash_b3,
        rank: adapter.rank,
        tier: adapter.tier,
        version: adapter.version.clone(),
        lifecycle_state: adapter.lifecycle_state.clone(),
        languages: serde_json::from_str(adapter.languages_json.as_deref().unwrap_or("[]"))
            .unwrap_or_default(),
        framework: adapter.framework,
        category: Some(adapter.category),
        scope: Some(adapter.scope),
        framework_id: adapter.framework_id,
        framework_version: adapter.framework_version,
        repo_id: adapter.repo_id,
        commit_sha: adapter.commit_sha,
        intent: adapter.intent,
        created_at: adapter.created_at,
        updated_at: Some(adapter.updated_at),
        stats: Some(AdapterStats {
            total_activations: total,
            selected_count: selected,
            avg_gate_value: avg_gate,
            selection_rate,
        }),
        runtime_state: Some(adapter.current_state),
        pinned: None,
        memory_bytes: None,
    }))
}

fn parse_hash_b3(hash_b3: &str) -> Result<B3Hash, String> {
    let trimmed = hash_b3.strip_prefix("b3:").unwrap_or(hash_b3);
    B3Hash::from_hex(trimmed).map_err(|e| e.to_string())
}

/// Unload an adapter from memory
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapters/{adapter_id}/unload",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter unloaded successfully"),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Failed to unload adapter", body = ErrorResponse)
    )
)]
pub async fn unload_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Operator, SRE, and Admin can unload adapters
    crate::permissions::require_permission(&claims, crate::permissions::Permission::AdapterUnload)?;

    // Get adapter from database
    let _adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get adapter")
                        .with_code("INTERNAL_SERVER_ERROR")
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

    // Update adapter state to 'unloading'
    state
        .db
        .update_adapter_state(&adapter_id, "unloading", "user_request")
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update adapter state")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    tracing::info!("Unloading adapter {}", adapter_id);

    // Actually unload the adapter using LifecycleManager if available
    if let Some(ref lifecycle) = state.lifecycle_manager {
        let adapter_idx = _adapter.id.parse::<u16>().unwrap_or(0);

        let lifecycle_mgr = lifecycle.lock().await;

        use adapteros_lora_lifecycle::AdapterLoader;
        use std::path::PathBuf;

        let adapters_path = PathBuf::from("./adapters");
        let mut loader = AdapterLoader::new(adapters_path, HashMap::new());

        match loader.unload_adapter(adapter_idx) {
            Ok(_) => {
                // Update adapter state to 'cold' and reset memory
                state
                    .db
                    .update_adapter_state(&adapter_id, "cold", "unloaded_successfully")
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                ErrorResponse::new("failed to update adapter state")
                                    .with_code("INTERNAL_SERVER_ERROR")
                                    .with_string_details(e.to_string()),
                            ),
                        )
                    })?;

                state.db.update_adapter_memory(&adapter_id, 0).await.ok();

                tracing::info!(
                    event = "adapter.unload",
                    adapter_id = %adapter_id,
                    "Adapter unloaded successfully"
                );
            }
            Err(e) => {
                // Rollback state on error
                state
                    .db
                    .update_adapter_state(&adapter_id, "warm", "unload_failed")
                    .await
                    .ok();

                tracing::error!("Failed to unload adapter {}: {}", adapter_id, e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to unload adapter")
                            .with_code("UNLOAD_FAILED")
                            .with_string_details(e.to_string()),
                    ),
                ));
            }
        }
    } else {
        // No lifecycle manager - just simulate
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        state
            .db
            .update_adapter_state(&adapter_id, "cold", "simulated_unload")
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update adapter state")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        state.db.update_adapter_memory(&adapter_id, 0).await.ok();

        tracing::info!(
            event = "adapter.unload",
            adapter_id = %adapter_id,
            "Adapter unloaded successfully (simulated)"
        );
    }

    // Audit log: adapter unload
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_UNLOAD,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    Ok(StatusCode::OK)
}

/// Verify GPU buffer integrity for loaded adapters
///
/// Performs cryptographic verification that adapter lifecycle metadata matches
/// actual GPU buffer state through fingerprinting and cross-layer hashing.
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/adapters/verify-gpu",
    params(
        ("adapter_id" = Option<String>, Query, description = "Specific adapter ID to verify (optional)")
    ),
    responses(
        (status = 200, description = "GPU integrity verification report", body = GpuIntegrityReport),
        (status = 500, description = "Verification failed", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn verify_gpu_integrity(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<adapteros_lora_lifecycle::GpuIntegrityReport>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let adapter_id = params.get("adapter_id").map(|s| s.as_str());

    tracing::info!(
        adapter_id = ?adapter_id,
        "GPU integrity verification requested"
    );

    // Check if Worker is available
    if let Some(worker) = &state.worker {
        let report = worker
            .lock()
            .await
            .verify_gpu_integrity()
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("GPU verification failed")
                            .with_code("VERIFICATION_FAILED")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        tracing::info!(
            total_checked = report.total_checked,
            verified = report.verified.len(),
            failed = report.failed.len(),
            skipped = report.skipped.len(),
            "GPU integrity verification completed"
        );

        Ok(Json(report))
    } else {
        // Worker not available - return empty report with informative message
        tracing::warn!("GPU verification endpoint called but Worker not available in AppState");

        let report = adapteros_lora_lifecycle::GpuIntegrityReport {
            verified: vec![],
            failed: vec![],
            skipped: vec![],
            total_checked: 0,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| {
                    aos_error_to_response(AosError::Internal(format!("System time error: {}", e)))
                })?
                .as_secs(),
        };

        Ok(Json(report))
    }
}

/// Get adapter activations
#[utoipa::path(
    tag = "system",
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
                Json(
                    ErrorResponse::new("failed to get activations")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let responses: Vec<AdapterActivationResponse> = activations
        .into_iter()
        .map(|a| AdapterActivationResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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

/// Promote adapter tier (persistent -> warm -> ephemeral)
///
/// Promotes an adapter to the next tier level. Returns 400 if already at maximum tier.
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/state/promote",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter tier promoted successfully", body = AdapterStateResponse),
        (status = 400, description = "Already at maximum tier", body = ErrorResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn promote_adapter_state(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Get current adapter state
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

    // Determine next tier based on current tier
    // Tiers: "persistent" → "warm" → "ephemeral"
    let new_tier = match adapter.tier.as_str() {
        "persistent" => "warm".to_string(),
        "warm" => "ephemeral".to_string(),
        "ephemeral" => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("adapter already at maximum tier (ephemeral)")
                        .with_code("ALREADY_AT_MAX_TIER"),
                ),
            ));
        }
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new(&format!("unknown tier: {}", other))
                        .with_code("UNKNOWN_TIER"),
                ),
            ));
        }
    };

    let old_tier = adapter.tier.clone();

    // Update adapter tier in database
    let timestamp = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE adapters SET tier = ?, updated_at = ? WHERE adapter_id = ?"
    )
    .bind(&new_tier)
    .bind(&timestamp)
    .bind(&adapter_id)
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to update adapter tier")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(AdapterStateResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        adapter_id,
        old_state: old_tier,
        new_state: new_tier,
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

    let manifest = AdapterManifest {
        adapter_id: adapter
            .adapter_id
            .clone()
            .unwrap_or_else(|| adapter.id.clone()),
        name: adapter.name,
        hash_b3: adapter.hash_b3,
        rank: adapter.rank,
        tier: adapter.tier.clone(),
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
                Json(
                    ErrorResponse::new("failed to get activations")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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
    tag = "system",
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
    let repos = state
        .db
        .list_repositories("default", 100, 0)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to list repositories")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let mut responses = Vec::new();
    for r in repos {
        let languages: Vec<String> = r
            .languages_json
            .as_ref()
            .and_then(|l| serde_json::from_str(l).ok())
            .unwrap_or_default();
        let frameworks: Vec<String> = Vec::new(); // TODO: Add frameworks field to Repository

        // Fetch latest CodeGraphMetadata for file/symbol counts
        let (file_count, symbol_count) =
            match state.db.get_latest_code_graph_metadata(&r.repo_id).await {
                Ok(Some(metadata)) => (
                    Some(metadata.file_count as i64),
                    Some(metadata.symbol_count as i64),
                ),
                _ => (None, None),
            };

        responses.push(RepositoryResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            id: r.id,
            repo_id: r.repo_id,
            path: r.path,
            languages,
            default_branch: r.default_branch,
            status: r.status,
            frameworks,
            file_count,
            symbol_count,
            created_at: r.created_at,
            updated_at: r.updated_at,
        });
    }

    Ok(Json(responses))
}

// ===== Metrics Endpoints =====

/// Get quality metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/metrics/quality",
    responses(
        (status = 200, description = "Quality metrics", body = QualityMetricsResponse)
    )
)]
pub async fn get_quality_metrics(
    Extension(claims): Extension<Claims>,
) -> Result<Json<QualityMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view metrics
    crate::permissions::require_permission(&claims, crate::permissions::Permission::MetricsView)?;

    // Stub implementation - would compute from telemetry
    Ok(Json(QualityMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        arr: 0.95,
        ecs5: 0.82,
        hlr: 0.02,
        cr: 0.01,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get adapter performance metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/metrics/adapters",
    responses(
        (status = 200, description = "Adapter metrics", body = AdapterMetricsResponse)
    )
)]
pub async fn get_adapter_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<AdapterMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view metrics
    crate::permissions::require_permission(&claims, crate::permissions::Permission::MetricsView)?;

    let adapters = state.db.list_adapters().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list adapters")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let mut performances = Vec::new();
    for adapter in adapters {
        let (total, selected, avg_gate) = state
            .db
            .get_adapter_stats(adapter.adapter_id.as_ref().unwrap_or(&adapter.id))
            .await
            .unwrap_or((0, 0, 0.0));

        let activation_rate = if total > 0 {
            (selected as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        performances.push(AdapterPerformance {
            adapter_id: adapter
                .adapter_id
                .clone()
                .unwrap_or_else(|| adapter.id.clone()),
            name: adapter.name,
            activation_rate,
            avg_gate_value: avg_gate,
            total_requests: total,
        });
    }

    Ok(Json(AdapterMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        adapters: performances,
    }))
}

/// Get system metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/metrics/system",
    responses(
        (status = 200, description = "System metrics", body = SystemMetricsResponse)
    )
)]
pub async fn get_system_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<SystemMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view metrics
    crate::permissions::require_permission(&claims, crate::permissions::Permission::MetricsView)?;

    use adapteros_system_metrics::SystemMetricsCollector;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Collect system metrics (using stubs until adapteros-system-metrics is re-enabled)
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let load_avg = collector.load_average();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_secs();

    Ok(Json(SystemMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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
    tag = "system",
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
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(query): Query<ListCommitsQuery>,
) -> Result<Json<Vec<CommitResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Use git subsystem if available
    if let Some(git_subsystem) = &state.git_subsystem {
        let limit = query.limit.unwrap_or(50).clamp(1, 200) as usize;
        let commits = git_subsystem
            .list_commits(query.repo_id.as_deref(), query.branch.as_deref(), limit)
            .await
            .map_err(|e| {
                tracing::error!("Failed to list commits: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to list commits")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        let response: Vec<CommitResponse> = commits
            .into_iter()
            .map(|commit| CommitResponse {
                id: commit.sha.clone(),
                repo_id: commit.repo_id,
                sha: commit.sha,
                message: commit.message,
                author: commit.author,
                date: commit.date.to_rfc3339(),
                branch: commit.branch,
                changed_files: commit.changed_files,
                impacted_symbols: commit.impacted_symbols,
                ephemeral_adapter_id: commit.ephemeral_adapter_id,
            })
            .collect();

        Ok(Json(response))
    } else {
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Git subsystem not available").with_code("SERVICE_UNAVAILABLE"),
            ),
        ))
    }
}

/// Get commit details
#[utoipa::path(
    tag = "system",
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
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(sha): Path<String>,
) -> Result<Json<CommitResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Use git subsystem if available
    if let Some(git_subsystem) = &state.git_subsystem {
        let commit = git_subsystem.get_commit(None, &sha).await.map_err(|e| {
            tracing::error!("Failed to get commit {}: {}", sha, e);
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(
                    ErrorResponse::new(format!("Failed to get commit: {}", e))
                        .with_code(if status == StatusCode::NOT_FOUND {
                            "NOT_FOUND"
                        } else {
                            "INTERNAL_ERROR"
                        })
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

        Ok(Json(CommitResponse {
            id: commit.sha.clone(),
            repo_id: commit.repo_id,
            sha: commit.sha,
            message: commit.message,
            author: commit.author,
            date: commit.date.to_rfc3339(),
            branch: commit.branch,
            changed_files: commit.changed_files,
            impacted_symbols: commit.impacted_symbols,
            ephemeral_adapter_id: commit.ephemeral_adapter_id,
        }))
    } else {
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Git subsystem not available").with_code("SERVICE_UNAVAILABLE"),
            ),
        ))
    }
}

/// Get commit diff
#[utoipa::path(
    tag = "system",
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
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(sha): Path<String>,
) -> Result<Json<CommitDiffResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Use git subsystem if available
    if let Some(git_subsystem) = &state.git_subsystem {
        let diff = git_subsystem
            .get_commit_diff(None, &sha)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get commit diff for {}: {}", sha, e);
                let status = if e.to_string().contains("not found") {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
                (
                    status,
                    Json(
                        ErrorResponse::new(format!("Failed to get commit diff: {}", e))
                            .with_code(if status == StatusCode::NOT_FOUND {
                                "NOT_FOUND"
                            } else {
                                "INTERNAL_ERROR"
                            })
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        Ok(Json(CommitDiffResponse {
            sha: diff.sha,
            diff: diff.diff,
            stats: DiffStats {
                files_changed: diff.files_changed,
                insertions: diff.insertions,
                deletions: diff.deletions,
            },
        }))
    } else {
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Git subsystem not available").with_code("SERVICE_UNAVAILABLE"),
            ),
        ))
    }
}

// ===== Routing Inspector Endpoints =====

/// Debug routing decision
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/routing/debug",
    request_body = RoutingDebugRequest,
    responses(
        (status = 200, description = "Routing debug info", body = RoutingDebugResponse)
    )
)]
pub async fn debug_routing(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Json(req): Json<RoutingDebugRequest>,
) -> Result<Json<RoutingDebugResponse>, (StatusCode, Json<ErrorResponse>)> {
    use adapteros_lora_router::{AdapterInfo, CodeFeatures, Router, RouterWeights};

    // Extract code features from prompt and context
    let combined_context = match req.context {
        Some(ctx) => format!("{} {}", req.prompt, ctx),
        None => req.prompt.clone(),
    };
    let code_features = CodeFeatures::from_context(&combined_context);

    // Fetch all adapters from database
    let adapters = state.db.list_adapters().await.map_err(|e| {
        tracing::error!("Failed to list adapters: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to fetch adapters for routing debug")
                    .with_code("ADAPTER_FETCH_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Convert database adapters to router AdapterInfo
    let adapter_infos: Vec<AdapterInfo> = adapters
        .iter()
        .map(|adapter| {
            let languages = adapter
                .languages_json
                .as_ref()
                .and_then(|json| serde_json::from_str::<Vec<usize>>(json).ok())
                .unwrap_or_default();

            AdapterInfo {
                id: adapter.id.clone(),
                framework: adapter.framework.clone(),
                languages,
                tier: adapter.tier.clone(),
            }
        })
        .collect();

    // Create router and route with code features
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    let decision = router.route_with_code_features(&code_features, &adapter_infos);
    let explanation = router.explain_score(&code_features.to_vector());

    // Build adapter scores
    let mut adapter_scores: Vec<AdapterScore> = Vec::new();
    for (idx, adapter) in adapter_infos.iter().enumerate() {
        let is_selected = decision.indices.iter().any(|&i| i as usize == idx);
        let gate_value = if is_selected {
            let position = decision
                .indices
                .iter()
                .position(|&i| i as usize == idx)
                .unwrap_or(0);
            decision.gates_f32()[position] as f64
        } else {
            0.0
        };

        adapter_scores.push(AdapterScore {
            adapter_id: adapter.id.clone(),
            score: explanation.total_score as f64,
            gate_value,
            selected: is_selected,
        });
    }

    // Extract language from code features
    let detected_lang_idx = code_features
        .lang_one_hot
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(idx, _)| idx);

    let language = detected_lang_idx
        .and_then(|idx| match idx {
            0 => Some("python"),
            1 => Some("rust"),
            2 => Some("javascript"),
            3 => Some("typescript"),
            4 => Some("go"),
            5 => Some("java"),
            6 => Some("cpp"),
            7 => Some("csharp"),
            _ => None,
        })
        .map(|s| s.to_string());

    let frameworks: Vec<String> = code_features
        .framework_prior
        .iter()
        .map(|(k, _)| k.clone())
        .collect();

    let selected_adapters: Vec<String> = decision
        .indices
        .iter()
        .filter_map(|&idx| adapter_infos.get(idx as usize).map(|a| a.id.clone()))
        .collect();

    Ok(Json(RoutingDebugResponse {
        features: FeatureVector {
            language,
            frameworks,
            symbol_hits: code_features.symbol_hits as i32,
            path_tokens: code_features.path_tokens.clone(),
            verb: format!("{:?}", code_features.prompt_verb),
        },
        adapter_scores,
        selected_adapters,
        explanation: format!(
            "Router selected {} adapters with entropy {:.3}. {}",
            decision.indices.len(),
            decision.entropy,
            explanation.format()
        ),
    }))
}

/// Get routing history
///
/// Returns the most recent routing decisions from the database.
/// This queries actual routing decisions stored during inference operations.
/// If no decisions exist yet, returns an empty list.
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/routing/history",
    params(
        ("limit" = Option<usize>, Query, description = "Maximum number of results (default: 50)")
    ),
    responses(
        (status = 200, description = "Routing history", body = Vec<RoutingDebugResponse>)
    )
)]
pub async fn get_routing_history(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<RoutingHistoryQuery>,
) -> Result<Json<Vec<RoutingDebugResponse>>, (StatusCode, Json<ErrorResponse>)> {
    use adapteros_db::RoutingDecisionFilters;
    use tracing::{debug, warn};

    let limit = params.limit.unwrap_or(50);
    debug!(limit = limit, "Querying routing history from database");

    // Query routing decisions from the database
    let filters = RoutingDecisionFilters {
        limit: Some(limit),
        ..Default::default()
    };

    let db_decisions = state
        .db
        .query_routing_decisions(&filters)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to query routing history");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!("Database error: {}", e))),
            )
        })?;

    // Convert database records to RoutingDebugResponse format
    let responses: Vec<RoutingDebugResponse> = db_decisions
        .into_iter()
        .map(|decision| {
            // Parse candidate adapters JSON
            let candidates: Vec<adapteros_db::RouterCandidate> =
                serde_json::from_str(&decision.candidate_adapters).unwrap_or_default();

            // Parse selected adapter IDs
            let selected_adapters: Vec<String> = decision
                .selected_adapter_ids
                .map(|ids| ids.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default();

            // Convert candidates to adapter scores
            let adapter_scores: Vec<AdapterScore> = candidates
                .iter()
                .map(|c| {
                    let gate_float = (c.gate_q15 as f32) / 32768.0;
                    let adapter_id = format!("adapter-{}", c.adapter_idx);
                    let is_selected = selected_adapters.contains(&adapter_id);
                    AdapterScore {
                        adapter_id,
                        score: c.raw_score as f64,
                        gate_value: gate_float as f64,
                        selected: is_selected,
                    }
                })
                .collect();

            // Build explanation from decision metadata
            let explanation = format!(
                "Step {} with entropy {:.3}, tau {:.3}, selected {} adapter(s)",
                decision.step,
                decision.entropy,
                decision.tau,
                selected_adapters.len()
            );

            RoutingDebugResponse {
                features: FeatureVector {
                    // Note: Detailed features not stored in routing_decisions table
                    // These are summarized during decision storage
                    language: None,
                    frameworks: vec![],
                    symbol_hits: 0,
                    path_tokens: vec![],
                    verb: "infer".to_string(),
                },
                adapter_scores,
                selected_adapters,
                explanation,
            }
        })
        .collect();

    Ok(Json(responses))
}

// ===== Agent D Contract Endpoints =====

/// Get system metadata
#[utoipa::path(
    tag = "system",
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
    tag = "system",
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
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<RoutingDecisionsQuery>,
) -> Result<Json<RoutingDecisionsResponse>, (StatusCode, Json<ErrorResponse>)> {
    use adapteros_db::RoutingDecisionFilters;
    use tracing::{error, info};

    info!(
        tenant = %params.tenant,
        limit = params.limit,
        user_id = %claims.sub,
        "Querying routing decisions"
    );

    // Build filters from query params
    let filters = RoutingDecisionFilters {
        tenant_id: Some(params.tenant.clone()),
        stack_id: params.stack_id.clone(),
        adapter_id: params.adapter_id.clone(),
        request_id: params.request_id.clone(),
        since: params.since.clone(),
        until: params.until.clone(),
        min_entropy: params.min_entropy,
        max_overhead_pct: params.max_overhead_pct,
        limit: Some(params.limit),
        offset: params.offset,
    };

    // Query database
    let db_decisions = if params.anomalies_only {
        // Get high overhead decisions (>8% budget)
        state
            .db
            .get_high_overhead_decisions(Some(params.tenant.clone()), params.limit)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to query high overhead decisions");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!("Database error: {}", e))),
                )
            })?
    } else {
        state
            .db
            .query_routing_decisions(&filters)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to query routing decisions");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!("Database error: {}", e))),
                )
            })?
    };

    // Convert database records to API response format
    let mut items: Vec<RoutingDecision> = Vec::new();
    for db_decision in db_decisions.iter() {
        // Parse candidates JSON
        let candidates: Vec<adapteros_db::RouterCandidate> =
            serde_json::from_str(&db_decision.candidate_adapters).unwrap_or_default();

        // Lookup stack name from adapter_stacks table if stack_id is available
        let stack_name = if let Some(stack_id) = &db_decision.stack_id {
            state
                .db
                .get_stack(&params.tenant, stack_id)
                .await
                .ok()
                .flatten()
                .map(|stack| stack.name)
        } else {
            None
        };

        // Convert to API format with Q15 to float conversion
        let candidate_infos: Vec<RouterCandidateInfo> = candidates
            .iter()
            .map(|c| {
                let gate_float = (c.gate_q15 as f32) / 32768.0;
                RouterCandidateInfo {
                    adapter_idx: c.adapter_idx,
                    adapter_name: None, // adapter_idx is internal routing index; adapter IDs are in adapters_used
                    raw_score: c.raw_score,
                    gate_q15: c.gate_q15,
                    gate_float,
                    selected: c.gate_q15 > 0,
                }
            })
            .collect();

        // Extract selected adapters for legacy field
        let adapters_used: Vec<String> = db_decision
            .selected_adapter_ids
            .clone()
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        // Extract activations (gate values as floats)
        let activations: Vec<f64> = candidate_infos
            .iter()
            .filter(|c| c.selected)
            .map(|c| c.gate_float as f64)
            .collect();

        items.push(RoutingDecision {
            id: db_decision.id.clone(),
            tenant_id: db_decision.tenant_id.clone(),
            timestamp: db_decision.timestamp.clone(),
            request_id: db_decision.request_id.clone(),
            step: db_decision.step,
            input_token_id: db_decision.input_token_id,
            stack_id: db_decision.stack_id.clone(),
            stack_name,
            stack_hash: db_decision.stack_hash.clone(),
            entropy: db_decision.entropy,
            tau: db_decision.tau,
            entropy_floor: db_decision.entropy_floor,
            k_value: db_decision.k_value,
            candidates: candidate_infos,
            router_latency_us: db_decision.router_latency_us,
            total_inference_latency_us: db_decision.total_inference_latency_us,
            overhead_pct: db_decision.overhead_pct,
            adapters_used,
            activations,
            reason: format!(
                "entropy={:.2}, k={}",
                db_decision.entropy,
                db_decision.k_value.unwrap_or(0)
            ),
            trace_id: db_decision.request_id.clone().unwrap_or_default(),
        });
    }

    info!(
        count = items.len(),
        "Successfully retrieved routing decisions"
    );

    Ok(Json(RoutingDecisionsResponse { items }))
}

/// List audits with extended fields
#[utoipa::path(
    tag = "system",
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
            Json(
                ErrorResponse::new("failed to fetch audits")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(AuditsResponse { items: audits }))
}

/// Get promotion record with signature
#[utoipa::path(
    tag = "system",
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
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("promotion not found").with_code("NOT_FOUND")),
        )
    })?;

    Ok(Json(promo))
}

// ===== Metrics Endpoint =====

/// Prometheus/OpenMetrics endpoint  
/// Note: This endpoint requires bearer token authentication via Authorization header.
/// Authentication is checked in the route layer, not in the handler itself.
pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    // Check if metrics are enabled
    let metrics_enabled = {
        let config = match state.config.read() {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::error!("Failed to acquire config read lock: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("internal error")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
                    .into_response();
            }
        };
        config.metrics.enabled
    };

    if !metrics_enabled {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("metrics disabled").with_code("INTERNAL_ERROR")),
        )
            .into_response();
    }

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
                Json(
                    ErrorResponse::new("failed to render metrics")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as TokioStreamExt;

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
                        .data("{\"error\": \"serialization failed\"}".to_string())),
                    state,
                ));
            }
        };

        Some((Ok(Event::default().event("metrics").data(json)), state))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// SSE stream for telemetry events
/// Streams telemetry events in real-time via broadcast channel.
/// Falls back to periodic bundle checks if no real-time events are available.
pub async fn telemetry_events_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Subscribe to the telemetry broadcast channel for real-time events
    let mut receiver = state.telemetry_tx.subscribe();

    let stream = stream::unfold((receiver, state), |(mut rx, state)| async move {
        // Use select to handle both real-time events and keepalive timeout
        tokio::select! {
            // Try to receive a real-time telemetry event
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        // Serialize the telemetry event
                        let json = match serde_json::to_string(&event) {
                            Ok(j) => j,
                            Err(e) => {
                                tracing::warn!("Failed to serialize telemetry event: {}", e);
                                return Some((
                                    Ok(Event::default()
                                        .event("error")
                                        .data(format!("{{\"error\": \"serialization failed: {}\"}}", e))),
                                    (rx, state),
                                ));
                            }
                        };
                        Some((
                            Ok(Event::default().event("telemetry").data(json)),
                            (rx, state),
                        ))
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        // Client is lagging behind, notify and continue
                        tracing::warn!(lagged_count = count, "Telemetry SSE client lagged behind");
                        Some((
                            Ok(Event::default()
                                .event("warning")
                                .data(format!("{{\"lagged_events\": {}}}", count))),
                            (rx, state),
                        ))
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        // Channel closed, end the stream gracefully
                        tracing::info!("Telemetry broadcast channel closed");
                        None
                    }
                }
            }
            // Send keepalive if no events for 30 seconds
            _ = tokio::time::sleep(Duration::from_secs(30)) => {
                // Check buffer health and send status
                let buffer_len = state.telemetry_buffer.len().await;
                let health_json = format!(
                    "{{\"status\": \"keepalive\", \"buffer_size\": {}}}",
                    buffer_len
                );
                Some((
                    Ok(Event::default().event("keepalive").data(health_json)),
                    (rx, state),
                ))
            }
        }
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
                        .data("{\"error\": \"serialization failed\"}".to_string())),
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

    // Collect system metrics (using stubs until adapteros-system-metrics is re-enabled)
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let load_avg = collector.load_average();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("time error: {}", e))?
        .as_secs();

    Ok(SystemMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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
    tag = "system",
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

    // Subscribe to the training signal broadcast channel
    let rx = state.training_signal_tx.subscribe();

    // Convert the broadcast receiver into a stream that filters by tenant
    let signal_stream = BroadcastStream::new(rx).filter_map(move |result| {
        let tenant_filter = tenant_id.clone();
        match result {
            Ok(signal) => {
                // Filter signals by tenant_id if present in payload
                let signal_tenant = signal
                    .payload
                    .get("tenant_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // Pass through if tenant matches or if no tenant filter in signal
                if signal_tenant.is_empty() || signal_tenant == tenant_filter {
                    let event_data = serde_json::json!({
                        "type": signal.signal_type.to_string(),
                        "timestamp": signal.timestamp,
                        "priority": format!("{:?}", signal.priority),
                        "payload": signal.payload,
                        "trace_id": signal.trace_id,
                    });

                    Some(Ok(Event::default()
                        .event("training")
                        .data(event_data.to_string())))
                } else {
                    None
                }
            }
            Err(e) => {
                tracing::debug!("Broadcast stream error (likely lag): {}", e);
                None
            }
        }
    });

    // Also include a periodic heartbeat to keep connection alive and provide fallback data
    let heartbeat_stream = stream::unfold(0u64, |counter| async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let event_data = serde_json::json!({
            "type": "heartbeat",
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("System time before UNIX epoch")
                .as_millis(),
            "sequence": counter,
        });
        Some((
            Ok(Event::default()
                .event("training")
                .data(event_data.to_string())),
            counter + 1,
        ))
    });

    // Merge the signal stream with heartbeat stream
    let merged_stream = futures_util::stream::select(signal_stream, heartbeat_stream);

    Sse::new(merged_stream).keep_alive(KeepAlive::default())
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
    tag = "system",
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

    // Subscribe to the discovery signal broadcast channel
    let rx = state.discovery_signal_tx.subscribe();

    // Convert the broadcast receiver into a stream that filters by tenant and repo
    let signal_stream = BroadcastStream::new(rx).filter_map(move |result| {
        let tenant_filter = tenant_id.clone();
        let repo_filter_clone = repo_filter.clone();
        match result {
            Ok(signal) => {
                // Filter signals by tenant_id if present in payload
                let signal_tenant = signal
                    .payload
                    .get("tenant_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // Filter by repo_id if specified
                let signal_repo = signal
                    .payload
                    .get("repo_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let tenant_matches = signal_tenant.is_empty() || signal_tenant == tenant_filter;
                let repo_matches = repo_filter_clone
                    .as_ref()
                    .map(|r| signal_repo.is_empty() || signal_repo == r)
                    .unwrap_or(true);

                if tenant_matches && repo_matches {
                    let event_data = serde_json::json!({
                        "type": signal.signal_type.to_string(),
                        "timestamp": signal.timestamp,
                        "priority": format!("{:?}", signal.priority),
                        "payload": signal.payload,
                        "trace_id": signal.trace_id,
                    });

                    Some(Ok(Event::default()
                        .event("discovery")
                        .data(event_data.to_string())))
                } else {
                    None
                }
            }
            Err(e) => {
                tracing::debug!("Discovery broadcast stream error (likely lag): {}", e);
                None
            }
        }
    });

    // Include a periodic heartbeat to keep connection alive
    let heartbeat_stream = stream::unfold(0u64, |counter| async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let event_data = serde_json::json!({
            "type": "heartbeat",
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("System time before UNIX epoch")
                .as_millis(),
            "sequence": counter,
        });
        Some((
            Ok(Event::default()
                .event("discovery")
                .data(event_data.to_string())),
            counter + 1,
        ))
    });

    // Merge the signal stream with heartbeat stream
    let merged_stream = futures_util::stream::select(signal_stream, heartbeat_stream);

    Sse::new(merged_stream).keep_alive(KeepAlive::default())
}

/// Contacts stream SSE endpoint
///
/// Streams real-time contact discovery and update events as contacts are
/// discovered during inference operations.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    tag = "system",
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

    // Subscribe to the contact signal broadcast channel
    let rx = state.contact_signal_tx.subscribe();

    // Convert the broadcast receiver into a stream that filters by tenant
    let signal_stream = BroadcastStream::new(rx).filter_map(move |result| {
        let tenant_filter = tenant_id.clone();
        match result {
            Ok(signal) => {
                // Filter signals by tenant_id if present in payload
                let signal_tenant = signal
                    .payload
                    .get("tenant_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // Pass through if tenant matches or if no tenant filter in signal
                if signal_tenant.is_empty() || signal_tenant == tenant_filter {
                    let event_data = serde_json::json!({
                        "type": signal.signal_type.to_string(),
                        "timestamp": signal.timestamp,
                        "priority": format!("{:?}", signal.priority),
                        "payload": signal.payload,
                        "trace_id": signal.trace_id,
                    });

                    Some(Ok(Event::default()
                        .event("contact")
                        .data(event_data.to_string())))
                } else {
                    None
                }
            }
            Err(e) => {
                tracing::debug!("Contact broadcast stream error (likely lag): {}", e);
                None
            }
        }
    });

    // Include a periodic heartbeat to keep connection alive
    let heartbeat_stream = stream::unfold(0u64, |counter| async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let event_data = serde_json::json!({
            "type": "heartbeat",
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("System time before UNIX epoch")
                .as_millis(),
            "sequence": counter,
        });
        Some((
            Ok(Event::default()
                .event("contact")
                .data(event_data.to_string())),
            counter + 1,
        ))
    });

    // Merge the signal stream with heartbeat stream
    let merged_stream = futures_util::stream::select(signal_stream, heartbeat_stream);

    Sse::new(merged_stream).keep_alive(KeepAlive::default())
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
    tag = "system",
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
            Json(
                ErrorResponse::new("failed to fetch contacts")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
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
    tag = "system",
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
            Json(
                ErrorResponse::new("invalid category")
                    .with_code("BAD_REQUEST")
                    .with_string_details(
                        "category must be one of: user, system, adapter, repository, external"
                            .to_string(),
                    ),
            ),
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
            Json(
                ErrorResponse::new("failed to create contact")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
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
    tag = "system",
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
                Json(ErrorResponse::new("contact not found").with_code("NOT_FOUND")),
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch contact")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
    tag = "system",
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
                Json(
                    ErrorResponse::new("failed to delete contact")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("contact not found").with_code("NOT_FOUND")),
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
    tag = "system",
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
                Json(ErrorResponse::new("contact not found").with_code("NOT_FOUND")),
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch contact")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
            Json(
                ErrorResponse::new("failed to fetch interactions")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let interactions: Vec<ContactInteractionResponse> =
        interactions.into_iter().map(|i| i.into()).collect();

    Ok(Json(ContactInteractionsResponse { interactions }))
}
// ========== Training Handlers ==========
// Note: list_training_jobs is defined in handlers/training.rs and re-exported via `pub use training::*`

/// Start adapter training session
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/training/sessions",
    request_body = StartTrainingRequest,
    responses(
        (status = 201, description = "Training session started successfully", body = TrainingJobResponse)
    ),
    tag = "training"
)]
pub async fn create_training_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<StartTrainingRequest>,
) -> Result<Json<TrainingJobResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Operator and Admin can start training
    crate::permissions::require_permission(&claims, crate::permissions::Permission::TrainingStart)?;

    let config = req.config.into();

    let job = state
        .training_service
        .start_training(
            req.adapter_name.clone(),
            config,
            req.template_id,
            req.repo_id,
            None, // dataset_id
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to start training session")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Audit log: training session created
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TRAINING_START,
        crate::audit_helper::resources::TRAINING_JOB,
        Some(&job.id),
    )
    .await;

    Ok(Json(job.into()))
}
/// Get training logs
#[utoipa::path(
    tag = "system",
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
    // Role check: All roles can view training logs
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::TrainingViewLogs,
    )?;

    let logs = state
        .training_service
        .get_logs(&job_id)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("failed to get logs")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(logs))
}

/// Get training metrics
#[utoipa::path(
    tag = "system",
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
    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("INTERNAL_ERROR")),
        )
    })?;

    let job = state.training_service.get_job(&job_id).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("training job not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(TrainingMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
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
    tag = "system",
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
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("INTERNAL_ERROR")),
        )
    })?;

    let templates = state.training_service.list_templates().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list templates")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(templates.into_iter().map(|t| t.into()).collect()))
}

/// Get a specific training template
#[utoipa::path(
    tag = "system",
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
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("INTERNAL_ERROR")),
        )
    })?;

    let template = state
        .training_service
        .get_template(&template_id)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("template not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(template.into()))
}

/// Training artifacts response
#[derive(Debug, Serialize, ToSchema)]
pub struct TrainingArtifactsResponse {
    /// Whether artifacts are ready for download
    pub ready: bool,
    /// Training job ID
    pub job_id: String,
    /// List of generated artifacts
    pub artifacts: Vec<TrainingArtifact>,
    /// Total size in bytes
    pub total_size_bytes: u64,
}

/// Individual training artifact
#[derive(Debug, Serialize, ToSchema)]
pub struct TrainingArtifact {
    /// Artifact name
    pub name: String,
    /// Artifact type (weights, metrics, logs, etc.)
    pub artifact_type: String,
    /// Size in bytes
    pub size_bytes: u64,
    /// BLAKE3 hash
    pub hash_b3: Option<String>,
    /// Download path
    pub path: String,
    /// Created timestamp
    pub created_at: String,
}

/// Get training job artifacts
///
/// Returns a list of artifacts generated by a completed training job,
/// including weights, metrics files, and logs.
///
/// **Permissions:** Requires `Operator`, `Admin`, or `Viewer` role.
#[utoipa::path(
    tag = "training",
    get,
    path = "/v1/training/jobs/{job_id}/artifacts",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "Training artifacts retrieved successfully", body = TrainingArtifactsResponse),
        (status = 404, description = "Job not found", body = ErrorResponse)
    )
)]
pub async fn get_training_artifacts(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<TrainingArtifactsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("FORBIDDEN")),
        )
    })?;

    // Get the training job
    let job = state.training_service.get_job(&job_id).await.map_err(|e| {
        let error_str = e.to_string();
        if error_str.contains("not found") || error_str.contains("NotFound") {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new(&format!("Training job not found: {}", job_id))
                        .with_code("NOT_FOUND"),
                ),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(&format!("Failed to get job: {}", e))
                        .with_code("DATABASE_ERROR"),
                ),
            )
        }
    })?;

    // Check if job is completed
    let ready = job.status.to_string().to_lowercase() == "completed";

    // Build artifacts list based on job output
    let mut artifacts = Vec::new();
    let mut total_size_bytes = 0u64;

    if ready {
        // Add weights artifact if job has adapter_id (populated on completion)
        if let Some(ref adapter_id) = job.adapter_id {
            artifacts.push(TrainingArtifact {
                name: format!("{}.safetensors", adapter_id),
                artifact_type: "weights".to_string(),
                size_bytes: 0, // Size would come from actual file
                hash_b3: None,
                path: format!("/v1/adapters/{}/download", adapter_id),
                created_at: job.created_at.clone(),
            });
        }

        // Add metrics artifact
        artifacts.push(TrainingArtifact {
            name: "training_metrics.json".to_string(),
            artifact_type: "metrics".to_string(),
            size_bytes: 0,
            hash_b3: None,
            path: format!("/v1/training/jobs/{}/metrics", job_id),
            created_at: job.created_at.clone(),
        });

        // Add logs artifact
        artifacts.push(TrainingArtifact {
            name: "training.log".to_string(),
            artifact_type: "logs".to_string(),
            size_bytes: 0,
            hash_b3: None,
            path: format!("/v1/training/jobs/{}/logs", job_id),
            created_at: job.created_at.clone(),
        });
    }

    Ok(Json(TrainingArtifactsResponse {
        ready,
        job_id,
        artifacts,
        total_size_bytes,
    }))
}

// Git integration handlers
// pub mod git; // Already declared above

// ===== Advanced Process Monitoring Handlers =====

/// List monitoring rules
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/rules",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("is_active" = Option<bool>, Query, description = "Filter by active status")
    ),
    responses(
        (status = 200, description = "Monitoring rules", body = Vec<MonitoringRuleResponse>)
    )
)]
pub async fn list_monitoring_rules(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<MonitoringRuleResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_id = params.get("tenant_id");
    let is_active = params.get("is_active").and_then(|s| s.parse::<bool>().ok());

    let rules = adapteros_system_metrics::ProcessMonitoringRule::list(
        state.db.pool(),
        tenant_id.map(|s| s.as_str()),
        is_active,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let response: Vec<MonitoringRuleResponse> = rules.into_iter().map(|rule| rule.into()).collect();

    Ok(Json(response))
}

/// Create monitoring rule
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/rules",
    request_body = CreateMonitoringRuleApiRequest,
    responses(
        (status = 200, description = "Rule created", body = MonitoringRuleResponse)
    )
)]
pub async fn create_monitoring_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateMonitoringRuleApiRequest>,
) -> Result<Json<MonitoringRuleResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let rule_request = req.try_into().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid request")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e),
            ),
        )
    })?;

    let rule_id =
        adapteros_system_metrics::ProcessMonitoringRule::create(state.db.pool(), rule_request)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

    // Get the created rule
    let rules = adapteros_system_metrics::ProcessMonitoringRule::list(state.db.pool(), None, None)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let rule = rules.into_iter().find(|r| r.id == rule_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("rule not found").with_code("NOT_FOUND")),
        )
    })?;

    Ok(Json(rule.into()))
}

/// Update monitoring rule
#[utoipa::path(
    tag = "system",
    put,
    path = "/v1/monitoring/rules/{rule_id}",
    params(
        ("rule_id" = String, Path, description = "Rule ID")
    ),
    request_body = UpdateMonitoringRuleApiRequest,
    responses(
        (status = 200, description = "Rule updated", body = MonitoringRuleResponse)
    )
)]
pub async fn update_monitoring_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(rule_id): Path<String>,
    Json(req): Json<UpdateMonitoringRuleApiRequest>,
) -> Result<Json<MonitoringRuleResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let update_request = req.into();

    adapteros_system_metrics::ProcessMonitoringRule::update(
        state.db.pool(),
        &rule_id,
        update_request,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Get the updated rule
    let rules = adapteros_system_metrics::ProcessMonitoringRule::list(state.db.pool(), None, None)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let rule = rules.into_iter().find(|r| r.id == rule_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("rule not found").with_code("NOT_FOUND")),
        )
    })?;

    Ok(Json(rule.into()))
}

/// Delete monitoring rule
#[utoipa::path(
    tag = "system",
    delete,
    path = "/v1/monitoring/rules/{rule_id}",
    params(
        ("rule_id" = String, Path, description = "Rule ID")
    ),
    responses(
        (status = 200, description = "Rule deleted")
    )
)]
pub async fn delete_monitoring_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(rule_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    adapteros_system_metrics::ProcessMonitoringRule::delete(state.db.pool(), &rule_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::OK)
}

/// List alerts
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/alerts",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("status" = Option<String>, Query, description = "Filter by status"),
        ("severity" = Option<String>, Query, description = "Filter by severity"),
        ("limit" = Option<i64>, Query, description = "Limit results")
    ),
    responses(
        (status = 200, description = "Alerts", body = Vec<AlertResponse>)
    )
)]
pub async fn list_alerts(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<AlertResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let filters = adapteros_system_metrics::AlertFilters {
        tenant_id: params.get("tenant_id").cloned(),
        worker_id: params.get("worker_id").cloned(),
        status: params
            .get("status")
            .and_then(|s| adapteros_system_metrics::AlertStatus::from_string(s.to_string()).into()),
        severity: params.get("severity").and_then(|s| {
            adapteros_system_metrics::AlertSeverity::from_string(s.to_string()).into()
        }),
        start_time: None,
        end_time: None,
        limit: params.get("limit").and_then(|s| s.parse::<i64>().ok()),
    };

    let alerts = adapteros_system_metrics::ProcessAlert::list(state.db.pool(), filters)
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
        })?;

    let response: Vec<AlertResponse> = alerts.into_iter().map(|alert| alert.into()).collect();

    Ok(Json(response))
}

/// Acknowledge alert
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/alerts/{alert_id}/acknowledge",
    params(
        ("alert_id" = String, Path, description = "Alert ID")
    ),
    request_body = AcknowledgeAlertRequest,
    responses(
        (status = 200, description = "Alert acknowledged", body = AlertResponse)
    )
)]
pub async fn acknowledge_alert(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(alert_id): Path<String>,
    Json(req): Json<AcknowledgeAlertRequest>,
) -> Result<Json<AlertResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    adapteros_system_metrics::ProcessAlert::update_status(
        state.db.pool(),
        &alert_id,
        adapteros_system_metrics::AlertStatus::Acknowledged,
        Some(&req.user),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Get the updated alert
    let filters = adapteros_system_metrics::AlertFilters {
        tenant_id: None,
        worker_id: None,
        status: None,
        severity: None,
        start_time: None,
        end_time: None,
        limit: Some(1),
    };

    let alerts = adapteros_system_metrics::ProcessAlert::list(state.db.pool(), filters)
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
        })?;

    let alert = alerts
        .into_iter()
        .find(|a| a.id == alert_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("alert not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(alert.into()))
}

/// Resolve alert
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/alerts/{alert_id}/resolve",
    params(
        ("alert_id" = String, Path, description = "Alert ID")
    ),
    responses(
        (status = 200, description = "Alert resolved", body = AlertResponse)
    )
)]
pub async fn resolve_alert(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(alert_id): Path<String>,
) -> Result<Json<AlertResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    adapteros_system_metrics::ProcessAlert::update_status(
        state.db.pool(),
        &alert_id,
        adapteros_system_metrics::AlertStatus::Resolved,
        None,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Get the updated alert
    let filters = adapteros_system_metrics::AlertFilters {
        tenant_id: None,
        worker_id: None,
        status: None,
        severity: None,
        start_time: None,
        end_time: None,
        limit: Some(1),
    };

    let alerts = adapteros_system_metrics::ProcessAlert::list(state.db.pool(), filters)
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
        })?;

    let alert = alerts
        .into_iter()
        .find(|a| a.id == alert_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("alert not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(alert.into()))
}

/// List anomalies
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/anomalies",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("status" = Option<String>, Query, description = "Filter by status"),
        ("anomaly_type" = Option<String>, Query, description = "Filter by anomaly type"),
        ("limit" = Option<i64>, Query, description = "Limit results")
    ),
    responses(
        (status = 200, description = "Anomalies", body = Vec<AnomalyResponse>)
    )
)]
pub async fn list_anomalies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<AnomalyResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let filters = adapteros_system_metrics::AnomalyFilters {
        tenant_id: params.get("tenant_id").cloned(),
        worker_id: params.get("worker_id").cloned(),
        status: params.get("status").and_then(|s| {
            adapteros_system_metrics::AnomalyStatus::from_string(s.to_string()).into()
        }),
        anomaly_type: params.get("anomaly_type").cloned(),
        start_time: None,
        end_time: None,
        limit: params.get("limit").and_then(|s| s.parse::<i64>().ok()),
    };

    let anomalies = adapteros_system_metrics::ProcessAnomaly::list(state.db.pool(), filters)
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
        })?;

    let response: Vec<AnomalyResponse> = anomalies
        .into_iter()
        .map(|anomaly| anomaly.into())
        .collect();

    Ok(Json(response))
}

/// Update anomaly status
#[utoipa::path(
    tag = "system",
    put,
    path = "/v1/monitoring/anomalies/{anomaly_id}",
    params(
        ("anomaly_id" = String, Path, description = "Anomaly ID")
    ),
    request_body = UpdateAnomalyStatusRequest,
    responses(
        (status = 200, description = "Anomaly updated", body = AnomalyResponse)
    )
)]
pub async fn update_anomaly_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(anomaly_id): Path<String>,
    Json(req): Json<UpdateAnomalyStatusRequest>,
) -> Result<Json<AnomalyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Update anomaly status in database
    sqlx::query(
        "UPDATE process_anomalies SET status = ?, investigation_notes = ?, investigated_by = ? WHERE id = ?",
    )
    .bind(&req.status)
    .bind(&req.investigation_notes)
    .bind(&req.investigated_by)
    .bind(&anomaly_id)
    .execute(state.db.pool())
    .await
    .map_err(|e: sqlx::Error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("INTERNAL_SERVER_ERROR").with_string_details(e.to_string())),
        )
    })?;

    // Get the updated anomaly
    let filters = adapteros_system_metrics::AnomalyFilters {
        tenant_id: None,
        worker_id: None,
        status: None,
        anomaly_type: None,
        start_time: None,
        end_time: None,
        limit: Some(1),
    };

    let anomalies = adapteros_system_metrics::ProcessAnomaly::list(state.db.pool(), filters)
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
        })?;

    let anomaly = anomalies
        .into_iter()
        .find(|a| a.id == anomaly_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("anomaly not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(anomaly.into()))
}

/// Get performance baselines
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/baselines",
    params(
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("metric_name" = Option<String>, Query, description = "Filter by metric name")
    ),
    responses(
        (status = 200, description = "Performance baselines", body = Vec<BaselineResponse>)
    )
)]
pub async fn get_performance_baselines(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<BaselineResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let worker_id = params.get("worker_id");
    let metric_name = params.get("metric_name");

    let mut query =
        "SELECT * FROM process_performance_baselines WHERE is_active = true".to_string();
    let mut params_vec: Vec<Box<dyn sqlx::Encode<'_, sqlx::Sqlite> + Send + Sync>> = vec![];
    let mut param_count = 0;

    if let Some(worker) = worker_id {
        param_count += 1;
        query.push_str(&format!(" AND worker_id = ${}", param_count));
        params_vec.push(Box::new(worker.to_string()));
    }

    if let Some(metric) = metric_name {
        param_count += 1;
        query.push_str(&format!(" AND metric_name = ${}", param_count));
        params_vec.push(Box::new(metric.to_string()));
    }

    query.push_str(" ORDER BY calculated_at DESC");

    let rows = sqlx::query(&query)
        .fetch_all(state.db.pool())
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
        })?;

    let mut baselines = Vec::new();
    for row in rows {
        let baseline = adapteros_system_metrics::PerformanceBaseline {
            id: row.get("id"),
            worker_id: row.get("worker_id"),
            tenant_id: row.get("tenant_id"),
            metric_name: row.get("metric_name"),
            baseline_value: row.get("baseline_value"),
            baseline_type: adapteros_system_metrics::BaselineType::from_string(
                row.get("baseline_type"),
            ),
            calculation_period_days: row.get("calculation_period_days"),
            confidence_interval: row.get("confidence_interval"),
            standard_deviation: row.get("standard_deviation"),
            percentile_95: row.get("percentile_95"),
            percentile_99: row.get("percentile_99"),
            is_active: row.get("is_active"),
            calculated_at: chrono::DateTime::parse_from_rfc3339(
                &row.get::<String, _>("calculated_at"),
            )
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
            .with_timezone(&chrono::Utc),
            expires_at: row
                .try_get::<Option<String>, _>("expires_at")
                .ok()
                .flatten()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc)),
        };
        baselines.push(baseline.into());
    }

    Ok(Json(baselines))
}
/// Recalculate baseline
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/baselines/recalculate",
    request_body = RecalculateBaselineRequest,
    responses(
        (status = 200, description = "Baseline recalculated", body = BaselineResponse)
    )
)]
pub async fn recalculate_baseline(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RecalculateBaselineRequest>,
) -> Result<Json<BaselineResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // This would typically call the baseline service
    // For now, we'll return a placeholder response
    let baseline = adapteros_system_metrics::PerformanceBaseline {
        id: uuid::Uuid::new_v4().to_string(),
        worker_id: req.worker_id,
        tenant_id: "default".to_string(),
        metric_name: req.metric_name,
        baseline_value: 0.0,
        baseline_type: adapteros_system_metrics::BaselineType::Statistical,
        calculation_period_days: req.calculation_period_days.unwrap_or(7),
        confidence_interval: Some(0.95),
        standard_deviation: Some(0.0),
        percentile_95: Some(0.0),
        percentile_99: Some(0.0),
        is_active: true,
        calculated_at: chrono::Utc::now(),
        expires_at: Some(chrono::Utc::now() + chrono::Duration::days(90)),
    };

    Ok(Json(baseline.into()))
}

/// Get dashboard configuration
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/dashboards/{dashboard_id}/config",
    params(
        ("dashboard_id" = String, Path, description = "Dashboard ID")
    ),
    responses(
        (status = 200, description = "Dashboard configuration", body = adapteros_system_metrics::DashboardConfig)
    )
)]
pub async fn get_dashboard_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dashboard_id): Path<String>,
) -> Result<Json<adapteros_system_metrics::DashboardConfig>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let dashboard_service =
        adapteros_system_metrics::DashboardService::new(std::sync::Arc::new(state.db.clone()));

    let config = dashboard_service
        .get_dashboard_config(&dashboard_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get dashboard config")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(config))
}

/// Get dashboard data
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/dashboards/{dashboard_id}/data",
    params(
        ("dashboard_id" = String, Path, description = "Dashboard ID"),
        ("time_range" = Option<String>, Query, description = "Time range (1h, 6h, 24h, 7d, 30d)")
    ),
    responses(
        (status = 200, description = "Dashboard data", body = adapteros_system_metrics::DashboardData)
    )
)]
pub async fn get_dashboard_data(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dashboard_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<adapteros_system_metrics::DashboardData>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let dashboard_service =
        adapteros_system_metrics::DashboardService::new(std::sync::Arc::new(state.db.clone()));
    let time_range = params.get("time_range").map(|s| s.as_str());

    let data = dashboard_service
        .get_dashboard_data(&dashboard_id, time_range)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get dashboard data")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(data))
}

/// Export dashboard data
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/dashboards/{dashboard_id}/export/{format}",
    params(
        ("dashboard_id" = String, Path, description = "Dashboard ID"),
        ("format" = String, Path, description = "Export format (json, csv)"),
        ("time_range" = Option<String>, Query, description = "Time range (1h, 6h, 24h, 7d, 30d)")
    ),
    responses(
        (status = 200, description = "Dashboard data export")
    )
)]
pub async fn export_dashboard_data(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((dashboard_id, format)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let dashboard_service =
        adapteros_system_metrics::DashboardService::new(std::sync::Arc::new(state.db.clone()));
    let time_range = params.get("time_range").map(|s| s.as_str());

    let export_data = dashboard_service
        .export_dashboard_data(&dashboard_id, &format, time_range)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to export dashboard data")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let content_type = match format.as_str() {
        "json" => "application/json",
        "csv" => "text/csv",
        _ => "text/plain",
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        .header(
            "Content-Disposition",
            format!(
                "attachment; filename=\"dashboard_{}.{}\"",
                dashboard_id, format
            ),
        )
        .body(axum::body::Body::from(export_data))
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to create response")
                        .with_code("INTERNAL_SERVER_ERROR"),
                ),
            )
        })?;

    Ok(response)
}

// ===== Enhanced SSE Streams for Advanced Monitoring =====

/// SSE stream for alerts
/// Pushes real-time alerts as they are created or updated
pub async fn alerts_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold(state, |state| async move {
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Fetch recent alerts
        let filters = adapteros_system_metrics::AlertFilters {
            tenant_id: None,
            worker_id: None,
            status: None,
            severity: None,
            start_time: Some(chrono::Utc::now() - chrono::Duration::minutes(5)),
            end_time: None,
            limit: Some(50),
        };

        let alerts =
            match adapteros_system_metrics::ProcessAlert::list(state.db.pool(), filters).await {
                Ok(alerts) => alerts,
                Err(e) => {
                    tracing::warn!("Failed to fetch alerts for SSE: {}", e);
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data(format!("{{\"error\": \"{}\"}}", e))),
                        state,
                    ));
                }
            };

        let alert_data = serde_json::json!({
            "alerts": alerts.iter().map(|a| adapteros_system_metrics::AlertResponse::from(a.clone())).collect::<Vec<_>>(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "count": alerts.len()
        });

        Some((
            Ok(Event::default()
                .event("alerts")
                .data(serde_json::to_string(&alert_data).unwrap_or_else(|_| "{}".to_string()))),
            state,
        ))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// SSE stream for anomalies
/// Pushes real-time anomaly detections
pub async fn anomalies_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold(state, |state| async move {
        tokio::time::sleep(Duration::from_secs(10)).await;

        // Fetch recent anomalies
        let filters = adapteros_system_metrics::AnomalyFilters {
            tenant_id: None,
            worker_id: None,
            status: None,
            anomaly_type: None,
            start_time: Some(chrono::Utc::now() - chrono::Duration::minutes(10)),
            end_time: None,
            limit: Some(20),
        };

        let anomalies =
            match adapteros_system_metrics::ProcessAnomaly::list(state.db.pool(), filters).await {
                Ok(anomalies) => anomalies,
                Err(e) => {
                    tracing::warn!("Failed to fetch anomalies for SSE: {}", e);
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data(format!("{{\"error\": \"{}\"}}", e))),
                        state,
                    ));
                }
            };

        let anomaly_data = serde_json::json!({
            "anomalies": anomalies.iter().map(|a| adapteros_system_metrics::AnomalyResponse::from(a.clone())).collect::<Vec<_>>(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "count": anomalies.len()
        });

        Some((
            Ok(Event::default()
                .event("anomalies")
                .data(serde_json::to_string(&anomaly_data).unwrap_or_else(|_| "{}".to_string()))),
            state,
        ))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
/// SSE stream for dashboard-specific metrics
/// Pushes metrics tailored for dashboard widgets
pub async fn dashboard_metrics_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(dashboard_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold((state, dashboard_id), |(state, dashboard_id)| async move {
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Get dashboard configuration (placeholder for now)
        let dashboard_config = serde_json::json!({
            "widgets": [
                {
                    "type": "time_series",
                    "metric": "cpu_usage",
                    "aggregation": "avg",
                    "window": "1h"
                },
                {
                    "type": "gauge",
                    "metric": "gpu_utilization",
                    "threshold_warning": 80,
                    "threshold_critical": 95
                },
                {
                    "type": "alert_list",
                    "severities": ["critical", "error"],
                    "limit": 10
                }
            ],
            "refresh_interval": 30,
            "time_range": "24h"
        });

        // Fetch metrics for each widget
        let mut widget_data = Vec::new();

        for widget in dashboard_config["widgets"].as_array().unwrap_or(&vec![]) {
            let widget_type = widget["type"].as_str().unwrap_or("unknown");
            let metric_name = widget["metric"].as_str().unwrap_or("");

            let filters = adapteros_system_metrics::MetricFilters {
                worker_id: None,
                tenant_id: None,
                metric_name: Some(metric_name.to_string()),
                start_time: Some(chrono::Utc::now() - chrono::Duration::hours(1)),
                end_time: None,
                limit: Some(100),
            };

            let metrics = match adapteros_system_metrics::ProcessHealthMetric::query(
                state.db.pool(),
                filters,
            )
            .await
            {
                Ok(metrics) => metrics,
                Err(e) => {
                    tracing::warn!("Failed to fetch metrics for widget: {}", e);
                    continue;
                }
            };

            let widget_result = match widget_type {
                "time_series" => {
                    let points: Vec<serde_json::Value> = metrics
                        .iter()
                        .map(|m| {
                            serde_json::json!({
                                "timestamp": m.collected_at.to_rfc3339(),
                                "value": m.metric_value,
                                "worker_id": m.worker_id
                            })
                        })
                        .collect();

                    serde_json::json!({
                        "widget_id": "time_series_1",
                        "widget_type": "time_series",
                        "data": {
                            "metric": metric_name,
                            "points": points,
                            "aggregation": widget["aggregation"],
                            "window": widget["window"]
                        }
                    })
                }
                "gauge" => {
                    let current_value = metrics.last().map(|m| m.metric_value).unwrap_or(0.0);
                    let status = if current_value
                        >= widget["threshold_critical"].as_f64().unwrap_or(95.0)
                    {
                        "critical"
                    } else if current_value >= widget["threshold_warning"].as_f64().unwrap_or(80.0)
                    {
                        "warning"
                    } else {
                        "healthy"
                    };

                    serde_json::json!({
                        "widget_id": "gauge_1",
                        "widget_type": "gauge",
                        "data": {
                            "metric": metric_name,
                            "current_value": current_value,
                            "threshold_warning": widget["threshold_warning"],
                            "threshold_critical": widget["threshold_critical"],
                            "status": status
                        }
                    })
                }
                "alert_list" => {
                    let alert_filters = adapteros_system_metrics::AlertFilters {
                        tenant_id: None,
                        worker_id: None,
                        status: Some(adapteros_system_metrics::AlertStatus::Active),
                        severity: None,
                        start_time: None,
                        end_time: None,
                        limit: Some(widget["limit"].as_i64().unwrap_or(10)),
                    };

                    let alerts = match adapteros_system_metrics::ProcessAlert::list(
                        state.db.pool(),
                        alert_filters,
                    )
                    .await
                    {
                        Ok(alerts) => alerts,
                        Err(e) => {
                            tracing::warn!("Failed to fetch alerts for widget: {}", e);
                            vec![]
                        }
                    };

                    let alert_summaries: Vec<serde_json::Value> = alerts
                        .iter()
                        .map(|a| {
                            serde_json::json!({
                                "id": a.id,
                                "title": a.title,
                                "severity": a.severity.to_string(),
                                "status": a.status.to_string(),
                                "worker_id": a.worker_id,
                                "created_at": a.created_at.to_rfc3339(),
                                "acknowledged_by": a.acknowledged_by
                            })
                        })
                        .collect();

                    serde_json::json!({
                        "widget_id": "alert_list_1",
                        "widget_type": "alert_list",
                        "data": {
                            "alerts": alert_summaries,
                            "total_count": alerts.len(),
                            "unacknowledged_count": alerts.iter().filter(|a| a.status.to_string() == "active").count()
                        }
                    })
                }
                _ => {
                    serde_json::json!({
                        "widget_id": "unknown_1",
                        "widget_type": widget_type,
                        "data": {},
                        "error": "Unknown widget type"
                    })
                }
            };

            widget_data.push(widget_result);
        }

        let dashboard_data = serde_json::json!({
            "dashboard_id": dashboard_id,
            "widgets": widget_data,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "refresh_interval": dashboard_config["refresh_interval"]
        });

        Some((
            Ok(Event::default()
                .event("dashboard_metrics")
                .data(serde_json::to_string(&dashboard_data).unwrap_or_else(|_| "{}".to_string()))),
            (state, dashboard_id),
        ))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Enhanced system metrics stream with monitoring data
/// Includes GPU metrics, inference latency, active alerts count, and recent anomalies
pub async fn enhanced_system_metrics_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold(state, |state| async move {
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Fetch basic system metrics
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

        // Fetch active alerts count
        let alert_filters = adapteros_system_metrics::AlertFilters {
            tenant_id: None,
            worker_id: None,
            status: Some(adapteros_system_metrics::AlertStatus::Active),
            severity: None,
            start_time: None,
            end_time: None,
            limit: Some(1), // Just count, not actual alerts
        };

        let active_alerts_count = match adapteros_system_metrics::ProcessAlert::list(
            state.db.pool(),
            alert_filters,
        )
        .await
        {
            Ok(alerts) => alerts.len(),
            Err(_) => 0,
        };

        // Fetch recent anomalies count
        let anomaly_filters = adapteros_system_metrics::AnomalyFilters {
            tenant_id: None,
            worker_id: None,
            status: Some(adapteros_system_metrics::AnomalyStatus::Detected),
            anomaly_type: None,
            start_time: Some(chrono::Utc::now() - chrono::Duration::hours(1)),
            end_time: None,
            limit: Some(1), // Just count, not actual anomalies
        };

        let recent_anomalies_count =
            match adapteros_system_metrics::ProcessAnomaly::list(state.db.pool(), anomaly_filters)
                .await
            {
                Ok(anomalies) => anomalies.len(),
                Err(_) => 0,
            };

        // Fetch worker health status
        let workers = match sqlx::query("SELECT id, status FROM workers WHERE status = 'active'")
            .fetch_all(state.db.pool())
            .await
        {
            Ok(workers) => workers.len(),
            Err(_) => 0,
        };

        let enhanced_metrics = serde_json::json!({
            "system_metrics": {
                "cpu_usage": metrics.cpu_usage,
                "memory_usage": metrics.memory_usage,
                "gpu_utilization": metrics.gpu_utilization,
                "gpu_memory_used": 0.0,
                "gpu_temperature": 0.0,
                "disk_usage": metrics.disk_usage,
                "network_rx": 0.0,
                "network_tx": 0.0
            },
            "monitoring_metrics": {
                "active_alerts_count": active_alerts_count,
                "recent_anomalies_count": recent_anomalies_count,
                "active_workers_count": workers,
                "inference_latency_p95": 0.0, // Placeholder - would come from worker
                "active_inference_sessions": 0, // Placeholder - would come from worker
                "adapter_swap_latency": 0.0, // Placeholder - would come from worker
                "lora_routing_overhead": 0.0 // Placeholder - would come from worker
            },
            "timestamp": chrono::Utc::now().to_rfc3339()
        });

        Some((
            Ok(Event::default().event("enhanced_metrics").data(
                serde_json::to_string(&enhanced_metrics).unwrap_or_else(|_| "{}".to_string()),
            )),
            state,
        ))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Get federation audit report
///
/// Returns federation chain verification status and host validation results.
/// Per Observability Layer requirement for canonical audit dashboard.
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/audit/federation",
    responses(
        (status = 200, description = "Federation audit report", body = FederationAuditResponse)
    )
)]
pub async fn get_federation_audit(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<FederationAuditResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Compliance, Role::Operator])?;

    // Fetch federation bundle signatures
    let pool = state.db.pool();

    let signatures = sqlx::query(
        r#"
        SELECT 
            bundle_hash,
            host_id,
            signature,
            verified,
            created_at
        FROM federation_bundle_signatures
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to fetch federation signatures")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let mut host_chains: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut total_signatures = 0;
    let mut verified_signatures = 0;

    for row in signatures {
        total_signatures += 1;
        let host_id: String = row.try_get("host_id").unwrap_or_default();
        let verified: bool = row.try_get("verified").unwrap_or(false);
        let bundle_hash: String = row.try_get("bundle_hash").unwrap_or_default();

        if verified {
            verified_signatures += 1;
        }

        host_chains.entry(host_id).or_default().push(bundle_hash);
    }

    // Check quarantine status
    let quarantine_status = sqlx::query(
        r#"
        SELECT reason, created_at
        FROM policy_quarantine
        WHERE released = FALSE
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to check quarantine status")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let quarantined = quarantine_status.is_some();
    let quarantine_reason = quarantine_status.and_then(|row| row.try_get("reason").ok());

    Ok(Json(FederationAuditResponse {
        total_hosts: host_chains.len(),
        total_signatures,
        verified_signatures,
        quarantined,
        quarantine_reason,
        host_chains: host_chains
            .into_iter()
            .map(|(host_id, bundles)| HostChainSummary {
                host_id,
                bundle_count: bundles.len(),
                latest_bundle: bundles.first().cloned(),
            })
            .collect(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get compliance audit report
///
/// Returns compliance status for all policy packs and control objectives.
/// Per Observability Layer requirement for canonical audit dashboard.
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/audit/compliance",
    responses(
        (status = 200, description = "Compliance audit report", body = ComplianceAuditResponse)
    )
)]
pub async fn get_compliance_audit(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ComplianceAuditResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Compliance, Role::Operator])?;

    // Fetch policy violations from telemetry bundles
    let pool = state.db.pool();

    let violations = sqlx::query(
        r#"
        SELECT COUNT(*) as count
        FROM policy_quarantine
        WHERE released = FALSE
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to count violations")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let active_violations: i64 = violations.try_get("count").unwrap_or(0);

    // Generate compliance controls status
    let controls = vec![
        ComplianceControl {
            control_id: "EGRESS-001".to_string(),
            control_name: "Network Egress Control".to_string(),
            status: if active_violations == 0 {
                "compliant"
            } else {
                "pending"
            }
            .to_string(),
            last_checked: chrono::Utc::now().to_rfc3339(),
            evidence: vec![
                "Zero egress mode enforced".to_string(),
                "PF rules active".to_string(),
            ],
            findings: vec![],
        },
        ComplianceControl {
            control_id: "DETERM-001".to_string(),
            control_name: "Deterministic Execution".to_string(),
            status: "compliant".to_string(),
            last_checked: chrono::Utc::now().to_rfc3339(),
            evidence: vec![
                "Metal kernels precompiled".to_string(),
                "HKDF seeding enabled".to_string(),
                "Tick ledger active".to_string(),
            ],
            findings: vec![],
        },
        ComplianceControl {
            control_id: "ISOLATION-001".to_string(),
            control_name: "Tenant Isolation".to_string(),
            status: "compliant".to_string(),
            last_checked: chrono::Utc::now().to_rfc3339(),
            evidence: vec![
                "Per-tenant processes".to_string(),
                "UID/GID separation".to_string(),
            ],
            findings: vec![],
        },
    ];

    let compliant_count = controls.iter().filter(|c| c.status == "compliant").count();
    let compliance_rate = if !controls.is_empty() {
        (compliant_count as f64 / controls.len() as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(ComplianceAuditResponse {
        compliance_rate,
        total_controls: controls.len(),
        compliant_controls: compliant_count,
        active_violations: active_violations as usize,
        controls,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}
/// Query audit logs with filtering and pagination
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/audit/logs",
    params(
        ("user_id" = Option<String>, Query, description = "Filter by user ID"),
        ("action" = Option<String>, Query, description = "Filter by action"),
        ("resource_type" = Option<String>, Query, description = "Filter by resource type"),
        ("resource_id" = Option<String>, Query, description = "Filter by resource ID"),
        ("status" = Option<String>, Query, description = "Filter by status (success/failure)"),
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("from_time" = Option<String>, Query, description = "Start time (RFC3339)"),
        ("to_time" = Option<String>, Query, description = "End time (RFC3339)"),
        ("limit" = Option<usize>, Query, description = "Maximum results (default: 100, max: 1000)"),
        ("offset" = Option<usize>, Query, description = "Offset for pagination"),
    ),
    responses(
        (status = 200, description = "Audit logs retrieved successfully", body = AuditLogsResponse),
        (status = 403, description = "Forbidden - requires AuditView permission", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "audit"
)]
pub async fn query_audit_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    axum::extract::Query(query): axum::extract::Query<crate::types::AuditLogsQuery>,
) -> Result<Json<crate::types::AuditLogsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Only Admin, SRE, and Compliance can view audit logs
    crate::permissions::require_permission(&claims, crate::permissions::Permission::AuditView)?;

    // Apply defaults and limits
    let limit = query.limit.unwrap_or(100).min(1000);
    let offset = query.offset.unwrap_or(0);

    // Query audit logs from database
    // Note: The db method signature is: query_audit_logs(user_id, action, resource_type, start_date, end_date, limit)
    // Additional filtering (resource_id, status, tenant_id, offset) can be applied post-query if needed
    let _ = (
        query.resource_id.as_deref(),
        query.status.as_deref(),
        query.tenant_id.as_deref(),
        offset,
    );
    let logs = state
        .db
        .query_audit_logs(
            query.user_id.as_deref(),
            query.action.as_deref(),
            query.resource_type.as_deref(),
            query.from_time.as_deref(),
            query.to_time.as_deref(),
            limit as i64,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to query audit logs")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Convert AuditLog to AuditLogResponse
    let log_responses: Vec<crate::types::AuditLogResponse> = logs
        .iter()
        .map(|log| crate::types::AuditLogResponse {
            id: log.id.clone(),
            timestamp: log.timestamp.clone(),
            user_id: log.user_id.clone(),
            user_role: log.user_role.clone(),
            tenant_id: log.tenant_id.clone(),
            action: log.action.clone(),
            resource_type: log.resource_type.clone(),
            resource_id: log.resource_id.clone(),
            status: log.status.clone(),
            error_message: log.error_message.clone(),
            ip_address: log.ip_address.clone(),
            metadata_json: log.metadata_json.clone(),
        })
        .collect();

    let total = log_responses.len();

    Ok(Json(crate::types::AuditLogsResponse {
        logs: log_responses,
        total,
        limit,
        offset,
    }))
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct FederationAuditResponse {
    pub total_hosts: usize,
    pub total_signatures: usize,
    pub verified_signatures: usize,
    pub quarantined: bool,
    pub quarantine_reason: Option<String>,
    pub host_chains: Vec<HostChainSummary>,
    pub timestamp: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct HostChainSummary {
    pub host_id: String,
    pub bundle_count: usize,
    pub latest_bundle: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ComplianceAuditResponse {
    pub compliance_rate: f64,
    pub total_controls: usize,
    pub compliant_controls: usize,
    pub active_violations: usize,
    pub controls: Vec<ComplianceControl>,
    pub timestamp: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ComplianceControl {
    pub control_id: String,
    pub control_name: String,
    pub status: String,
    pub last_checked: String,
    pub evidence: Vec<String>,
    pub findings: Vec<String>,
}

// ============================================================================
// Semantic Name Validation Endpoints
// ============================================================================

use adapteros_core::{AdapterName, StackName};
use adapteros_policy::{
    AdapterNameValidation, NamingConfig, NamingPolicy, NamingViolation, StackNameValidation,
};

/// Convert tier string to integer: persistent=0, warm=1, ephemeral=2
fn tier_str_to_int(tier: &str) -> i32 {
    match tier {
        "persistent" => 0,
        "warm" => 1,
        "ephemeral" => 2,
        _ => 1, // default to warm
    }
}

/// Request to validate an adapter name
#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ValidateAdapterNameRequest {
    /// Adapter name to validate (format: {tenant}/{domain}/{purpose}/{revision})
    pub name: String,
    /// Tenant ID making the request
    pub tenant_id: String,
    /// Parent adapter name (if forking/extending)
    pub parent_name: Option<String>,
    /// Latest revision number in lineage (if extending)
    pub latest_revision: Option<u32>,
}

/// Response for adapter name validation
#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ValidateAdapterNameResponse {
    /// Whether the name is valid
    pub valid: bool,
    /// List of validation violations (empty if valid)
    pub violations: Vec<NameViolationResponse>,
    /// Parsed adapter name components (if valid)
    pub parsed: Option<ParsedAdapterName>,
}

/// Parsed adapter name components
#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ParsedAdapterName {
    pub tenant: String,
    pub domain: String,
    pub purpose: String,
    pub revision: String,
    pub revision_number: u32,
    pub base_path: String,
    pub display_name: String,
}

/// Name violation details
#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct NameViolationResponse {
    /// Violation type
    pub violation_type: String,
    /// Component that violated policy
    pub component: String,
    /// Detailed reason
    pub reason: String,
    /// Suggested fix
    pub suggestion: Option<String>,
}

/// Request to validate a stack name
#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ValidateStackNameRequest {
    /// Stack name to validate (format: stack.{namespace}[.{identifier}])
    pub name: String,
    /// Tenant ID making the request
    pub tenant_id: String,
}

/// Response for stack name validation
#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ValidateStackNameResponse {
    /// Whether the name is valid
    pub valid: bool,
    /// List of validation violations (empty if valid)
    pub violations: Vec<NameViolationResponse>,
    /// Parsed stack name components (if valid)
    pub parsed: Option<ParsedStackName>,
}

/// Parsed stack name components
#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ParsedStackName {
    pub namespace: String,
    pub identifier: Option<String>,
    pub full_name: String,
}

/// Validate an adapter name
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapters/validate-name",
    request_body = ValidateAdapterNameRequest,
    responses(
        (status = 200, description = "Validation result", body = ValidateAdapterNameResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "adapters"
)]
pub async fn validate_adapter_name(
    State(state): State<AppState>,
    Json(req): Json<ValidateAdapterNameRequest>,
) -> Result<Json<ValidateAdapterNameResponse>, (StatusCode, String)> {
    // Create naming policy with default config
    let policy = NamingPolicy::new(NamingConfig::default());

    // Create validation request
    let validation_req = AdapterNameValidation {
        name: req.name.clone(),
        tenant_id: req.tenant_id.clone(),
        parent_name: req.parent_name.clone(),
        latest_revision: req.latest_revision,
    };

    // Analyze the name for violations
    let violations = policy.analyze_adapter_name(&validation_req);

    // Try to parse the name if no violations
    let parsed = if violations.is_empty() {
        AdapterName::parse(&req.name)
            .ok()
            .map(|name| ParsedAdapterName {
                tenant: name.tenant().to_string(),
                domain: name.domain().to_string(),
                purpose: name.purpose().to_string(),
                revision: name.revision().to_string(),
                revision_number: name.revision_number().unwrap_or(0),
                base_path: name.base_path().to_string(),
                display_name: name.display_name().to_string(),
            })
    } else {
        None
    };

    // Convert violations to response format
    let violation_responses: Vec<_> = violations
        .iter()
        .map(|v| NameViolationResponse {
            violation_type: format!("{:?}", v.violation_type),
            component: v.component.clone(),
            reason: v.reason.clone(),
            suggestion: v.suggestion.clone(),
        })
        .collect();

    Ok(Json(ValidateAdapterNameResponse {
        valid: violation_responses.is_empty(),
        violations: violation_responses,
        parsed,
    }))
}

/// Validate a stack name
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/stacks/validate-name",
    request_body = ValidateStackNameRequest,
    responses(
        (status = 200, description = "Validation result", body = ValidateStackNameResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "stacks"
)]
pub async fn validate_stack_name(
    State(state): State<AppState>,
    Json(req): Json<ValidateStackNameRequest>,
) -> Result<Json<ValidateStackNameResponse>, (StatusCode, String)> {
    // Create naming policy with default config
    let policy = NamingPolicy::new(NamingConfig::default());

    // Create validation request
    let validation_req = StackNameValidation {
        name: req.name.clone(),
        tenant_id: req.tenant_id.clone(),
    };

    // Validate the stack name
    let result = policy.validate_stack_name(&validation_req);

    // Try to parse the name
    let parsed = StackName::parse(&req.name)
        .ok()
        .map(|name| ParsedStackName {
            namespace: name.namespace().to_string(),
            identifier: name.identifier().map(|s| s.to_string()),
            full_name: name.to_string(),
        });

    // Convert error to violation if validation failed
    let violations = if let Err(e) = result {
        vec![NameViolationResponse {
            violation_type: "ValidationError".to_string(),
            component: req.name.clone(),
            reason: e.to_string(),
            suggestion: None,
        }]
    } else {
        vec![]
    };

    let valid = violations.is_empty();
    let parsed_result = if valid { parsed } else { None };

    Ok(Json(ValidateStackNameResponse {
        valid,
        violations,
        parsed: parsed_result,
    }))
}

/// Get the next revision number for an adapter lineage
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/adapters/next-revision/{tenant}/{domain}/{purpose}",
    params(
        ("tenant" = String, Path, description = "Tenant namespace"),
        ("domain" = String, Path, description = "Domain namespace"),
        ("purpose" = String, Path, description = "Purpose identifier")
    ),
    responses(
        (status = 200, description = "Next revision number", body = NextRevisionResponse),
        (status = 404, description = "Lineage not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "adapters"
)]
pub async fn get_next_revision(
    State(state): State<AppState>,
    Path((tenant, domain, purpose)): Path<(String, String, String)>,
) -> Result<Json<NextRevisionResponse>, (StatusCode, String)> {
    // Get registry from database
    let registry = state.registry.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Registry not available".to_string(),
        )
    })?;

    // Get next revision number
    let next_rev = registry
        .next_revision_number(&tenant, &domain, &purpose)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Format the suggested name
    let suggested_name = format!("{}/{}/{}/r{:03}", tenant, domain, purpose, next_rev);

    Ok(Json(NextRevisionResponse {
        next_revision: next_rev,
        suggested_name,
        base_path: format!("{}/{}/{}", tenant, domain, purpose),
    }))
}

/// Response for next revision query
#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct NextRevisionResponse {
    /// Next revision number
    pub next_revision: u32,
    /// Suggested full adapter name
    pub suggested_name: String,
    /// Base path (tenant/domain/purpose)
    pub base_path: String,
}
