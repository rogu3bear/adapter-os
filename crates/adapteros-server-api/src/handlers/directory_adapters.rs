//! Directory adapter handlers
//!
//! This module contains handlers for directory-based synthetic adapters.

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use adapteros_db::users::Role;
use axum::{extract::State, http::StatusCode, Extension, Json};
use tracing::{error, info_span};

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
        use adapteros_core::adapter_repo_paths::RepoAdapterPaths;
        // adapters_root is String, convert to Option<String> for from_env_and_config
        let config_value = if config.paths.adapters_root.is_empty() {
            None
        } else {
            Some(config.paths.adapters_root.clone())
        };
        let adapters_paths = RepoAdapterPaths::from_env_and_config(config_value);
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
                        .lifecycle_db()
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
                                .lifecycle_db()
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
                            .lifecycle_db()
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
                    .lifecycle_db()
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
