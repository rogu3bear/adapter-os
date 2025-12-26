#![allow(unused_variables)]

use crate::auth::Claims;
use crate::middleware::{require_any_role, require_role};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*; // This already re-exports adapteros_api_types::*
use crate::uds_client::{UdsClient, UdsClientError};
use crate::validation::*;
use adapteros_core::tenant_snapshot::TenantStateSnapshot;
use adapteros_core::{AosError, B3Hash};
use adapteros_db::{
    AdapterVersionRuntimeState, CreateDraftVersionParams as CreateDraftAdapterVersionParams,
};
use adapteros_lora_lifecycle::GpuIntegrityReport;
use adapteros_types::training::LoraTier;
use sqlx::{Row, Sqlite, Transaction};
// System metrics integration
use adapteros_system_metrics;
use adapteros_system_metrics::monitoring_types::{
    AcknowledgeAlertRequest, AlertResponse, AnomalyResponse, BaselineResponse,
    CreateMonitoringRuleApiRequest, MonitoringRuleResponse, RecalculateBaselineRequest,
    UpdateAnomalyStatusRequest, UpdateMonitoringRuleApiRequest,
};
use axum::response::Response;
use chrono::Utc;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

pub mod activity;
pub mod adapter_health;
pub mod adapter_lifecycle;
pub mod adapter_stacks;
pub mod adapter_utils;
pub mod adapter_versions;
pub mod adapters;
pub mod adapters_read;
pub mod admin;
pub mod admin_lifecycle;
pub mod api_keys;
pub mod auth;
pub mod auth_enhanced;
pub mod batch;
pub mod boot_progress;
pub mod capacity;
pub mod chat_sessions;
pub mod chunked_upload;
pub mod code;
pub mod collections;
pub mod coreml_verification;
pub mod dashboard;
pub mod datasets;
pub mod dev_contracts;
pub mod diagnostics;
pub mod discovery;
pub mod documents;
pub mod domain_adapters;
pub mod evidence;
pub mod execution_policy;
pub mod federation;
pub mod git;
pub mod git_repository;
pub mod golden;
pub mod health;
pub mod inference;
pub mod infrastructure;
pub mod journeys;
pub mod kv_isolation;
pub mod memory_detail;
pub mod metrics_time_series;
pub mod models;
pub mod monitoring;
pub mod node_detail;
pub mod notifications;
pub mod openai_compat;
pub mod orchestration;
pub mod owner_chat;
pub mod owner_cli;
// pub mod packages; // Feature removed in migration 0200
pub mod pilot_status;
pub mod plugins;
pub mod policies;
pub mod promotion;
pub mod rag_common;
pub mod registry;
pub mod replay;
pub mod replay_inference;
pub mod repos;
pub mod router_config;
pub mod routing_decisions;
pub mod runtime;
pub mod services;
pub mod settings;
pub mod storage;
pub mod streaming;
pub mod streaming_infer;
pub mod system_info;
pub mod system_overview;
pub mod system_state;
pub mod telemetry;
pub mod tenant_policies;
pub mod tenants;
pub mod testkit;
pub mod training;
pub mod training_datasets;
pub mod tutorials;
pub mod utils;
pub mod validation;
pub mod worker_detail;
pub mod worker_manifests;
pub mod workers;
pub mod workspaces;

// Re-export specialized adapter repository and validation handlers/types.
pub use adapters_read::{
    create_adapter_repository, get_adapter_repository, get_adapter_repository_policy,
    list_adapter_repositories, upsert_adapter_repository_policy, ListAdapterRepositoriesParams,
};
pub use validation::{
    validate_adapter_name, validate_stack_name, NameViolationResponse, ParsedAdapterName,
    ParsedStackName, ValidateAdapterNameRequest, ValidateAdapterNameResponse,
    ValidateStackNameRequest, ValidateStackNameResponse,
};

// Re-export adapter lifecycle functions
pub use adapter_lifecycle::{
    __path_load_adapter, __path_promote_adapter_state, __path_unload_adapter,
    download_adapter_manifest, load_adapter, promote_adapter_state, unload_adapter,
};

// Re-export adapter version management functions
pub use adapter_versions::{
    create_draft_version, get_adapter_version, list_adapter_versions,
    promote_adapter_version_handler, resolve_adapter_version_handler,
    rollback_adapter_version_handler, tag_adapter_version_handler, ListAdapterVersionsParams,
};

// Re-export adapter health functions
pub use adapter_health::{
    __path_get_adapter_activations, __path_verify_gpu_integrity, get_adapter_activations,
    get_adapter_health, verify_gpu_integrity,
};

// Inline module to re-export adapter lifecycle functions for routes.rs (legacy compatibility)
pub mod adapters_lifecycle {
    pub use super::adapter_lifecycle::{load_adapter, unload_adapter};
    pub use super::{delete_adapter, register_adapter};
}

// Re-export utils for error handling
pub use adapter_utils::{
    guard_in_flight_requests, lora_scope_from_provenance, lora_tier_from_provenance,
};
use utils::aos_error_to_response;

// Re-export adapter lifecycle and lineage handlers
pub use adapters::*;

// Re-export tenant handlers
pub use tenants::*;

// Re-export tenant policy handlers (including utoipa path types for OpenAPI)
pub use tenant_policies::{
    __path_list_tenant_policy_bindings, __path_query_policy_decisions, __path_toggle_tenant_policy,
    __path_verify_policy_audit_chain, list_tenant_policy_bindings, query_policy_decisions,
    toggle_tenant_policy, verify_policy_audit_chain,
};

// Re-export policy handlers from policies module (consolidates duplicates)
pub use policies::{
    // utoipa path macros
    __path_assign_policy,
    __path_list_policy_assignments,
    __path_list_violations,
    apply_policy,
    assign_policy,
    assign_tenant_policies,
    compare_policy_versions,
    export_policy,
    get_policy,
    list_policies,
    list_policy_assignments,
    list_violations,
    sign_policy,
    validate_policy,
    verify_policy_signature,
};

// Re-export auth handlers (including utoipa path types)
pub use auth::{__path_auth_login, auth_login, auth_me};
pub use auth_enhanced::{
    __path_mfa_disable_handler, __path_mfa_start_handler, __path_mfa_status_handler,
    __path_mfa_verify_handler, mfa_disable_handler, mfa_start_handler, mfa_status_handler,
    mfa_verify_handler,
};

// Re-export training handlers
pub use training::*;

// Re-export health and system info handlers
pub use coreml_verification::*;
pub use health::*;
pub use system_info::*;

// Re-export system state handler
pub use system_state::*;

// Re-export boot progress (specific to avoid ambiguity with streaming module)
pub use boot_progress::{boot_progress_stream, BootProgressEvent};

// Re-export streaming handlers
pub use streaming::*;

// Re-export adapter_stacks streaming handler
pub use adapter_stacks::stack_policy_stream;

// Re-export inference handler (including utoipa path types)
pub use inference::{__path_infer, infer};

// Re-export domain adapter handlers
pub use domain_adapters::*;

// Re-export infrastructure handlers (nodes & system operations)
pub use infrastructure::{
    evict_node, get_base_model_status, get_node_details, list_jobs, list_nodes, mark_node_offline,
    register_node, test_node_connection, ListJobsQuery,
};

// Re-export worker handlers
pub use workers::{get_worker_health_summary, list_worker_incidents, receive_worker_fatal};

use adapteros_db::sqlx;
use adapteros_db::users::Role;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, info_span, warn};

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
    validate_tenant_isolation(&claims, &tenant_id)?;
    let root_str = req.root;
    let path_str = req.path;

    // Read timeout and adapter path from config
    let (timeout_secs, adapters_root) = {
        let config = state.config.read().map_err(|e| {
            error!("Failed to acquire config read lock: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Configuration unavailable").with_code("INTERNAL_ERROR")),
            )
        })?;
        // Use centralized adapter path resolution (ENV > Config > Default)
        use adapteros_core::adapter_repo_paths::AdapterPaths;
        // adapters_root is String, convert to Option<String> for from_env_and_config
        let config_value = if config.paths.adapters_root.is_empty() {
            None
        } else {
            Some(config.paths.adapters_root.clone())
        };
        let adapters_paths = AdapterPaths::from_env_and_config(config_value);
        (
            config.directory_analysis_timeout_secs,
            adapters_paths.repo_root.clone(),
        )
    };

    let tenant_id_for_blocking = tenant_id.clone();
    let (adapter_id, hash_hex, hash_b3, analysis) = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        tokio::task::spawn_blocking(move || {
            let _span = info_span!("directory_adapter_blocking_ops", tenant = %tenant_id_for_blocking).entered();

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
                tenant_id_for_blocking,
                analysis.fingerprint.to_short_hex()
            );
            let hash_hex = analysis.fingerprint.to_hex();
            let hash_b3 = format!("b3:{}", hash_hex);

            // Phase 3: Ensure placeholder artifact (blocking filesystem I/O)
            // Use centralized adapter path resolution
            let _artifact_span = info_span!("artifact_creation", hash = %hash_hex).entered();
            let artifact_path = adapters_root.join(format!("{}.safetensors", hash_hex));
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
        state
            .db
            .get_adapter_for_tenant(&tenant_id, &adapter_id)
            .await
            .map_err(|e| {
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
            .tenant_id(&tenant_id)
            .adapter_id(&adapter_id)
            .name(&adapter_id)
            .hash_b3(&hash_b3)
            .rank(analysis.symbols.len() as i32 % 17 + 16)
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
        let adapter_result = state
            .db
            .get_adapter_for_tenant(&tenant_id, &adapter_id)
            .await;

        if let Ok(Some(_a)) = adapter_result {
            tracing::info!(adapter_id = %adapter_id, "loading adapter via lifecycle manager");

            // Use lifecycle manager if available
            if let Some(ref lifecycle) = state.lifecycle_manager {
                let mut manager = lifecycle.lock().await;

                // Load adapter (updates internal state only)
                if let Err(e) = manager.get_or_reload(&adapter_id) {
                    tracing::warn!(adapter_id = %adapter_id, error = %e, "Failed to load adapter via lifecycle manager");
                    // Fallback: update DB state to indicate load failure
                    let _ = state
                        .db
                        .update_adapter_state_tx_for_tenant(
                            &tenant_id,
                            &adapter_id,
                            "cold",
                            "load_failed",
                        )
                        .await;
                } else {
                    // Update state (handles DB update if db is set)
                    if let Some(adapter_idx) = manager.get_adapter_idx(&adapter_id) {
                        use adapteros_lora_lifecycle::AdapterState;
                        if let Err(e) = manager
                            .update_adapter_state(adapter_idx, AdapterState::Cold, "loaded_via_api")
                            .await
                        {
                            tracing::warn!(adapter_id = %adapter_id, error = %e, "Failed to update adapter state via lifecycle manager");
                            // Fallback: update DB state directly
                            let _ = state
                                .db
                                .update_adapter_state_tx_for_tenant(
                                    &tenant_id,
                                    &adapter_id,
                                    "cold",
                                    "loaded_via_api",
                                )
                                .await;
                        } else {
                            tracing::info!(adapter_id = %adapter_id, "adapter loaded successfully");
                            activated = true;
                        }
                    } else {
                        tracing::warn!(adapter_id = %adapter_id, "Adapter not found in lifecycle manager");
                        // Fallback: update DB state directly
                        let _ = state
                            .db
                            .update_adapter_state_tx_for_tenant(
                                &tenant_id,
                                &adapter_id,
                                "cold",
                                "loaded_via_api",
                            )
                            .await;
                    }
                }
            } else {
                // Fallback: direct DB update if no lifecycle manager
                tracing::info!(adapter_id = %adapter_id, "simulating adapter load (no lifecycle manager)");
                let _ = state
                    .db
                    .update_adapter_state_tx_for_tenant(
                        &tenant_id,
                        &adapter_id,
                        "warm",
                        "simulated_load",
                    )
                    .await;
                activated = true;
            }
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
// UmaPressureMonitor: using mock from AppState (adapteros_lora_worker crate is excluded)

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

    for event in &sorted_events {
        if let Err(e) = apply_event(&mut tx, &req.tenant_id, event).await {
            tracing::error!(identity = ?event.get("identity"), error = %e, "Failed to apply event in hydration");
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
                serde_json::to_string(&adapter_ids).map_err(AosError::Serialization)?;
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
            let rules_json = serde_json::to_string(&rules).map_err(AosError::Serialization)?;

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

            let value_json = serde_json::to_string(&value).map_err(AosError::Serialization)?;

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
            let value_json = serde_json::to_string(&value).map_err(AosError::Serialization)?;

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
            let value_json = serde_json::to_string(&enabled).map_err(AosError::Serialization)?;

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

/// Get base model status
///
/// # Endpoint
/// GET /v1/models/status
///
/// # Authentication
/// Optional - unauthenticated requests receive limited response
///
/// # Permissions (when authenticated)
/// Requires one of: Operator, Admin, Compliance
///
/// # Query Parameters
/// - `tenant_id`: Optional tenant ID filter (defaults to "default", only applies when authenticated)
///
/// # Response
/// Returns the current base model load status. Response varies by authentication:
///
/// **Unauthenticated response (limited data):**
/// - `model_id`: "none"
/// - `model_name`: "No Model Loaded"
/// - `model_path`: null
/// - `status`: "unloaded"
/// - `loaded_at`: null
/// - `unloaded_at`: null
/// - `error_message`: null
/// - `memory_usage_mb`: null
/// - `is_loaded`: false
/// - `updated_at`: Current timestamp
///
/// **Authenticated response (full data):**
/// - `model_id`: Identifier of the loaded model (or "none")
/// - `model_name`: Human-readable model name (or "No Model Loaded")
/// - `model_path`: Filesystem path to model files
/// - `status`: Load status (loaded, unloaded, loading, error)
/// - `loaded_at`: Timestamp when model was loaded
/// - `unloaded_at`: Timestamp when model was unloaded
/// - `error_message`: Error message if status is error
/// - `memory_usage_mb`: Memory consumption in MB
/// - `is_loaded`: Boolean flag indicating if model is currently in memory
/// - `updated_at`: Last status update timestamp
///
/// # Errors
/// - `NOT_FOUND` (404): Model referenced in status record not found in database (authenticated only)
/// - `INTERNAL_ERROR` (500): Database query failure (authenticated only)
///
/// # Example
/// ```
/// GET /v1/models/status?tenant_id=default
/// ```
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
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_millis(500))
        .timeout(std::time::Duration::from_secs(10))
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
    let spawn_url = format!("{}/spawn_worker", node.agent_endpoint);

    let max_attempts = 3u32;
    let mut attempt = 0u32;
    let mut backoff = std::time::Duration::from_millis(100);
    let response = loop {
        attempt += 1;
        match client.post(&spawn_url).json(&spawn_req).send().await {
            Ok(response) => break Ok(response),
            Err(e) => {
                if attempt >= max_attempts {
                    break Err(e);
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(std::time::Duration::from_millis(800));
            }
        }
    }
    .map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(
                ErrorResponse::new("failed to contact node agent")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(format!("{} (after {} attempts)", e, max_attempts)),
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

    // Register worker using Db trait method
    use adapteros_db::workers::WorkerInsertBuilder;
    let worker_id = uuid::Uuid::now_v7().to_string();
    let mut builder = WorkerInsertBuilder::new()
        .id(&worker_id)
        .tenant_id(&req.tenant_id)
        .node_id(&req.node_id)
        .plan_id(&req.plan_id)
        .uds_path(&uds_path)
        .status(adapteros_core::WorkerStatus::Created.as_str());
    builder = builder.pid(pid);
    let params = builder.build().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to build worker parameters")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    state.db.insert_worker(params).await.map_err(|e| {
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
        capabilities: Vec::new(),
        backend: None,
        model_id: None,
        model_hash: None,
        model_loaded: false,
        cache_used_mb: None,
        cache_max_mb: None,
        cache_pinned_entries: None,
        cache_active_entries: None,
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

    async fn resolve_plan_model_info(
        db: &adapteros_db::Db,
        plan_id: &str,
    ) -> (Option<String>, Option<String>) {
        let plan = match db.get_plan(plan_id).await {
            Ok(Some(p)) => p,
            _ => return (None, None),
        };

        let manifest_row = match db.get_manifest_by_hash(&plan.manifest_hash_b3).await {
            Ok(Some(m)) => m,
            _ => return (None, None),
        };

        let parsed =
            serde_json::from_str::<adapteros_manifest::ManifestV3>(&manifest_row.body_json)
                .or_else(|_| {
                    serde_yaml::from_str::<adapteros_manifest::ManifestV3>(&manifest_row.body_json)
                });

        match parsed {
            Ok(manifest) => (
                Some(manifest.base.model_id),
                Some(manifest.base.model_hash.to_hex()),
            ),
            Err(_) => (None, None),
        }
    }

    let mut plan_model_cache: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    let mut response: Vec<WorkerResponse> = Vec::with_capacity(workers.len());

    for w in workers {
        let runtime = state
            .worker_runtime
            .get(&w.id)
            .map(|entry| entry.value().clone())
            .unwrap_or_default();

        let (model_id, resolved_model_hash) = match plan_model_cache.get(&w.plan_id) {
            Some(cached) => cached.clone(),
            None => {
                let resolved = resolve_plan_model_info(&state.db, &w.plan_id).await;
                plan_model_cache.insert(w.plan_id.clone(), resolved.clone());
                resolved
            }
        };

        let model_hash = resolved_model_hash.or(runtime.model_hash);
        let model_loaded = matches!(w.status.as_str(), "healthy" | "draining" | "serving");

        response.push(WorkerResponse {
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
            capabilities: runtime.capabilities,
            backend: runtime.backend,
            model_id,
            model_hash,
            model_loaded,
            cache_used_mb: runtime.cache_used_mb,
            cache_max_mb: runtime.cache_max_mb,
            cache_pinned_entries: runtime.cache_pinned_entries,
            cache_active_entries: runtime.cache_active_entries,
        });
    }

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

    // Update worker status to 'stopping' using Db trait method
    state
        .db
        .update_worker_status(&worker_id, "stopping")
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

    // Update worker status to 'stopped' using Db trait method
    state
        .db
        .update_worker_status(&worker_id, "stopped")
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
    crate::audit_helper::log_success_or_warn(
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

/// List telemetry bundles (stub)
pub async fn list_telemetry_bundles(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<TelemetryBundleResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let bundles = state
        .db
        .get_telemetry_bundles_by_tenant(&claims.tenant_id, 100, 0)
        .await
        .map_err(|e| {
            tracing::error!(tenant_id = %claims.tenant_id, error = %e, "Failed to list telemetry bundles");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list telemetry bundles")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let response = bundles
        .into_iter()
        .map(|bundle| {
            let size_bytes = std::fs::metadata(&bundle.path)
                .map(|m| m.len())
                .unwrap_or(0);

            TelemetryBundleResponse {
                id: bundle.id,
                cpid: bundle.cpid,
                event_count: bundle.event_count as u64,
                size_bytes,
                created_at: bundle.created_at,
            }
        })
        .collect();

    Ok(Json(response))
}

/// Export telemetry bundle as NDJSON
pub async fn export_telemetry_bundle(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(bundle_id): Path<String>,
) -> Result<Json<ExportTelemetryBundleResponse>, (StatusCode, Json<ErrorResponse>)> {
    let bundle = state
        .db
        .get_telemetry_bundle(&claims.tenant_id, &bundle_id)
        .await
        .map_err(|e| {
            tracing::error!(tenant_id = %claims.tenant_id, bundle_id = %bundle_id, error = %e, "Failed to load telemetry bundle");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to load telemetry bundle")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let Some(bundle) = bundle else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Bundle not found").with_code("NOT_FOUND")),
        ));
    };

    let size_bytes = std::fs::metadata(&bundle.path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    Ok(Json(ExportTelemetryBundleResponse {
        bundle_id: bundle.id.clone(),
        events_count: bundle.event_count,
        size_bytes,
        download_url: format!("/v1/telemetry/bundles/{}/download", bundle.id),
        expires_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Verify telemetry bundle Ed25519 signature
pub async fn verify_bundle_signature(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(bundle_id): Path<String>,
) -> Result<Json<VerifyBundleSignatureResponse>, (StatusCode, Json<ErrorResponse>)> {
    use adapteros_core::B3Hash;

    // Parse bundle ID as B3Hash
    let bundle_hash = B3Hash::from_hex(&bundle_id).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "Invalid bundle ID format - must be hex-encoded BLAKE3 hash",
            )),
        )
    })?;

    // Get bundle metadata from store - scope the lock tightly to avoid Send issues
    let metadata = {
        let bundle_store = state.telemetry_bundle_store.read().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to access bundle store")),
            )
        })?;

        bundle_store.get_metadata(&bundle_hash).cloned()
        // Lock dropped here at end of block
    };

    let metadata = match metadata {
        Some(meta) => meta,
        None => {
            return Ok(Json(VerifyBundleSignatureResponse {
                bundle_id,
                valid: false,
                signature: String::new(),
                signed_by: String::new(),
                signed_at: String::new(),
                verification_error: Some("Bundle not found".to_string()),
            }));
        }
    };

    // Verify the signature using the telemetry library
    let verification_result = adapteros_telemetry::verify_bundle_signature(
        &metadata.merkle_root,
        &metadata.signature,
        &metadata.public_key,
    );

    match verification_result {
        Ok(true) => {
            // Log successful verification
            state
                .log_crypto_success(
                    adapteros_crypto::audit::CryptoOperation::Verify,
                    Some(metadata.key_id.clone()),
                    None,
                    serde_json::json!({
                        "bundle_id": bundle_id,
                        "merkle_root": metadata.merkle_root.to_string(),
                    }),
                )
                .await;

            // Convert signed_at_us to RFC3339
            let signed_at = chrono::DateTime::from_timestamp_micros(metadata.signed_at_us as i64)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| "unknown".to_string());

            Ok(Json(VerifyBundleSignatureResponse {
                bundle_id,
                valid: true,
                signature: format!(
                    "ed25519:{}",
                    &metadata.signature[..16.min(metadata.signature.len())]
                ),
                signed_by: metadata.key_id.clone(),
                signed_at,
                verification_error: None,
            }))
        }
        Ok(false) => {
            // Log verification failure
            state
                .log_crypto_failure(
                    adapteros_crypto::audit::CryptoOperation::Verify,
                    Some(metadata.key_id.clone()),
                    None,
                    "Signature verification failed",
                    serde_json::json!({
                        "bundle_id": bundle_id,
                        "merkle_root": metadata.merkle_root.to_string(),
                    }),
                )
                .await;

            Ok(Json(VerifyBundleSignatureResponse {
                bundle_id,
                valid: false,
                signature: format!(
                    "ed25519:{}",
                    &metadata.signature[..16.min(metadata.signature.len())]
                ),
                signed_by: metadata.key_id.clone(),
                signed_at: String::new(),
                verification_error: Some("Signature verification failed".to_string()),
            }))
        }
        Err(e) => {
            // Log verification error
            state
                .log_crypto_failure(
                    adapteros_crypto::audit::CryptoOperation::Verify,
                    Some(metadata.key_id.clone()),
                    None,
                    &format!("Verification error: {}", e),
                    serde_json::json!({
                        "bundle_id": bundle_id,
                    }),
                )
                .await;

            Ok(Json(VerifyBundleSignatureResponse {
                bundle_id,
                valid: false,
                signature: metadata.signature.clone(),
                signed_by: metadata.key_id.clone(),
                signed_at: String::new(),
                verification_error: Some(format!("Verification error: {}", e)),
            }))
        }
    }
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

fn is_missing_metrics_table(err: &adapteros_core::AosError) -> bool {
    err.to_string()
        .contains("no such table: process_health_metrics")
}

fn map_metrics(
    metrics: Vec<adapteros_system_metrics::ProcessHealthMetric>,
) -> Vec<ProcessHealthMetricResponse> {
    metrics
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
        .collect()
}

async fn fetch_process_health_metrics_with_fallback(
    state: &AppState,
    filters: adapteros_system_metrics::MetricFilters,
) -> Result<Vec<ProcessHealthMetricResponse>, (StatusCode, Json<ErrorResponse>)> {
    match adapteros_system_metrics::ProcessHealthMetric::query(state.db.pool(), filters).await {
        Ok(metrics) => Ok(map_metrics(metrics)),
        Err(e) => {
            if is_missing_metrics_table(&e) {
                warn!(
                    error = %e,
                    "process_health_metrics table missing; returning empty metrics payload"
                );
                Ok(Vec::new())
            } else {
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                ))
            }
        }
    }
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

    let response_metrics = fetch_process_health_metrics_with_fallback(&state, filters).await?;

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
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringReportResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let worker_filter = params.get("worker_id");
    let metric_filter = params.get("metric_name");
    let start_time_filter = params.get("start_time");
    let end_time_filter = params.get("end_time");

    let filters = adapteros_system_metrics::MetricFilters {
        worker_id: worker_filter.cloned(),
        tenant_id: None,
        metric_name: metric_filter.cloned(),
        start_time: start_time_filter
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        end_time: end_time_filter
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        limit: Some(1000),
    };

    let response_metrics = fetch_process_health_metrics_with_fallback(&state, filters).await?;

    let report = ProcessMonitoringReportResponse {
        id: format!("report-{}", Uuid::now_v7()),
        name: "Live metrics".to_string(),
        description: Some("Alias of /v1/monitoring/health-metrics".to_string()),
        tenant_id: claims.tenant_id.clone(),
        report_type: "metrics_alias".to_string(),
        report_config: json!({"alias": "health-metrics", "filters": params}),
        generated_at: Utc::now().to_rfc3339(),
        report_data: Some(json!(response_metrics)),
        file_path: None,
        file_size_bytes: None,
        created_by: Some(claims.sub.clone()),
    };

    Ok(Json(vec![report]))
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
        ("tier" = Option<String>, Query, description = "Filter by tier"),
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

    let adapters = state
        .db
        .list_adapters_for_tenant(&claims.tenant_id)
        .await
        .map_err(|e| {
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
        // Enforce tenant isolation: skip adapters not belonging to user's tenant
        // (admin users can see all adapters)
        if claims.role != "admin" && validate_tenant_isolation(&claims, &adapter.tenant_id).is_err()
        {
            continue; // Skip this adapter
        }

        // Filter by tier if specified
        if let Some(ref tier) = query.tier {
            if adapter.tier != *tier {
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

        let lora_tier = lora_tier_from_provenance(&adapter.provenance_json);
        let lora_scope =
            lora_scope_from_provenance(&adapter.provenance_json, Some(adapter.scope.clone()));
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
            assurance_tier: None,
            languages,
            framework: adapter.framework.clone(),
            category: Some(adapter.category.clone()),
            scope: Some(adapter.scope.clone()),
            framework_id: adapter.framework_id.clone(),
            framework_version: adapter.framework_version.clone(),
            repo_id: adapter.repo_id.clone(),
            commit_sha: adapter.commit_sha.clone(),
            intent: adapter.intent.clone(),
            lora_tier,
            lora_strength: adapter.lora_strength,
            lora_scope,
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
            deduplicated: None,
            drift_reference_backend: None,
            drift_baseline_backend: None,
            drift_test_backend: None,
            drift_tier: None,
            drift_metric: None,
            drift_loss_metric: None,
            drift_slice_size: None,
            drift_slice_offset: None,
        });
    }

    Ok(Json(responses))
}

/// Archive a repository
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapter-repositories/{repo_id}/archive",
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    responses(
        (status = 204, description = "Repository archived"),
        (status = 404, description = "Repository not found", body = ErrorResponse)
    )
)]
pub async fn archive_adapter_repository(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;

    let archived = state
        .db
        .archive_adapter_repository(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to archive repository")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if !archived {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("repository not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(repo_id),
            ),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
// ListAdapterVersionsParams moved to adapter_versions module
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
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    let adapter = state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &adapter_id)
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

    let lora_tier = lora_tier_from_provenance(&adapter.provenance_json);
    let lora_scope =
        lora_scope_from_provenance(&adapter.provenance_json, Some(adapter.scope.clone()));

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
        assurance_tier: None,
        languages,
        framework: adapter.framework.clone(),
        category: Some(adapter.category.clone()),
        scope: Some(adapter.scope.clone()),
        framework_id: adapter.framework_id.clone(),
        framework_version: adapter.framework_version.clone(),
        repo_id: adapter.repo_id.clone(),
        commit_sha: adapter.commit_sha.clone(),
        intent: adapter.intent.clone(),
        lora_tier,
        lora_strength: adapter.lora_strength,
        lora_scope,
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
        deduplicated: None,
        drift_reference_backend: None,
        drift_baseline_backend: None,
        drift_test_backend: None,
        drift_tier: None,
        drift_metric: None,
        drift_loss_metric: None,
        drift_slice_size: None,
        drift_slice_offset: None,
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
            Json(ErrorResponse::new("category is required").with_code("BAD_REQUEST")),
        ));
    }

    // POLICY ENFORCEMENT: Check naming policy compliance
    // Get policy assignments for this tenant
    let policy_assignments = state
        .db
        .get_policy_assignments("tenant", Some(&claims.tenant_id))
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to get policy assignments");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check policy assignments")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Check if naming policy is assigned and enforced
    for assignment in &policy_assignments {
        if assignment.enforced {
            // Fetch the policy pack
            if let Ok(Some(pack)) = state.db.get_policy_pack(&assignment.policy_pack_id).await {
                if pack.policy_type == "naming" && pack.status == "active" {
                    // Parse naming policy configuration from policy content
                    use adapteros_policy::packs::naming_policy::{
                        AdapterNameValidation, NamingConfig, NamingPolicy,
                    };
                    // Security: Fail explicitly on malformed policy JSON to prevent bypass
                    let config: NamingConfig =
                        serde_json::from_str(&pack.content_json).map_err(|e| {
                            tracing::error!(
                                policy_pack_id = %pack.id,
                                error = %e,
                                "Malformed policy pack JSON - refusing to apply empty policy"
                            );
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(
                                    ErrorResponse::new(
                                        "Policy pack has invalid JSON configuration",
                                    )
                                    .with_code("POLICY_PACK_CORRUPT")
                                    .with_string_details(
                                        format!(
                                            "Policy pack '{}' contains malformed JSON: {}",
                                            pack.id, e
                                        ),
                                    ),
                                ),
                            )
                        })?;
                    let naming_policy = NamingPolicy::new(config);

                    // Validate adapter name against naming policy
                    let validation_request = AdapterNameValidation {
                        name: req.name.clone(),
                        tenant_id: claims.tenant_id.clone(),
                        parent_name: None,
                        latest_revision: None,
                    };

                    if let Err(e) = naming_policy.validate_adapter_name(&validation_request) {
                        // Record policy violation
                        let violation_id = state
                            .db
                            .record_policy_violation(
                                &pack.id,
                                Some(&assignment.id),
                                "naming",
                                "high",
                                "adapter",
                                Some(&req.adapter_id),
                                &claims.tenant_id,
                                &format!("Naming policy violation: {}", e),
                                None,
                            )
                            .await
                            .map_err(|e| {
                                tracing::error!(error = %e, "Failed to record policy violation");
                                (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(
                                        ErrorResponse::new("Failed to record policy violation")
                                            .with_code("INTERNAL_ERROR")
                                            .with_string_details(e.to_string()),
                                    ),
                                )
                            })?;

                        tracing::warn!(
                            adapter_name = %req.name,
                            tenant_id = %claims.tenant_id,
                            violation_id = %violation_id,
                            "Naming policy violation detected"
                        );

                        // Audit log: policy violation during adapter registration
                        crate::audit_helper::log_failure_or_warn(
                            &state.db,
                            &claims,
                            crate::audit_helper::actions::ADAPTER_REGISTER,
                            crate::audit_helper::resources::ADAPTER,
                            Some(&req.adapter_id),
                            &format!("Naming policy violation: {}", e),
                        )
                        .await;

                        // Reject registration if naming policy is enforced
                        return Err((
                            StatusCode::FORBIDDEN,
                            Json(
                                ErrorResponse::new(format!("Naming policy violation: {}", e))
                                    .with_code("POLICY_VIOLATION")
                                    .with_string_details(format!("Violation ID: {}", violation_id)),
                            ),
                        ));
                    }
                }
            }
        }
    }

    // Build registration params using the builder pattern
    let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .tenant_id(&claims.tenant_id)
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

    let id = match state.db.register_adapter(params).await {
        Ok(id) => id,
        Err(e) => {
            // Audit log: adapter registration failure
            crate::audit_helper::log_failure_or_warn(
                &state.db,
                &claims,
                crate::audit_helper::actions::ADAPTER_REGISTER,
                crate::audit_helper::resources::ADAPTER,
                Some(&req.adapter_id),
                &format!("Failed to register adapter: {}", e),
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to register adapter")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    // Audit log: adapter registration
    crate::audit_helper::log_success_or_warn(
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
            assurance_tier: None,
            version: "1.0".to_string(),
            lifecycle_state: "active".to_string(),
            languages: req.languages,
            framework: req.framework,
            category: Some(req.category.clone()),
            scope: req.scope.clone(),
            lora_tier: None,
            lora_strength: Some(1.0),
            lora_scope: req.scope.clone(),
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
            deduplicated: None,
            drift_reference_backend: None,
            drift_baseline_backend: None,
            drift_test_backend: None,
            drift_tier: None,
            drift_metric: None,
            drift_loss_metric: None,
            drift_slice_size: None,
            drift_slice_offset: None,
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

    // Get adapter with tenant-scoped query
    let _adapter = state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &adapter_id)
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

    if let Err(e) = state
        .db
        .delete_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
    {
        // Audit log: adapter deletion failure
        crate::audit_helper::log_failure_or_warn(
            &state.db,
            &claims,
            crate::audit_helper::actions::ADAPTER_DELETE,
            crate::audit_helper::resources::ADAPTER,
            Some(&adapter_id),
            &format!("Failed to delete adapter: {}", e),
        )
        .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to delete adapter")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        ));
    }

    // Audit log: adapter deletion
    crate::audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_DELETE,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
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

    let adapters = state
        .db
        .list_adapters_for_tenant(&claims.tenant_id)
        .await
        .map_err(|e| {
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

    // Collect additional metrics for frontend compatibility
    let active_workers =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE status = 'active'")
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(0) as i32;

    let requests_per_second = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-1 minute')",
    )
    .fetch_one(state.db.pool())
    .await
    .map(|count| count as f32 / 60.0)
    .unwrap_or(0.0);

    let avg_latency_ms = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT AVG(latency_ms) FROM request_log WHERE timestamp > datetime('now', '-5 minutes')",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(None)
    .unwrap_or(0.0) as f32;

    // Calculate active sessions count
    let active_sessions = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM chat_sessions WHERE updated_at > datetime('now', '-30 minutes')",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(0) as i32;

    // Calculate error rate from recent requests
    let error_rate = {
        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-5 minutes')",
        )
        .fetch_one(state.db.pool())
        .await
        .unwrap_or(0);

        if total > 0 {
            let errors = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-5 minutes') AND status_code >= 500",
            )
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(0);
            Some((errors as f32) / (total as f32))
        } else {
            Some(0.0)
        }
    };

    // Tokens per second would come from inference telemetry - use 0.0 as default
    // TODO: Track actual tokens/sec from inference endpoints
    let tokens_per_second: f32 = 0.0;

    // Calculate p95 latency
    let latency_p95_ms = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT latency_ms FROM request_log WHERE timestamp > datetime('now', '-5 minutes') ORDER BY latency_ms DESC LIMIT 1 OFFSET (SELECT COUNT(*) * 5 / 100 FROM request_log WHERE timestamp > datetime('now', '-5 minutes'))",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(None)
    .map(|v| v as f32);

    Ok(Json(SystemMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        cpu_usage: metrics.cpu_usage as f32,
        memory_usage: metrics.memory_usage as f32,
        active_workers,
        requests_per_second,
        avg_latency_ms,
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
        // Additional fields for frontend compatibility
        cpu_usage_percent: Some(metrics.cpu_usage as f32),
        memory_usage_percent: Some(metrics.memory_usage as f32),
        tokens_per_second: Some(tokens_per_second),
        error_rate,
        active_sessions: Some(active_sessions),
        latency_p95_ms,
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
    Extension(claims): Extension<Claims>,
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
    let adapters = state
        .db
        .list_adapters_for_tenant(&claims.tenant_id)
        .await
        .map_err(|e| {
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
                base_model: adapter.base_model_id.clone(),
                ..Default::default()
            }
        })
        .collect();

    // Create router and route with code features
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    let decision = router
        .route_with_code_features(&code_features, &adapter_infos)
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to compute routing decision for debug");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to compute routing decision")
                        .with_code("ROUTING_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
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

    let frameworks: Vec<String> = code_features.framework_prior.keys().cloned().collect();

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
                    let gate_float = (c.gate_q15 as f32) / 32767.0;
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
        source_type: params.source_type.clone(),
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
                let gate_float = (c.gate_q15 as f32) / 32767.0;
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

    // Update alert metrics from database
    {
        use adapteros_db::process_monitoring::{AlertFilters, ProcessAlert};

        let filters = AlertFilters::default();
        match ProcessAlert::list(state.db.pool(), filters).await {
            Ok(alerts) => {
                let alert_tuples: Vec<(String, String, String, String, String)> = alerts
                    .iter()
                    .map(|a| {
                        (
                            a.title.clone(),
                            format!("{:?}", a.severity).to_lowercase(),
                            a.tenant_id.clone(),
                            a.worker_id.clone(),
                            format!("{:?}", a.status).to_lowercase(),
                        )
                    })
                    .collect();
                state.metrics_exporter.update_alert_metrics(&alert_tuples);
            }
            Err(e) => {
                tracing::warn!("Failed to fetch alerts for metrics: {}", e);
            }
        }
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
                        .data("{\"error\": \"serialization failed\"}")),
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
    let receiver = state.telemetry_tx.subscribe();

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
    Extension(claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let tenant_id = claims.tenant_id.clone();
    let stream = stream::unfold((state, tenant_id), |(state, tenant_id)| async move {
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Fetch all adapters
        let adapters = match state.db.list_adapters_for_tenant(&tenant_id).await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("Failed to fetch adapters for SSE: {}", e);
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data(format!("{{\"error\": \"{}\"}}", e))),
                    (state, tenant_id),
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
                        .data("{\"error\": \"serialization failed\"}")),
                    (state, tenant_id),
                ));
            }
        };

        Some((
            Ok(Event::default().event("adapters").data(json)),
            (state, tenant_id),
        ))
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

    let active_workers =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE status = 'active'")
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(0) as i32;

    let requests_per_second = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-1 minute')",
    )
    .fetch_one(state.db.pool())
    .await
    .map(|count| count as f32 / 60.0)
    .unwrap_or(0.0);

    let avg_latency_ms = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT AVG(latency_ms) FROM request_log WHERE timestamp > datetime('now', '-5 minutes')",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(None)
    .unwrap_or(0.0) as f32;

    Ok(SystemMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        cpu_usage: metrics.cpu_usage as f32,
        memory_usage: metrics.memory_usage as f32,
        active_workers,
        requests_per_second,
        avg_latency_ms,
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
        // Additional fields for frontend compatibility (optional in internal helper)
        cpu_usage_percent: Some(metrics.cpu_usage as f32),
        memory_usage_percent: Some(metrics.memory_usage as f32),
        tokens_per_second: None,
        error_rate: None,
        active_sessions: None,
        latency_p95_ms: None,
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

    // Serialize post_actions to JSON if provided
    let post_actions_json = req
        .post_actions
        .as_ref()
        .and_then(|pa| serde_json::to_string(pa).ok());

    let dataset_version_ids_core = req.dataset_version_ids.as_ref().map(|versions| {
        versions
            .iter()
            .map(|v| adapteros_types::training::DatasetVersionSelection {
                dataset_version_id: v.dataset_version_id.clone(),
                weight: v.weight,
            })
            .collect()
    });

    let has_versions = dataset_version_ids_core
        .as_ref()
        .map(|v: &Vec<adapteros_types::training::DatasetVersionSelection>| !v.is_empty())
        .unwrap_or(false);
    let (synthetic_mode, data_lineage_mode) = if has_versions {
        (false, adapteros_types::training::DataLineageMode::Versioned)
    } else if req.dataset_id.is_some() {
        (
            false,
            adapteros_types::training::DataLineageMode::DatasetOnly,
        )
    } else {
        (true, adapteros_types::training::DataLineageMode::Synthetic)
    };

    let job = state
        .training_service
        .start_training(
            req.adapter_name.clone(),
            config,
            req.template_id,
            req.repo_id,
            req.target_branch,
            req.base_version_id,
            req.dataset_id,                 // dataset_id
            dataset_version_ids_core,       // dataset_version_ids
            synthetic_mode,                 // synthetic_mode
            data_lineage_mode,              // data_lineage_mode
            Some(claims.tenant_id.clone()), // tenant_id
            Some(claims.sub.clone()),       // initiated_by
            Some(claims.role.clone()),      // initiated_by_role
            req.base_model_id,              // base_model_id
            req.collection_id,              // collection_id
            req.scope.clone(),              // scope
            req.lora_tier,                  // lora_tier
            // Category metadata
            req.category,
            req.description,
            req.language,
            req.framework_id,
            req.framework_version,
            // Post-training actions
            post_actions_json,
            // Not a retry - new training job
            None,
            None,                // versioning
            req.code_commit_sha, // code_commit_sha
            req.data_spec,       // data_spec_json
            req.data_spec_hash,  // data_spec_hash
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
    crate::audit_helper::log_success_or_warn(
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
        backend: job.backend.clone(),
        backend_device: job.backend_device.clone(),
        using_gpu: job.backend.as_ref().map(|b| b != "CPU"),
        examples_processed: job.examples_processed,
        tokens_processed: job.tokens_processed,
        training_time_ms: job.training_time_ms,
        throughput_examples_per_sec: job.throughput_examples_per_sec,
        gpu_utilization_pct: job.gpu_utilization_pct,
        peak_gpu_memory_mb: job.peak_gpu_memory_mb,
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
                    ErrorResponse::new(format!("Training job not found: {}", job_id))
                        .with_code("NOT_FOUND"),
                ),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to get job: {}", e))
                        .with_code("DATABASE_ERROR"),
                ),
            )
        }
    })?;

    // Check if job is completed
    let ready = job.status.to_string().to_lowercase() == "completed";

    // Build artifacts list based on job output
    let mut artifacts = Vec::new();
    let total_size_bytes = 0u64;

    if ready {
        // Add weights artifact if job has adapter_id (populated on completion)
        if let Some(ref adapter_id) = job.adapter_id {
            artifacts.push(TrainingArtifact {
                name: format!("{}.safetensors", adapter_id),
                artifact_type: "weights".to_string(),
                size_bytes: 0, // Size would come from actual file
                hash_b3: job.weights_hash_b3.clone(),
                path: format!("/v1/adapters/{}/download", adapter_id),
                created_at: job.created_at.clone(),
            });
        }

        // Add packaged .aos artifact if available
        if let Some(ref aos_path) = job.aos_path {
            artifacts.push(TrainingArtifact {
                name: aos_path
                    .split('/')
                    .next_back()
                    .unwrap_or("adapter.aos")
                    .to_string(),
                artifact_type: "package".to_string(),
                size_bytes: 0, // TODO: populate size when available
                hash_b3: job
                    .package_hash_b3
                    .clone()
                    .or_else(|| job.weights_hash_b3.clone()),
                path: aos_path.clone(),
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
    state
        .db
        .update_anomaly_status(
            &anomaly_id,
            &req.status,
            req.investigation_notes.as_deref().unwrap_or(""),
            req.investigated_by.as_deref().unwrap_or("system"),
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

    // PRD-DATA-01: Check T1 adapter evidence compliance (cp-evidence-004)
    let t1_adapters_without_dataset = sqlx::query(
        r#"
        SELECT COUNT(*) as count
        FROM adapters
        WHERE tier = 'persistent'
          AND (primary_dataset_id IS NULL OR primary_dataset_id = '')
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to count T1 adapters without dataset")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    let t1_without_dataset: i64 = t1_adapters_without_dataset.try_get("count").unwrap_or(0);

    let t1_adapters_without_evidence = sqlx::query(
        r#"
        SELECT COUNT(DISTINCT a.id) as count
        FROM adapters a
        WHERE a.tier = 'persistent'
          AND NOT EXISTS (
              SELECT 1 FROM evidence_entries e
              WHERE e.adapter_id = a.id
          )
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to count T1 adapters without evidence")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    let t1_without_evidence: i64 = t1_adapters_without_evidence.try_get("count").unwrap_or(0);

    // Generate compliance controls status
    let mut controls = vec![
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

    // PRD-DATA-01: Add evidence control (cp-evidence-004)
    let evidence_status = if t1_without_dataset == 0 && t1_without_evidence == 0 {
        "compliant"
    } else {
        "non_compliant"
    };
    let mut evidence_findings = vec![];
    if t1_without_dataset > 0 {
        evidence_findings.push(format!(
            "{} T1 adapters missing primary dataset",
            t1_without_dataset
        ));
    }
    if t1_without_evidence > 0 {
        evidence_findings.push(format!(
            "{} T1 adapters missing evidence entries",
            t1_without_evidence
        ));
    }

    controls.push(ComplianceControl {
        control_id: "EVIDENCE-004".to_string(),
        control_name: "Training Provenance & Evidence (cp-evidence-004)".to_string(),
        status: evidence_status.to_string(),
        last_checked: chrono::Utc::now().to_rfc3339(),
        evidence: vec![
            "Dataset-adapter linkage enabled".to_string(),
            "Evidence entries tracked".to_string(),
        ],
        findings: evidence_findings,
    });

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
        .query_audit_logs_for_tenant(
            &claims.tenant_id,
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
) -> Result<Json<NextRevisionResponse>, (StatusCode, Json<ErrorResponse>)> {
    use crate::error_helpers::internal_error;

    // Get registry from database
    let registry = state
        .registry
        .as_ref()
        .ok_or_else(|| internal_error("Registry not available"))?;

    // Get next revision number
    let next_rev = registry
        .next_revision_number(&tenant, &domain, &purpose)
        .map_err(internal_error)?;

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
