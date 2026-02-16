//! Directory adapter handlers
//!
//! This module contains handlers for directory-based adapters with deterministic
//! SafeTensors artifacts.

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use adapteros_core::B3Hash;
use adapteros_db::users::Role;
use adapteros_lora_worker::{DirectoryAdapterManager, DirectoryAdapterSpec, LoRAWeights};
use axum::{extract::State, http::StatusCode, Extension, Json};
use safetensors::tensor::TensorView;
use std::path::Path;
use tracing::{error, info_span};

/// Upsert a directory adapter and optionally activate it.
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
/// 3. **Artifact Creation**: Generates deterministic `.safetensors` bytes and persists artifact
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
/// - `artifact_creation`: Artifact write phase (includes content hash field)
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
    let (adapter_id, hash_b3, analysis, adapter_rank) = tokio::time::timeout(
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

            // Build deterministic directory adapter spec and serialize canonical weights.
            let directory_adapter_spec =
                directory_adapter_spec_from_analysis(&tenant_id_for_blocking, &analysis).map_err(
                    |e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                ErrorResponse::new("failed to build directory adapter spec")
                                    .with_code("INTERNAL_SERVER_ERROR")
                                    .with_string_details(e),
                            ),
                        )
                    },
                )?;
            let artifact_bytes = serialize_lora_weights_to_safetensors(&directory_adapter_spec.weights)
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("failed to serialize adapter artifact")
                                .with_code("INTERNAL_SERVER_ERROR")
                                .with_string_details(e),
                        ),
                    )
                })?;

            let hash_hex = B3Hash::hash(&artifact_bytes).to_hex().to_string();
            let hash_b3 = format!("b3:{}", hash_hex);
            let adapter_rank = i32::try_from(directory_adapter_spec.weights.lora_a.len()).map_err(
                |_| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("failed to compute adapter rank")
                                .with_code("INTERNAL_SERVER_ERROR")
                                .with_string_details("adapter rank exceeds i32"),
                        ),
                    )
                },
            )?;

            // Phase 3: Ensure deterministic artifact exists (blocking filesystem I/O)
            let _artifact_span = info_span!("artifact_creation", hash = %hash_hex).entered();
            let artifact_path = adapters_root
                .join(&tenant_id_for_blocking)
                .join(format!("{}.safetensors", directory_adapter_spec.adapter_id));
            write_adapter_artifact_if_changed(&artifact_path, &artifact_bytes).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to write adapter artifact")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e),
                    ),
                )
            })?;
            drop(_artifact_span);

            Ok((directory_adapter_spec.adapter_id, hash_b3, analysis, adapter_rank))
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

    if let Some(existing_adapter) = &existing {
        if existing_adapter.hash_b3.as_str() != hash_b3.as_str() {
            state
                .db
                .update_adapter_weight_hash_for_tenant(&tenant_id, &adapter_id, &hash_b3)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("failed to update adapter hash")
                                .with_code("INTERNAL_SERVER_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;
        }
    } else {
        let languages = analysis.language_stats.keys().cloned().collect::<Vec<_>>();
        let languages_json = serde_json::to_string(&languages).unwrap_or("[]".to_string());

        tracing::info!(adapter_id = %adapter_id, "registering adapter in db");
        let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
            .tenant_id(&tenant_id)
            .adapter_id(&adapter_id)
            .name(&adapter_id)
            .hash_b3(&hash_b3)
            .rank(adapter_rank)
            .tier("warm")
            .languages_json(Some(languages_json.clone()))
            // Keep within DB enum constraints; directory adapters are effectively codebase-scoped.
            .category("codebase")
            // Keep within DB enum constraints; directory adapters are still globally routable.
            .scope("global")
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
                        use adapteros_lora_lifecycle::AdapterHeatState;
                        if let Err(e) = manager
                            .update_adapter_state(
                                adapter_idx,
                                AdapterHeatState::Cold,
                                "loaded_via_api",
                            )
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

fn directory_adapter_spec_from_analysis(
    tenant_id: &str,
    analysis: &adapteros_codegraph::DirectoryAnalysis,
) -> std::result::Result<DirectoryAdapterSpec, String> {
    let mut manager = DirectoryAdapterManager::new();
    let adapter_id = manager
        .upsert_from_analysis(tenant_id, analysis)
        .map_err(|e| format!("directory adapter upsert failed: {}", e))?;
    manager
        .adapters_for_tenant(tenant_id)
        .into_iter()
        .find(|spec| spec.adapter_id == adapter_id)
        .cloned()
        .ok_or_else(|| "directory adapter spec not found after upsert".to_string())
}

fn serialize_lora_weights_to_safetensors(
    weights: &LoRAWeights,
) -> std::result::Result<Vec<u8>, String> {
    let rank = weights.lora_a.len();
    let hidden_dim = weights.lora_a.first().map(|row| row.len()).unwrap_or(0);
    if rank == 0 || hidden_dim == 0 {
        return Err("lora weights must contain non-empty lora_a matrix".to_string());
    }
    if weights.lora_a.iter().any(|row| row.len() != hidden_dim) {
        return Err("lora_a rows must have consistent hidden_dim".to_string());
    }
    if weights.lora_b.len() != hidden_dim {
        return Err("lora_b outer dimension must match hidden_dim".to_string());
    }
    if weights.lora_b.iter().any(|row| row.len() != rank) {
        return Err("lora_b rows must have rank entries".to_string());
    }

    let mut lora_a_bytes = Vec::with_capacity(rank * hidden_dim * std::mem::size_of::<f32>());
    for row in &weights.lora_a {
        for value in row {
            lora_a_bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut lora_b_bytes = Vec::with_capacity(hidden_dim * rank * std::mem::size_of::<f32>());
    for row in &weights.lora_b {
        for value in row {
            lora_b_bytes.extend_from_slice(&value.to_le_bytes());
        }
    }

    let lora_a = TensorView::new(
        safetensors::Dtype::F32,
        vec![rank, hidden_dim],
        &lora_a_bytes,
    )
    .map_err(|e| format!("failed to build lora_a tensor: {}", e))?;
    let lora_b = TensorView::new(
        safetensors::Dtype::F32,
        vec![hidden_dim, rank],
        &lora_b_bytes,
    )
    .map_err(|e| format!("failed to build lora_b tensor: {}", e))?;

    safetensors::serialize([("lora_a", lora_a), ("lora_b", lora_b)], &None)
        .map_err(|e| format!("failed to serialize safetensors: {}", e))
}

fn write_adapter_artifact_if_changed(
    artifact_path: &Path,
    artifact_bytes: &[u8],
) -> std::result::Result<bool, String> {
    use std::io::Write as _;

    let parent = artifact_path.parent().unwrap_or_else(|| Path::new("."));
    if !parent.as_os_str().is_empty() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create adapters directory: {}", e))?;
    }

    match std::fs::read(artifact_path) {
        Ok(existing) if existing == artifact_bytes => return Ok(false),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(format!(
                "failed to read existing adapter artifact {}: {}",
                artifact_path.display(),
                e
            ))
        }
    }

    let file_name = artifact_path.file_name().ok_or_else(|| {
        format!(
            "invalid adapter artifact path {}: missing file name",
            artifact_path.display()
        )
    })?;

    let mut temp_path = None;
    let mut temp_file = None;
    for attempt in 0..64_u32 {
        let candidate_name = format!(
            ".{}.tmp.{}.{}",
            file_name.to_string_lossy(),
            std::process::id(),
            attempt
        );
        let candidate_path = parent.join(candidate_name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate_path)
        {
            Ok(file) => {
                temp_path = Some(candidate_path);
                temp_file = Some(file);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!(
                    "failed to create temporary adapter artifact {}: {}",
                    candidate_path.display(),
                    e
                ))
            }
        }
    }

    let temp_path = temp_path.ok_or_else(|| {
        format!(
            "failed to create temporary adapter artifact in {} after multiple attempts",
            parent.display()
        )
    })?;
    let mut temp_file = temp_file.ok_or_else(|| {
        format!(
            "failed to open temporary adapter artifact file {}",
            temp_path.display()
        )
    })?;
    if let Err(e) = temp_file.write_all(artifact_bytes) {
        drop(temp_file);
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!(
            "failed to write temporary adapter artifact {}: {}",
            temp_path.display(),
            e
        ));
    }
    if let Err(e) = temp_file.sync_all() {
        drop(temp_file);
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!(
            "failed to sync temporary adapter artifact {}: {}",
            temp_path.display(),
            e
        ));
    }
    drop(temp_file);

    std::fs::rename(&temp_path, artifact_path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        format!(
            "failed to atomically replace adapter artifact {} from {}: {}",
            artifact_path.display(),
            temp_path.display(),
            e
        )
    })?;

    #[cfg(unix)]
    if let Ok(parent_dir) = std::fs::File::open(parent) {
        let _ = parent_dir.sync_all();
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_codegraph::{DirectoryAnalysis, DirectorySymbol, DirectorySymbolKind};
    use adapteros_core::{resolve_var_dir, B3Hash};
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    fn sample_analysis(path: &str) -> DirectoryAnalysis {
        DirectoryAnalysis {
            path: PathBuf::from(path),
            symbols: vec![DirectorySymbol {
                name: "handler".to_string(),
                kind: DirectorySymbolKind::Function,
                file: PathBuf::from("src/lib.rs"),
                language: "rust".to_string(),
            }],
            language_stats: BTreeMap::new(),
            pattern_counts: BTreeMap::new(),
            architectural_styles: BTreeSet::new(),
            fingerprint: B3Hash::hash(path.as_bytes()),
            total_files: 1,
            total_lines: 42,
        }
    }

    #[test]
    fn directory_adapter_artifact_serialization_is_valid_safetensors() {
        let analysis = sample_analysis("src/api");
        let spec =
            directory_adapter_spec_from_analysis("tenant-a", &analysis).expect("spec generation");
        let serialized =
            serialize_lora_weights_to_safetensors(&spec.weights).expect("serialize safetensors");

        let tensors =
            safetensors::SafeTensors::deserialize(&serialized).expect("valid safetensors");
        let lora_a = tensors.tensor("lora_a").expect("lora_a tensor");
        let lora_b = tensors.tensor("lora_b").expect("lora_b tensor");

        let rank = spec.weights.lora_a.len();
        let hidden_dim = spec.weights.lora_a[0].len();
        assert_eq!(lora_a.shape(), &[rank, hidden_dim]);
        assert_eq!(lora_b.shape(), &[hidden_dim, rank]);
    }

    #[test]
    fn directory_adapter_artifact_generation_is_deterministic() {
        let analysis = sample_analysis("src/domain");
        let spec_a =
            directory_adapter_spec_from_analysis("tenant-a", &analysis).expect("spec A generation");
        let spec_b =
            directory_adapter_spec_from_analysis("tenant-a", &analysis).expect("spec B generation");

        let artifact_a = serialize_lora_weights_to_safetensors(&spec_a.weights)
            .expect("artifact A serialization");
        let artifact_b = serialize_lora_weights_to_safetensors(&spec_b.weights)
            .expect("artifact B serialization");

        assert_eq!(spec_a.adapter_id, spec_b.adapter_id);
        assert_eq!(artifact_a, artifact_b);
        assert_eq!(blake3::hash(&artifact_a), blake3::hash(&artifact_b));
    }

    #[test]
    fn directory_adapter_artifact_rewrites_placeholder_bytes() {
        let base = resolve_var_dir().join("tmp");
        std::fs::create_dir_all(&base).expect("create var/tmp for test");
        let temp_dir = tempfile::Builder::new()
            .prefix("directory_adapter_artifact_rewrite_")
            .tempdir_in(&base)
            .expect("create tempdir in var/tmp");

        let analysis = sample_analysis("src/feature");
        let spec =
            directory_adapter_spec_from_analysis("tenant-a", &analysis).expect("spec generation");
        let artifact_path = temp_dir
            .path()
            .join("tenant-a")
            .join(format!("{}.safetensors", spec.adapter_id));
        if let Some(parent) = artifact_path.parent() {
            std::fs::create_dir_all(parent).expect("create artifact parent");
        }
        std::fs::write(&artifact_path, b"synthetic adapter placeholder")
            .expect("write placeholder bytes");

        let serialized =
            serialize_lora_weights_to_safetensors(&spec.weights).expect("serialize safetensors");
        let first_write = write_adapter_artifact_if_changed(&artifact_path, &serialized)
            .expect("rewrite placeholder bytes");
        let stored = std::fs::read(&artifact_path).expect("read rewritten artifact");
        let second_write = write_adapter_artifact_if_changed(&artifact_path, &serialized)
            .expect("no-op on identical bytes");

        assert!(first_write);
        assert_eq!(stored, serialized);
        assert!(!second_write);
    }
}
