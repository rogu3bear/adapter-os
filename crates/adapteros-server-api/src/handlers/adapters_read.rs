#![allow(unused_variables)]

use crate::adapter_helpers::fetch_adapter_for_tenant;
use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*; // This already re-exports adapteros_api_types::*
use adapteros_db::{
    AdapterRepository, AdapterVersionRuntimeState,
    CreateDraftVersionParams as CreateDraftAdapterVersionParams,
    CreateRepositoryParams as CreateAdapterRepositoryParams, UpsertAdapterRepositoryPolicyParams,
};
use adapteros_types::training::LoraTier;
use serde_json::Value;
use std::collections::HashMap;
use utoipa::IntoParams;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;

fn lora_tier_from_provenance(provenance_json: &Option<String>) -> Option<LoraTier> {
    provenance_json
        .as_ref()
        .and_then(|json| serde_json::from_str::<Value>(json).ok())
        .and_then(|v| {
            v.get("lora_tier")
                .and_then(|t| t.as_str())
                .map(str::to_string)
        })
        .and_then(|s| match s.as_str() {
            "micro" => Some(LoraTier::Micro),
            "standard" => Some(LoraTier::Standard),
            "max" => Some(LoraTier::Max),
            _ => None,
        })
}

fn lora_scope_from_provenance(
    provenance_json: &Option<String>,
    fallback_scope: Option<String>,
) -> Option<String> {
    provenance_json
        .as_ref()
        .and_then(|json| serde_json::from_str::<Value>(json).ok())
        .and_then(|v| {
            v.get("lora_scope")
                .or_else(|| v.get("scope"))
                .and_then(|s| s.as_str())
                .map(str::to_string)
        })
        .or(fallback_scope)
}

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

    let limit = query.limit.unwrap_or(100).min(500);
    let offset = query.offset.unwrap_or(0);
    let start = std::time::Instant::now();

    let adapters = state
        .db
        .list_adapters_for_tenant_paged(
            &claims.tenant_id,
            Some(limit),
            Some(offset),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve adapters from database")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(format!(
                            "Database query failed for tenant '{}'. This may indicate a temporary connection issue. Try again in a moment. Technical details: {}",
                            claims.tenant_id, e
                        )),
                ),
            )
        })?;

    let mut responses = Vec::new();
    for adapter in adapters {
        // Note: list_adapters_for_tenant() already scoped to claims.tenant_id,
        // so all adapters here belong to the user's tenant. No additional validation needed.

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

        // Get stats (tenant-scoped)
        let (total, selected, avg_gate) = state
            .db
            .get_adapter_stats(&claims.tenant_id, adapter_id_str)
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
            // Codebase adapter fields
            adapter_type: None,
            base_adapter_id: None,
            stream_session_id: None,
            versioning_threshold: None,
            coreml_package_hash: None,
        });
    }

    let elapsed_ms = start.elapsed().as_millis() as f64;
    let _ = state
        .metrics_registry
        .record_metric("http.list_adapters.duration_ms".to_string(), elapsed_ms)
        .await;
    if elapsed_ms > 200.0 {
        tracing::warn!(
            tenant_id = %claims.tenant_id,
            elapsed_ms,
            limit,
            offset,
            "list_adapters exceeded latency budget (200ms)"
        );
    }

    Ok(Json(responses))
}
#[derive(Debug, Deserialize, IntoParams)]
pub struct ListAdapterRepositoriesParams {
    pub base_model_id: Option<String>,
    pub archived: Option<bool>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Create a new adapter repository
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapter-repositories",
    request_body = CreateRepositoryRequest,
    responses(
        (status = 201, description = "Repository created", body = CreateRepositoryResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 400, description = "Validation error", body = ErrorResponse)
    )
)]
pub async fn create_adapter_repository(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateRepositoryRequest>,
) -> Result<(StatusCode, Json<CreateRepositoryResponse>), (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;
    validate_tenant_isolation(&claims, &req.tenant_id)?;

    let repo_id = state
        .db
        .create_adapter_repository(CreateAdapterRepositoryParams {
            tenant_id: &claims.tenant_id,
            name: &req.name,
            base_model_id: req.base_model_id.as_deref(),
            default_branch: req.default_branch.as_deref(),
            created_by: Some(&claims.sub),
            description: req.description.as_deref(),
        })
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Failed to create adapter repository")
                        .with_code("REPOSITORY_CREATION_FAILED")
                        .with_string_details(format!(
                            "Repository '{}' could not be created for tenant '{}'. Common causes: duplicate repository name, invalid base model ID, or database constraint violation. Technical details: {}",
                            req.name, claims.tenant_id, e
                        )),
                ),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(CreateRepositoryResponse { repo_id }),
    ))
}

/// Get repository metadata
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/adapter-repositories/{repo_id}",
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    responses(
        (status = 200, description = "Repository", body = AdapterRepositoryResponse),
        (status = 404, description = "Not found", body = ErrorResponse)
    )
)]
pub async fn get_adapter_repository(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
) -> Result<Json<AdapterRepositoryResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterList)?;

    let repo = state
        .db
        .get_adapter_repository(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch repository")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let repo = match repo {
        Some(r) => r,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Adapter repository not found")
                        .with_code("REPOSITORY_NOT_FOUND")
                        .with_string_details(format!(
                            "Repository '{}' does not exist for tenant '{}'. Check the repository ID and ensure it was created successfully.",
                            repo_id, claims.tenant_id
                        )),
                ),
            ))
        }
    };

    let policy = state
        .db
        .get_adapter_repository_policy(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve repository training policy")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(format!(
                            "Could not load training policy for repository '{}'. The repository exists but policy metadata is inaccessible. Technical details: {}",
                            repo_id, e
                        )),
                ),
            )
        })?;

    let training_policy = policy.map(|p| AdapterRepositoryPolicyResponse {
        repo_id: p.repo_id,
        preferred_backends: p
            .preferred_backends_json
            .as_ref()
            .and_then(|json| serde_json::from_str(json).ok()),
        coreml_allowed: p.coreml_allowed,
        coreml_required: p.coreml_required,
        autopromote_coreml: p.autopromote_coreml,
        coreml_mode: p.coreml_mode,
        repo_tier: p.repo_tier,
        auto_rollback_on_trust_regress: p.auto_rollback_on_trust_regress,
        created_at: p.created_at,
    });

    Ok(Json(AdapterRepositoryResponse {
        id: repo.id,
        tenant_id: repo.tenant_id,
        name: repo.name,
        base_model_id: repo.base_model_id,
        default_branch: repo.default_branch,
        archived: repo.archived != 0,
        created_by: repo.created_by,
        created_at: repo.created_at,
        description: repo.description,
        training_policy,
    }))
}

/// Upsert repository training policy (backend/coreml preferences)
#[utoipa::path(
    tag = "system",
    put,
    path = "/v1/adapter-repositories/{repo_id}/policy",
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    request_body = AdapterRepositoryPolicyRequest,
    responses(
        (status = 200, description = "Policy updated", body = AdapterRepositoryPolicyResponse),
        (status = 404, description = "Repository not found", body = ErrorResponse)
    )
)]
pub async fn upsert_adapter_repository_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
    Json(req): Json<AdapterRepositoryPolicyRequest>,
) -> Result<Json<AdapterRepositoryPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;

    let repo = state
        .db
        .get_adapter_repository(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch repository")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if repo.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("repository not found").with_code("NOT_FOUND")),
        ));
    }

    let preferred_backends_json = req
        .preferred_backends
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok());

    state
        .db
        .upsert_adapter_repository_policy(UpsertAdapterRepositoryPolicyParams {
            repo_id: &repo_id,
            tenant_id: &claims.tenant_id,
            preferred_backends_json: preferred_backends_json.as_deref(),
            coreml_allowed: req.coreml_allowed,
            coreml_required: req.coreml_required,
            autopromote_coreml: req.autopromote_coreml,
            coreml_mode: req.coreml_mode,
            repo_tier: req.repo_tier,
            auto_rollback_on_trust_regress: req.auto_rollback_on_trust_regress,
        })
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to update repository training policy")
                        .with_code("POLICY_UPDATE_FAILED")
                        .with_string_details(format!(
                            "Training policy for repository '{}' could not be updated. Verify that backend preferences and CoreML settings are valid. Technical details: {}",
                            repo_id, e
                        )),
                ),
            )
        })?;

    let policy = state
        .db
        .get_adapter_repository_policy(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch repository policy")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("policy not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(AdapterRepositoryPolicyResponse {
        repo_id: policy.repo_id,
        preferred_backends: policy
            .preferred_backends_json
            .as_ref()
            .and_then(|json| serde_json::from_str(json).ok()),
        coreml_allowed: policy.coreml_allowed,
        coreml_required: policy.coreml_required,
        autopromote_coreml: policy.autopromote_coreml,
        coreml_mode: policy.coreml_mode,
        repo_tier: policy.repo_tier,
        auto_rollback_on_trust_regress: policy.auto_rollback_on_trust_regress,
        created_at: policy.created_at,
    }))
}

/// Get repository training policy
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/adapter-repositories/{repo_id}/policy",
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    responses(
        (status = 200, description = "Policy", body = AdapterRepositoryPolicyResponse),
        (status = 404, description = "Not found", body = ErrorResponse)
    )
)]
pub async fn get_adapter_repository_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
) -> Result<Json<AdapterRepositoryPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterList)?;

    let repo = state
        .db
        .get_adapter_repository(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch repository")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if repo.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("repository not found").with_code("NOT_FOUND")),
        ));
    }

    let policy = state
        .db
        .get_adapter_repository_policy(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch repository policy")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("policy not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(AdapterRepositoryPolicyResponse {
        repo_id: policy.repo_id,
        preferred_backends: policy
            .preferred_backends_json
            .as_ref()
            .and_then(|json| serde_json::from_str(json).ok()),
        coreml_allowed: policy.coreml_allowed,
        coreml_required: policy.coreml_required,
        autopromote_coreml: policy.autopromote_coreml,
        coreml_mode: policy.coreml_mode,
        repo_tier: policy.repo_tier,
        auto_rollback_on_trust_regress: policy.auto_rollback_on_trust_regress,
        created_at: policy.created_at,
    }))
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
                    ErrorResponse::new("Failed to archive adapter repository")
                        .with_code("ARCHIVE_FAILED")
                        .with_string_details(format!(
                            "Repository '{}' could not be archived. This operation marks the repository as inactive. Technical details: {}",
                            repo_id, e
                        )),
                ),
            )
        })?;

    if !archived {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Adapter repository not found")
                    .with_code("REPOSITORY_NOT_FOUND")
                    .with_string_details(format!(
                        "Repository '{}' does not exist for tenant '{}'. Cannot archive non-existent repository.",
                        repo_id, claims.tenant_id
                    )),
            ),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// List adapter repositories for the caller's tenant
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/adapter-repositories",
    params(ListAdapterRepositoriesParams),
    responses(
        (status = 200, description = "List of adapter repositories", body = Vec<AdapterRepositoryResponse>),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    )
)]
pub async fn list_adapter_repositories(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<ListAdapterRepositoriesParams>,
) -> Result<Json<Vec<AdapterRepositoryResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterList)?;

    let limit = params.limit.unwrap_or(100).min(500);
    let offset = params.offset.unwrap_or(0);
    let start = std::time::Instant::now();

    let repos = state
        .db
        .list_adapter_repositories_paged(
            &claims.tenant_id,
            params.base_model_id.as_deref(),
            params.archived,
            Some(limit),
            Some(offset),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve adapter repositories")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(format!(
                            "Could not list repositories for tenant '{}'. This may indicate a temporary database issue. Try again in a moment. Technical details: {}",
                            claims.tenant_id, e
                        )),
                ),
            )
        })?;

    let responses = repos
        .into_iter()
        .map(|repo: AdapterRepository| AdapterRepositoryResponse {
            id: repo.id,
            tenant_id: repo.tenant_id,
            name: repo.name,
            base_model_id: repo.base_model_id,
            default_branch: repo.default_branch,
            archived: repo.archived != 0,
            created_by: repo.created_by,
            created_at: repo.created_at,
            description: repo.description,
            training_policy: None,
        })
        .collect();

    let elapsed_ms = start.elapsed().as_millis() as f64;
    let _ = state
        .metrics_registry
        .record_metric(
            "http.list_adapter_repositories.duration_ms".to_string(),
            elapsed_ms,
        )
        .await;
    if elapsed_ms > 200.0 {
        tracing::warn!(
            tenant_id = %claims.tenant_id,
            elapsed_ms,
            limit,
            offset,
            "list_adapter_repositories exceeded latency budget (200ms)"
        );
    }

    Ok(Json(responses))
}

#[deprecated(note = "Use /v1/adapter-repositories instead.")]
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/repositories",
    params(ListAdapterRepositoriesParams),
    responses(
        (status = 200, description = "List of adapter repositories", body = Vec<AdapterRepositoryResponse>),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    )
)]
pub async fn list_repositories_legacy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<ListAdapterRepositoriesParams>,
) -> Result<Json<Vec<AdapterRepositoryResponse>>, (StatusCode, Json<ErrorResponse>)> {
    list_adapter_repositories(State(state), Extension(claims), Query(params)).await
}

/// Extract manifest lineage fields from a packaged .aos artifact.
async fn manifest_lineage_from_aos(
    aos_path: Option<&str>,
) -> Option<(Option<Vec<String>>, Option<String>, Option<String>)> {
    let path = aos_path?;
    let data = tokio::fs::read(path).await.ok()?;
    let file_view = adapteros_aos::open_aos(&data).ok()?;
    let manifest: serde_json::Value = serde_json::from_slice(file_view.manifest_bytes).ok()?;

    let dataset_version_ids = manifest
        .get("dataset_version_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<String>>()
        })
        .filter(|ids| !ids.is_empty());

    let training_backend = manifest
        .get("training_backend")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            manifest
                .get("metadata")
                .and_then(|m| m.get("training_backend"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

    let scope_path = manifest
        .get("metadata")
        .and_then(|m| m.get("scope_path"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some((dataset_version_ids, training_backend, scope_path))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListAdapterVersionsParams {
    pub branch: Option<String>,
    pub state: Option<String>,
}

fn compute_serveable_state(release_state: &str, trust_state: &str) -> (bool, Option<String>) {
    let release_norm = release_state.trim().to_ascii_lowercase();
    if release_norm != "active" && release_norm != "ready" {
        return (
            false,
            Some(format!("release_state={} not serveable", release_state)),
        );
    }
    let trust_norm = trust_state.trim().to_ascii_lowercase();
    if matches!(
        trust_norm.as_str(),
        "blocked" | "blocked_regressed" | "needs_approval" | "unknown"
    ) {
        return (
            false,
            Some(format!("trust_state={} not serveable", trust_state)),
        );
    }
    (true, None)
}

/// List adapter versions for a repository
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/adapter-repositories/{repo_id}/versions",
    params(
        ("repo_id" = String, Path, description = "Repository ID"),
        ListAdapterVersionsParams
    ),
    responses(
        (status = 200, description = "List of adapter versions", body = Vec<AdapterVersionResponse>),
        (status = 404, description = "Repository not found", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    )
)]
pub async fn list_adapter_versions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
    Query(params): Query<ListAdapterVersionsParams>,
) -> Result<Json<Vec<AdapterVersionResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterList)?;

    let repo = state
        .db
        .get_adapter_repository(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch repository")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if repo.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("repository not found").with_code("NOT_FOUND")),
        ));
    }

    let state_filter: Option<Vec<String>> = params.state.as_ref().map(|s| vec![s.clone()]);
    let state_refs: Option<Vec<&str>> = state_filter
        .as_ref()
        .map(|vals| vals.iter().map(|s| s.as_str()).collect());

    let versions = state
        .db
        .list_adapter_versions_for_repo(
            &claims.tenant_id,
            &repo_id,
            params.branch.as_deref(),
            state_refs.as_deref(),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve adapter versions")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(format!(
                            "Could not list versions for repository '{}'. Verify the repository exists and is accessible. Technical details: {}",
                            repo_id, e
                        )),
                ),
            )
        })?;

    let mut responses = Vec::new();
    for version in versions {
        let runtime_state = state
            .db
            .get_adapter_version_runtime_state(&version.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to load runtime state")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
            .map(|s: AdapterVersionRuntimeState| s.runtime_state);

        let db_lineage = state
            .db
            .list_dataset_versions_with_trust_for_adapter_version(&version.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to load dataset versions")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        let manifest_lineage = manifest_lineage_from_aos(version.aos_path.as_deref()).await;
        let manifest_ids = manifest_lineage
            .as_ref()
            .and_then(|(ids, _, _)| ids.clone());
        let manifest_training_backend = manifest_lineage.as_ref().and_then(|(_, tb, _)| tb.clone());
        let manifest_scope_path = manifest_lineage
            .as_ref()
            .and_then(|(_, _, scope)| scope.clone());

        let db_ids: Vec<String> = db_lineage.iter().map(|(id, _)| id.clone()).collect();
        if let Some(ref manifest_ids) = manifest_ids {
            if !db_ids.is_empty() && manifest_ids != &db_ids {
                tracing::warn!(
                    version_id = %version.id,
                    manifest_ids = ?manifest_ids,
                    db_dataset_version_ids = ?db_ids,
                    "Manifest lineage differs from DB mirror; preferring manifest"
                );
            }
        }

        let trust_map: HashMap<String, Option<String>> = db_lineage.into_iter().collect();

        let dataset_version_ids = manifest_ids.filter(|ids| !ids.is_empty()).or_else(|| {
            if db_ids.is_empty() {
                None
            } else {
                Some(db_ids.clone())
            }
        });

        let dataset_version_trust = dataset_version_ids.as_ref().map(|ids| {
            ids.iter()
                .map(|id| DatasetVersionTrustSnapshot {
                    dataset_version_id: id.clone(),
                    trust_at_training_time: trust_map.get(id).cloned().flatten(),
                })
                .collect()
        });

        let (serveable, serveable_reason) =
            compute_serveable_state(&version.release_state, &version.adapter_trust_state);

        responses.push(AdapterVersionResponse {
            id: version.id,
            repo_id: version.repo_id,
            tenant_id: version.tenant_id,
            version: version.version,
            branch: version.branch,
            aos_path: version.aos_path,
            aos_hash: version.aos_hash,
            manifest_schema_version: version.manifest_schema_version,
            parent_version_id: version.parent_version_id,
            code_commit_sha: version.code_commit_sha,
            data_spec_hash: version.data_spec_hash,
            training_backend: manifest_training_backend
                .clone()
                .or_else(|| version.training_backend.clone()),
            coreml_used: Some(version.coreml_used),
            coreml_device_type: version.coreml_device_type,
            dataset_version_ids,
            scope_path: manifest_scope_path,
            dataset_version_trust,
            adapter_trust_state: version.adapter_trust_state,
            release_state: version.release_state,
            metrics_snapshot_id: version.metrics_snapshot_id,
            evaluation_summary: version.evaluation_summary,
            created_at: version.created_at,
            runtime_state,
            serveable,
            serveable_reason,
        });
    }

    Ok(Json(responses))
}

/// Create a draft version for a repository
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapter-versions/draft",
    request_body = CreateDraftVersionRequest,
    responses(
        (status = 201, description = "Draft version created", body = CreateDraftVersionResponse),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 404, description = "Repository not found", body = ErrorResponse)
    )
)]
pub async fn create_draft_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateDraftVersionRequest>,
) -> Result<(StatusCode, Json<CreateDraftVersionResponse>), (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;

    // Ensure repository is owned by tenant
    let repo = state
        .db
        .get_adapter_repository(&claims.tenant_id, &req.repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch repository")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if repo.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("repository not found").with_code("NOT_FOUND")),
        ));
    }

    let version_id = state
        .db
        .create_adapter_draft_version(CreateDraftAdapterVersionParams {
            repo_id: &req.repo_id,
            tenant_id: &claims.tenant_id,
            branch: &req.branch,
            branch_classification: "protected",
            parent_version_id: req.parent_version_id.as_deref(),
            code_commit_sha: req.code_commit_sha.as_deref(),
            data_spec_hash: req.data_spec_hash.as_deref(),
            training_backend: None,
            dataset_version_ids: None,
            actor: Some(&claims.sub),
            reason: None,
        })
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Failed to create draft adapter version")
                        .with_code("VERSION_CREATION_FAILED")
                        .with_string_details(format!(
                            "Draft version for repository '{}' on branch '{}' could not be created. Common causes: invalid parent version ID, branch naming conflict, or database constraint violation. Technical details: {}",
                            req.repo_id, req.branch, e
                        )),
                ),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(CreateDraftVersionResponse { version_id }),
    ))
}

/// Get adapter version details
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/adapter-versions/{version_id}",
    params(
        ("version_id" = String, Path, description = "Version ID")
    ),
    responses(
        (status = 200, description = "Version", body = AdapterVersionResponse),
        (status = 404, description = "Version not found", body = ErrorResponse)
    )
)]
pub async fn get_adapter_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(version_id): Path<String>,
) -> Result<Json<AdapterVersionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterList)?;

    let version = state
        .db
        .get_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve adapter version")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(format!(
                            "Adapter version '{}' could not be loaded. Verify the version ID is correct and accessible. Technical details: {}",
                            version_id, e
                        )),
                ),
            )
        })?;

    let version = match version {
        Some(v) => v,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Adapter version not found")
                        .with_code("VERSION_NOT_FOUND")
                        .with_string_details(format!(
                            "Version '{}' does not exist for tenant '{}'. Check the version ID or list available versions using GET /v1/adapter-repositories/{{repo_id}}/versions",
                            version_id, claims.tenant_id
                        ))
                ),
            ))
        }
    };

    let db_lineage = state
        .db
        .list_dataset_versions_with_trust_for_adapter_version(&version.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to load training dataset lineage")
                        .with_code("LINEAGE_LOAD_FAILED")
                        .with_string_details(format!(
                            "Dataset version metadata for adapter version '{}' could not be retrieved. Training data provenance is unavailable. Technical details: {}",
                            version.id, e
                        )),
                ),
            )
        })?;

    let manifest_lineage = manifest_lineage_from_aos(version.aos_path.as_deref()).await;
    let manifest_ids = manifest_lineage
        .as_ref()
        .and_then(|(ids, _, _)| ids.clone());
    let manifest_training_backend = manifest_lineage.as_ref().and_then(|(_, tb, _)| tb.clone());
    let manifest_scope_path = manifest_lineage
        .as_ref()
        .and_then(|(_, _, scope)| scope.clone());

    let db_ids: Vec<String> = db_lineage.iter().map(|(id, _)| id.clone()).collect();
    if let Some(ref manifest_ids) = manifest_ids {
        if !db_ids.is_empty() && manifest_ids != &db_ids {
            tracing::warn!(
                version_id = %version.id,
                manifest_ids = ?manifest_ids,
                db_dataset_version_ids = ?db_ids,
                "Manifest lineage differs from DB mirror; preferring manifest"
            );
        }
    }

    let trust_map: HashMap<String, Option<String>> = db_lineage.into_iter().collect();

    let dataset_version_ids = manifest_ids.filter(|ids| !ids.is_empty()).or_else(|| {
        if db_ids.is_empty() {
            None
        } else {
            Some(db_ids.clone())
        }
    });

    let dataset_version_trust = dataset_version_ids.as_ref().map(|ids| {
        ids.iter()
            .map(|id| DatasetVersionTrustSnapshot {
                dataset_version_id: id.clone(),
                trust_at_training_time: trust_map.get(id).cloned().flatten(),
            })
            .collect()
    });

    let (serveable, serveable_reason) =
        compute_serveable_state(&version.release_state, &version.adapter_trust_state);

    Ok(Json(AdapterVersionResponse {
        id: version.id,
        repo_id: version.repo_id,
        tenant_id: version.tenant_id,
        version: version.version,
        branch: version.branch,
        aos_path: version.aos_path,
        aos_hash: version.aos_hash,
        manifest_schema_version: version.manifest_schema_version,
        parent_version_id: version.parent_version_id,
        code_commit_sha: version.code_commit_sha,
        data_spec_hash: version.data_spec_hash,
        training_backend: manifest_training_backend
            .clone()
            .or_else(|| version.training_backend.clone()),
        coreml_used: Some(version.coreml_used),
        coreml_device_type: version.coreml_device_type,
        dataset_version_ids,
        scope_path: manifest_scope_path,
        dataset_version_trust,
        adapter_trust_state: version.adapter_trust_state,
        release_state: version.release_state,
        metrics_snapshot_id: version.metrics_snapshot_id,
        evaluation_summary: version.evaluation_summary,
        created_at: version.created_at,
        runtime_state: None,
        serveable,
        serveable_reason,
    }))
}

/// Promote a version on its branch
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapter-versions/{version_id}/promote",
    request_body = PromoteVersionRequest,
    responses(
        (status = 204, description = "Version promoted"),
        (status = 404, description = "Version not found", body = ErrorResponse)
    )
)]
pub async fn promote_adapter_version_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> ApiResult<AdapterResponse> {
    let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    let (total, selected, avg_gate) = state
        .db
        .get_adapter_stats(&claims.tenant_id, &adapter_id)
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
        // Codebase adapter fields
        adapter_type: None,
        base_adapter_id: None,
        stream_session_id: None,
        versioning_threshold: None,
        coreml_package_hash: None,
    }))
}
